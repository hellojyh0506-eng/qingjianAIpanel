//! 按消息类型分派：会话开关、按键、轮询、失焦上屏、选区 / 光标矩形 / 中英模式的通知。

use qingjian_platform::protocol::{
    ClientMessage, Frame, KeyEvent, KeyOutcome, PROTOCOL_VERSION, SESSION_OPENED_SINCE,
    ServerMessage, SessionId,
};

use super::Router;
use super::key::Effect;
use super::session::SessionInfo;

impl Router {
    pub(super) fn dispatch(&mut self, message: ClientMessage) -> Option<ServerMessage> {
        match message {
            ClientMessage::OpenSession {
                session,
                app,
                protocol,
            } => {
                tracing::debug!(?session, app, protocol, "会话打开");
                if protocol != PROTOCOL_VERSION {
                    tracing::warn!(
                        ?session,
                        app,
                        dll = protocol,
                        server = PROTOCOL_VERSION,
                        "DLL 与 Server 的协议版本不同（应用还没重启、用着旧 DLL？），照常服务"
                    );
                }
                // 同一会话重开（DLL 断线重连）：从干净状态起。
                if self.focused == Some(session) {
                    self.reset_composition();
                    self.focused = None;
                }
                self.sessions.insert(
                    session,
                    SessionInfo {
                        app,
                        private: false,
                        protocol,
                    },
                );
                // 按键行为设置回一次，让 DLL 不必自己读配置文件。**只回给会读这条回包的 DLL**：
                // 更老的 DLL 的 `open` 是只写不读，多回一条会被它当成下一次 `Poll` 的应答而报错，
                // 那条连接就废了（老 DLL 在没重启的应用里还会活很久）。它们从 `ModeSync` 那一拍
                // 也能拿到同一份（新字段它直接忽略），只是慢一拍。
                (protocol >= SESSION_OPENED_SINCE).then(|| ServerMessage::SessionOpened {
                    session,
                    input: self.input_settings(),
                })
            }
            ClientMessage::Key { session, event } => Some(self.handle_key(session, event)),
            ClientMessage::Poll { session } => Some(self.handle_poll(session)),
            ClientMessage::Commit { session } => {
                let text = self.commit_raw_for(session);
                tracing::debug!(?session, ?text, "焦点离开，结束组句");
                Some(ServerMessage::Committed { session, text })
            }
            ClientMessage::Surrounding { session, text } => {
                tracing::trace!(?session, chars = text.chars().count(), "收到光标前文");
                self.set_surrounding(session, text);
                None
            }
            ClientMessage::Privacy { session, private } => {
                tracing::debug!(?session, private, "输入框私密状态");
                self.set_privacy(session, private);
                None
            }
            ClientMessage::Selection {
                session,
                request,
                text,
                rect,
            } => Some(self.handle_selection(session, request, text, rect)),
            ClientMessage::PositionCandidates { session, rect } => {
                self.position_candidates(session, rect);
                None
            }
            ClientMessage::HideCandidates { session } => {
                // 组句在 DLL 侧结束（应用终止组句 / 翻译评审失焦）：只收窗口；缓冲留给下一键的 Commit 清。
                if self.focused == Some(session) {
                    self.end_translation();
                    self.end_recall_session();
                    self.hide_candidate_window();
                }
                None
            }
            ClientMessage::ModeChanged { session, english } => {
                tracing::debug!(?session, english, "中英模式");
                self.handle_mode_changed(english);
                None
            }
            ClientMessage::SyncMode { session } => {
                self.handle_ime_active();
                Some(ServerMessage::ModeSync {
                    session,
                    english: Some(self.english),
                    input: self.input_settings(),
                    indicator: self.indicator_state(),
                })
            }
            ClientMessage::ImeSwitched { session } => {
                tracing::debug!(?session, "切成了别的输入法");
                self.handle_ime_switched();
                None
            }
            ClientMessage::Indicator { session, command } => {
                tracing::debug!(?session, ?command, "任务栏图标菜单");
                self.handle_indicator(command);
                None
            }
            ClientMessage::CloseSession { session } => {
                self.sessions.remove(&session);
                if self.focused == Some(session) {
                    self.reset_composition();
                    self.focused = None;
                }
                self.flush_learning();
                tracing::debug!(?session, "会话关闭");
                None
            }
        }
    }

    fn handle_key(&mut self, session: SessionId, event: KeyEvent) -> ServerMessage {
        self.ensure_focus(session);
        self.notice = None;
        if self.translation.is_some() {
            return self.handle_translation_review(session, &event);
        }
        if self.recall_session.is_some() {
            return self.handle_recall_session(session, &event);
        }
        if self.engine.composition().is_empty()
            && self.matches_recall_combo(&event)
        {
            return self.start_recall_session(session);
        }
        if self.matches_learning_mode_combo(&event) {
            let next = self.learning_mode.next();
            self.learning_mode = next;
            self.config.learning_mode = next;
            tracing::info!(mode = ?next, "快捷键：切换英语学习模式");
            self.persist("general", "learning_mode", next.key());
            self.notice = Some(format!("已切换至【{}】", next.label()));
            if next == qingjian_platform::LearningMode::Learn && self.composing() {
                self.on_candidate_selection_changed();
            } else if next == qingjian_platform::LearningMode::Daily {
                self.passive_learning_card = None;
            } else if next == qingjian_platform::LearningMode::Drill && self.composing() {
                self.start_drill_session();
            }
            self.reconcile_status();
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            };
        }
        if self.engine.composition().is_empty()
            && self.engine.prediction_enabled()
            && self.matches_translate_combo(&event)
        {
            self.selection_seq += 1;
            self.pending_selection = Some(self.selection_seq);
            tracing::debug!(
                ?session,
                request = self.selection_seq,
                "翻译选中文字：请 DLL 读选区"
            );
            return ServerMessage::RequestSelection {
                session,
                request: self.selection_seq,
            };
        }
        if self.matches_learning_expand_combo(&event) {
            if self.composing() {
                match self.learning_mode {
                    qingjian_platform::LearningMode::Daily => {
                        self.daily_show_english = !self.daily_show_english;
                        if self.daily_show_english {
                            let target = self
                                .layout_candidate(self.highlight)
                                .map(|c| c.text)
                                .or_else(|| self.layout_candidate(0).map(|c| c.text));
                            if let Some(target) = target {
                                self.passive_translation_target = Some(target.clone());
                                if self.engine.prediction_enabled() {
                                    let due = self.today_due_count();
                                    self.passive_learning_card = Some(qingjian_platform::protocol::LearningCardFrame {
                                        target_text: target.clone(),
                                        state: qingjian_platform::protocol::LearningState::Loading,
                                        primary: None,
                                        alternatives: Vec::new(),
                                        expanded: false,
                                        selected_index: None,
                                        drill_stage: None,
                                        drill_input: String::new(),
                                        drill_attempt: None,
                                        drill_notice: None,
                                        today_due_count: due,
                                        review_current_index: 0,
                                        review_total_count: due,
                                        upgrade: None,
                                    });
                                    self.translation_start_time = Some(std::time::Instant::now());
                                    if let Some(seq) = self.engine.request_translation(&target) {
                                        self.generation_id = seq;
                                    }
                                }
                            }
                        } else {
                            self.generation_id += 1;
                            self.passive_learning_card = None;
                            self.passive_translation = None;
                            self.passive_translation_target = None;
                        }
                    }
                    qingjian_platform::LearningMode::Learn => {
                        if let Some(card) = self.passive_learning_card.as_mut() {
                            card.expanded = !card.expanded;
                            if !card.expanded {
                                card.selected_index = None;
                            }
                        }
                    }
                    qingjian_platform::LearningMode::Drill => {
                        if let Some(card) = self.passive_learning_card.as_mut() {
                            let reference = card
                                .primary
                                .as_ref()
                                .map(|p| p.text.as_str())
                                .unwrap_or("");
                            card.drill_notice = Some(format!("参考答案: {}", reference));
                        }
                    }
                }
            } else {
                if self.learning_mode == qingjian_platform::LearningMode::Daily {
                    self.daily_show_english = !self.daily_show_english;
                    self.notice = Some(if self.daily_show_english {
                        "已开启日常英文辅助 (Ctrl+Shift+E)".to_string()
                    } else {
                        "已关闭日常英文辅助 (Ctrl+Shift+E)".to_string()
                    });
                    self.reconcile_status();
                }
            }
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            };
        }
        if self.matches_learning_drill_combo(&event) {
            if self.composing() {
                self.start_drill_session();
            } else {
                self.learning_mode = qingjian_platform::LearningMode::Drill;
                self.config.learning_mode = qingjian_platform::LearningMode::Drill;
                self.persist("general", "learning_mode", qingjian_platform::LearningMode::Drill.key());
                self.notice = Some("已切换至【主动回忆】".to_string());
                self.reconcile_status();
            }
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            };
        }
        if self.composing() && self.matches_learning_review_combo(&event) {
            if let Some(card) = self.passive_learning_card.as_mut() {
                if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                    let vault = qingjian_learning::LearningVault::open(&user_dir);
                    let now = jiff::Zoned::now().timestamp().as_second();
                    let due_list = vault.today_due_reviews(now);
                    if due_list.is_empty() {
                        card.drill_notice = Some("🎉 今日暂无到期复习！记忆状态良好。".to_string());
                    } else {
                        let first = due_list[0];
                        card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Review);
                        card.today_due_count = due_list.len();
                        card.review_current_index = 1;
                        card.review_total_count = due_list.len();
                        card.target_text = first.prompt.clone();
                        card.primary = Some(qingjian_platform::protocol::LearningExpression {
                            text: first.expression.clone(),
                            ..Default::default()
                        });
                        card.drill_input.clear();
                        card.drill_attempt = None;
                        card.drill_notice = None;
                    }
                }
            }
            self.reconcile_status();
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            };
        }
        // Active Recall: 若候选卡片处于 Drill 输入或对比展示交互态，拦截按键并隔离到卡片本身，绝不污染中文组句
        if self.composing() {
            if let Some(card) = self.passive_learning_card.as_mut() {
                if let Some(stage) = card.drill_stage {
                    match stage {
                        qingjian_platform::protocol::DrillStage::Input => {
                            if event.virtual_key == super::key::ESCAPE {
                                self.end_drill_session();
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if event.virtual_key == super::key::RETURN {
                                let input = card.drill_input.trim();
                                if input.is_empty() {
                                    card.drill_notice = Some("请输入英文表达进行对比".to_string());
                                } else {
                                    let reference = card
                                        .primary
                                        .as_ref()
                                        .map(|p| p.text.as_str())
                                        .unwrap_or("");
                                    card.drill_stage =
                                        Some(qingjian_platform::protocol::DrillStage::Comparing);
                                    card.drill_notice = None;
                                    self.compare_start_time = Some(std::time::Instant::now());
                                    self.engine.request_compare(
                                        &card.target_text,
                                        input,
                                        reference,
                                    );
                                }
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            let chord = event.modifiers.chord();
                            let is_reveal = event.virtual_key == super::key::TAB
                                || (chord.ctrl && chord.shift && !chord.alt && !chord.win
                                    && (event.character.map(|c| c.to_ascii_lowercase()) == Some('r') || event.virtual_key == 0x52));
                            if is_reveal {
                                let reference = card
                                    .primary
                                    .as_ref()
                                    .map(|p| p.text.as_str())
                                    .unwrap_or("");
                                card.drill_notice = Some(format!("参考答案: {}", reference));
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if event.virtual_key == super::key::BACK {
                                card.drill_input.pop();
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if let Some(c) = event.character {
                                if !c.is_control() {
                                    card.drill_input.push(c);
                                    card.drill_notice = None;
                                    let shown = self.self_drawn_frame();
                                    self.reconcile_candidates(&shown);
                                    return ServerMessage::KeyResult {
                                        session,
                                        outcome: KeyOutcome::Consumed,
                                        commit: None,
                                        frame: self.current_frame(),
                                    };
                                }
                            }
                            return ServerMessage::KeyResult {
                                session,
                                outcome: KeyOutcome::Consumed,
                                commit: None,
                                frame: self.current_frame(),
                            };
                        }
                        qingjian_platform::protocol::DrillStage::Comparing => {
                            if event.virtual_key == super::key::ESCAPE {
                                card.drill_stage = None;
                            }
                            let shown = self.self_drawn_frame();
                            self.reconcile_candidates(&shown);
                            return ServerMessage::KeyResult {
                                session,
                                outcome: KeyOutcome::Consumed,
                                commit: None,
                                frame: self.current_frame(),
                            };
                        }
                        qingjian_platform::protocol::DrillStage::Compared => {
                            let is_retry = event.virtual_key == super::key::TAB
                                || event.virtual_key == 0x52
                                || event.character.map(|c| c.to_ascii_lowercase()) == Some('r');
                            if is_retry {
                                card.drill_stage =
                                    Some(qingjian_platform::protocol::DrillStage::Input);
                                card.drill_input.clear();
                                card.drill_notice = None;
                                card.drill_attempt = None;
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            let is_save = event.virtual_key == 0x53
                                || (event.modifiers.ctrl
                                    && event.character.map(|c| c.to_ascii_lowercase()) == Some('s'))
                                || event.character.map(|c| c.to_ascii_lowercase()) == Some('s');
                            if is_save {
                                if let Some(attempt) = &card.drill_attempt {
                                    self.engine.record_recall_attempt(
                                        self.engine.learning_language(),
                                        attempt,
                                    );
                                }
                                if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                                    let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                                    let saved_expr = card
                                        .drill_attempt
                                        .as_ref()
                                        .map(|a| a.comparison.natural_expression.as_str())
                                        .or_else(|| card.primary.as_ref().map(|p| p.text.as_str()))
                                        .unwrap_or("");
                                    let tag = card.primary.as_ref().map(|p| p.tag.as_str());
                                    vault.save_expression(&card.target_text, saved_expr, tag, None);
                                    let _ = vault.flush();
                                    card.drill_notice = Some(format!("⭐ 已保存到表达本: {}", saved_expr));
                                } else {
                                    card.drill_notice = Some("⭐ 已保存到表达本".to_string());
                                }
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if event.virtual_key == 0x50
                                || event.character.map(|c| c.to_ascii_lowercase()) == Some('p')
                            {
                                let profile_text = if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                                    let vault = qingjian_learning::LearningVault::open(&user_dir);
                                    vault.calculate_profile().format_ascii_bars()
                                } else {
                                    "暂无学习画像数据".to_string()
                                };
                                card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Profile);
                                card.drill_notice = Some(profile_text);
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if event.virtual_key == super::key::ESCAPE
                                || event.virtual_key == super::key::RETURN
                            {
                                self.end_drill_session();
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            return ServerMessage::KeyResult {
                                session,
                                outcome: KeyOutcome::Consumed,
                                commit: None,
                                frame: self.current_frame(),
                            };
                        }
                        qingjian_platform::protocol::DrillStage::Profile => {
                            if event.character.map(|c| c.to_ascii_lowercase()) == Some('c') {
                                if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                                    let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                                    let _ = vault.clear_all();
                                }
                                card.drill_notice = Some("已彻底清空个人学习画像与错题记录。\n请按 Esc 返回。".to_string());
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if event.virtual_key == super::key::ESCAPE
                                || event.virtual_key == super::key::RETURN
                            {
                                self.end_drill_session();
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            return ServerMessage::KeyResult {
                                session,
                                outcome: KeyOutcome::Consumed,
                                commit: None,
                                frame: self.current_frame(),
                            };
                        }
                        qingjian_platform::protocol::DrillStage::Review => {
                            if event.virtual_key == super::key::ESCAPE {
                                self.end_drill_session();
                                let shown = self.self_drawn_frame();
                                self.reconcile_candidates(&shown);
                                return ServerMessage::KeyResult {
                                    session,
                                    outcome: KeyOutcome::Consumed,
                                    commit: None,
                                    frame: self.current_frame(),
                                };
                            }
                            if card.drill_attempt.is_none() {
                                if event.virtual_key == super::key::RETURN {
                                    if card
                                        .drill_notice
                                        .as_ref()
                                        .map(|n| n.contains("全部完成"))
                                        .unwrap_or(false)
                                    {
                                        self.end_drill_session();
                                    } else {
                                        let input = card.drill_input.trim();
                                        if input.is_empty() {
                                            card.drill_notice =
                                                Some("请输入英文表达进行复习对比".to_string());
                                        } else {
                                            let reference = card
                                                .primary
                                                .as_ref()
                                                .map(|p| p.text.as_str())
                                                .unwrap_or("");
                                            let attempt =
                                                qingjian_learning::RecallBook::evaluate_attempt(
                                                    &card.target_text,
                                                    input,
                                                    reference,
                                                );
                                            if let Some(user_dir) =
                                                qingjian_platform::dirs::user_dir()
                                            {
                                                let mut vault =
                                                    qingjian_learning::LearningVault::open(&user_dir);
                                                vault.record_attempt(&attempt);
                                                let _ = vault.flush();
                                            }
                                            card.drill_attempt = Some(attempt);
                                            card.drill_notice = None;
                                        }
                                    }
                                } else if event.virtual_key == super::key::TAB {
                                    let reference = card
                                        .primary
                                        .as_ref()
                                        .map(|p| p.text.as_str())
                                        .unwrap_or("");
                                    card.drill_notice = Some(format!("参考答案: {}", reference));
                                } else if event.virtual_key == super::key::BACK {
                                    card.drill_input.pop();
                                } else if let Some(c) = event.character {
                                    if !c.is_control() {
                                        card.drill_input.push(c);
                                        card.drill_notice = None;
                                    }
                                }
                            } else {
                                if event.character.map(|c| c.to_ascii_lowercase()) == Some('r') {
                                    card.drill_attempt = None;
                                    card.drill_notice = None;
                                } else if let Some(c) = event.character {
                                    if c == '1' || c == '2' || c == '3' {
                                        let rating = match c {
                                            '1' => qingjian_learning::ReviewRating::Again,
                                            '3' => qingjian_learning::ReviewRating::Easy,
                                            _ => qingjian_learning::ReviewRating::Good,
                                        };
                                        if let Some(user_dir) =
                                            qingjian_platform::dirs::user_dir()
                                        {
                                            let mut vault =
                                                qingjian_learning::LearningVault::open(&user_dir);
                                            let now = jiff::Zoned::now().timestamp().as_second();
                                            let target_expr = card
                                                .primary
                                                .as_ref()
                                                .map(|p| p.text.clone())
                                                .unwrap_or_default();
                                            vault.process_review(
                                                &card.target_text,
                                                &target_expr,
                                                rating,
                                                now,
                                            );
                                            let _ = vault.flush();

                                            let remaining = vault.today_due_reviews(now);
                                            if remaining.is_empty() {
                                                card.drill_notice = Some(
                                                    "🎉 今日复习已全部完成！记忆已固化。\n请按 Esc 返回。"
                                                        .to_string(),
                                                );
                                                card.drill_attempt = None;
                                                card.drill_input.clear();
                                                card.today_due_count = 0;
                                            } else {
                                                let next = remaining[0];
                                                card.today_due_count = remaining.len();
                                                card.review_current_index += 1;
                                                card.target_text = next.prompt.clone();
                                                card.primary = Some(
                                                    qingjian_platform::protocol::LearningExpression {
                                                        text: next.expression.clone(),
                                                        ..Default::default()
                                                    },
                                                );
                                                card.drill_input.clear();
                                                card.drill_attempt = None;
                                                card.drill_notice = None;
                                            }
                                        }
                                    }
                                }
                            }
                            let shown = self.self_drawn_frame();
                            self.reconcile_candidates(&shown);
                            return ServerMessage::KeyResult {
                                session,
                                outcome: KeyOutcome::Consumed,
                                commit: None,
                                frame: self.current_frame(),
                            };
                        }
                    }
                }
            }
        }
        let (commit, outcome) = match self.apply_key(&event) {
            Effect::Changed(commit) => {
                self.recompose();
                (commit, KeyOutcome::Consumed)
            }
            Effect::Navigated => (None, KeyOutcome::Consumed),
            Effect::Passthrough => (None, KeyOutcome::Passthrough),
        };
        self.poll_prediction();
        // 自绘窗吃未降级的帧；发给 DLL 的那份按老协议降级（见 composed 的 current_frame）
        let shown = self.self_drawn_frame();
        self.reconcile_candidates(&shown);
        ServerMessage::KeyResult {
            session,
            outcome,
            commit,
            frame: self.current_frame(),
        }
    }

    /// 云联想轮询：聚焦会话拉一次异步结果回最新一帧，否则回空帧。释义兜底与本地整句模型也借这个节拍收。
    fn handle_poll(&mut self, session: SessionId) -> ServerMessage {
        self.tick();
        let learned = self.engine.poll_glosses();
        if learned > 0 {
            tracing::info!(learned, "释义兜底写入个人释义表");
        }
        let frame = if self.focused == Some(session) {
            self.poll_prediction();
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            self.current_frame()
        } else {
            Frame::default()
        };
        ServerMessage::Update { session, frame }
    }
}
