use qingjian_core::{
    Candidate, CandidateKind, CandidateList, Language, RecallOutcome,
};
use qingjian_learning::RecallBook;
use qingjian_platform::protocol::{
    Frame, KeyEvent, KeyOutcome, PreeditKind, PreeditSegment, ServerMessage, SessionId,
};

use super::Router;
use super::key::{BACK, ESCAPE, RETURN, TAB};

/// 主动回忆单次测试会话
#[derive(Debug, Clone)]
pub struct RecallSession {
    pub prompt: String,
    pub target: String,
    pub language: Language,
    pub input: String,
    pub revealed: bool,
}

impl Router {
    pub(super) fn matches_recall_combo(&self, event: &KeyEvent) -> bool {
        let chord = event.modifiers.chord();
        event.character.map(|c| c.to_ascii_lowercase()) == Some('r')
            && chord.ctrl
            && chord.alt
            && !chord.shift
            && !chord.win
    }

    /// 开启主动回忆会话：若有待复习条目则进入，否则在 notice 给出温和提示
    pub(super) fn start_recall_session(&mut self, session: SessionId) -> ServerMessage {
        let lang = self.engine.learning_language();
        if let Some(item) = self.engine.next_recall_item(lang) {
            self.recall_session = Some(RecallSession {
                prompt: item.prompt,
                target: item.target,
                language: lang,
                input: String::new(),
                revealed: false,
            });
            let frame = self.recall_frame();
            self.reconcile_candidates(&frame);
            ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame,
            }
        } else {
            self.notice = Some("【主动回忆】暂无待复习条目 (上屏带翻译的候选词会自动加入)".into());
            let frame = self.self_drawn_frame();
            self.reconcile_candidates(&frame);
            ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            }
        }
    }

    /// 主动回忆交互态：
    /// Esc: 退出回忆模式
    /// Tab: 查看答案并记为 Revealed
    /// Backspace: 删除已输入字母
    /// Enter: 提交回忆，执行确定性打分并记录
    /// 字母/数字/标点: 输入英文
    pub(super) fn handle_recall_session(
        &mut self,
        session: SessionId,
        event: &KeyEvent,
    ) -> ServerMessage {
        let Some(recall) = self.recall_session.as_mut() else {
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Passthrough,
                commit: None,
                frame: Frame::default(),
            };
        };

        if event.virtual_key == ESCAPE {
            self.end_recall_session();
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: Frame::default(),
            };
        }

        if event.virtual_key == TAB {
            recall.revealed = true;
            self.engine.record_recall(
                recall.language,
                &recall.prompt,
                &recall.target,
                RecallOutcome::Revealed,
            );
            let frame = self.recall_frame();
            self.reconcile_candidates(&frame);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame,
            };
        }

        if event.virtual_key == RETURN {
            let prompt = recall.prompt.clone();
            let target = recall.target.clone();
            let user_typed = recall.input.clone();
            let lang = recall.language;
            let matched = RecallBook::evaluate_with_prompt(&user_typed, &target, Some(&prompt));
            let outcome = if matched {
                RecallOutcome::Success
            } else {
                RecallOutcome::Failure
            };
            self.engine
                .record_recall_with_input(lang, &prompt, &target, outcome, &user_typed);
            self.end_recall_session();

            let notice = if matched {
                if let Some(pref) =
                    self.engine.preferred_recall_alternative(lang, &prompt, &target)
                {
                    format!("【主动回忆】正确！已学习首选表达「{}」-> {}", prompt, pref)
                } else {
                    format!("【主动回忆】正确！「{}」-> {}", prompt, target)
                }
            } else {
                format!("【主动回忆】未匹配。目标为: {}", target)
            };
            self.notice = Some(notice);
            let frame = self.self_drawn_frame();
            self.reconcile_candidates(&frame);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame: self.current_frame(),
            };
        }

        if event.virtual_key == BACK {
            recall.input.pop();
            let frame = self.recall_frame();
            self.reconcile_candidates(&frame);
            return ServerMessage::KeyResult {
                session,
                outcome: KeyOutcome::Consumed,
                commit: None,
                frame,
            };
        }

        if let Some(c) = event.character {
            if !c.is_control() {
                recall.input.push(c);
                let frame = self.recall_frame();
                self.reconcile_candidates(&frame);
                return ServerMessage::KeyResult {
                    session,
                    outcome: KeyOutcome::Consumed,
                    commit: None,
                    frame,
                };
            }
        }

        ServerMessage::KeyResult {
            session,
            outcome: KeyOutcome::Consumed,
            commit: None,
            frame: self.recall_frame(),
        }
    }

    pub(super) fn end_recall_session(&mut self) {
        if self.recall_session.take().is_some() {
            self.hide_candidate_window();
        }
    }

    pub(super) fn recall_frame(&self) -> Frame {
        let Some(recall) = &self.recall_session else {
            return Frame::default();
        };

        let notice = if recall.revealed {
            format!(
                "【主动回忆】中文: {} | 答案: {} (Enter结束/Esc退出)",
                recall.prompt, recall.target
            )
        } else {
            format!(
                "【主动回忆】中文: {} (输入英文, Enter提交, Tab看答案, Esc退出)",
                recall.prompt
            )
        };

        let preedit_text = if recall.input.is_empty() {
            " ".to_string()
        } else {
            recall.input.clone()
        };

        Frame {
            preedit: vec![PreeditSegment {
                text: preedit_text,
                kind: PreeditKind::Typed,
            }],
            preedit_mode: self.config.preedit,
            cursor: recall.input.chars().count(),
            candidates: CandidateList {
                items: vec![Candidate {
                    text: if recall.revealed {
                        recall.target.clone()
                    } else {
                        "✍ 请输入英文表达...".to_string()
                    },
                    kind: CandidateKind::Cloud,
                    syllables: Vec::new(),
                    reading: None,
                    translation: None,
                    aux_code: None,
                }],
            },
            highlight: 0,
            page: 0,
            page_count: 1,
            layout: self.config.layout,
            theme: self.config.theme,
            aux_code_show: self.config.aux_code_show,
            sentence: None,
            notice: Some(notice),
            learning_mode: self.learning_mode,
        }
    }
}
