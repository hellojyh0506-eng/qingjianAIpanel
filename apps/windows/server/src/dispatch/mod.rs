//! 协议分派：把 DLL 发来的 [`ClientMessage`] 交给 Engine，产出回给 DLL 的 [`ServerMessage`]。
//! 消息分派在 [`message`]，会话在 [`session`]，组句展示状态在 [`composed`]，按键在 [`key`]，
//! 候选窗口输出在 [`candidates`]，状态条在 [`status`]，翻译选中文字在 [`translate`]，配置热加载在 [`reload`]，
//! 本地整句模型在 [`rescore`]，形码码表在 [`code`]。

mod candidate_action;
mod candidates;
mod code;
mod composed;
mod config;
mod key;
mod message;
mod reload;
mod rescore;
mod session;
mod status;
mod translate;
mod recall;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use qingjian_core::Engine;
use qingjian_platform::LocalModelConfig;
use qingjian_platform::protocol::{
    ClientMessage, Frame, IndicatorState, InputSettings, ScreenRect, ServerMessage, SessionId,
};

pub use self::candidate_action::CandidateAction;
pub use self::candidates::{CandidateSink, NoopSink, RenderSettings};
pub use self::code::find_code_table;
use self::composed::Composed;
pub use self::config::RouterConfig;
use self::recall::RecallSession;
use self::reload::ConfigReload;
pub use self::reload::{DataDirs, attach_cloud};
pub use self::rescore::find_model;
use self::rescore::{ModelLoader, RescoreState};
use self::session::SessionInfo;
pub use self::status::{NoopStatusSink, StatusEvent, StatusSink, StatusView};
use self::translate::Translation;

/// 学习数据落盘间隔（与 macOS 壳一致）；Server 没有定时器，借消息节拍看时间。
const LEARNING_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// 同一时刻只有一个应用有键盘焦点，所以一个 Engine 持当前组句；焦点切到别的会话时先清掉上一个的残留。
pub struct Router {
    /// 输入内核，进程内唯一。
    engine: Engine,

    /// 每页候选数 / 云端槽位 / 排布 / 外观 / 翻页键等。
    config: RouterConfig,

    /// 活跃会话及各自的宿主应用。
    sessions: HashMap<SessionId, SessionInfo>,

    /// 当前持有组句的会话。
    focused: Option<SessionId>,

    /// 当前组句的展示状态；没在组句时为 `None`。
    composed: Option<Composed>,

    /// 「翻译选中文字」进行态；与 `composed` 互斥。
    translation: Option<Translation>,

    /// 「主动回忆」进行态；与 `composed` 互斥。
    recall_session: Option<RecallSession>,

    /// 当前组句的被动英文学习翻译提示；缓冲变化时清空，仅作为学习旁路信息。
    pub(crate) passive_translation: Option<String>,

    /// 当前英语学习模式（Daily / Learn / Drill）。
    pub(crate) learning_mode: qingjian_platform::LearningMode,

    /// 开启 Drill 临时训练会话前的英语学习模式（Daily 或 Learn），退出时恢复。
    pub(crate) previous_learning_mode: qingjian_platform::LearningMode,

    /// 记录当前被动翻译触发时刻（T0），用于端到端延迟诊断。
    pub(crate) translation_start_time: Option<Instant>,
    pub(crate) compare_start_time: Option<Instant>,
    pub(crate) daily_show_english: bool,
    pub(crate) generation_id: u64,

    /// 当前被动翻译请求对应的候选词；若当前首选词变更则作废旧译文，防止污染新候选。
    pub(crate) passive_translation_target: Option<String>,

    /// 当前候选框第三层轻量英语学习卡片帧数据（Learn 模式或主动展开时呈现）。
    pub(crate) passive_learning_card: Option<qingjian_platform::protocol::LearningCardFrame>,

    /// 已发出、等 DLL 回选区的请求号；对不上的 `Selection` 丢弃。
    pending_selection: Option<u64>,

    /// 「翻译选中文字」请求号计数器。
    selection_seq: u64,

    /// 整句补全（preedit 右侧、Tab 上屏）；缓冲变化时清空。
    sentence: Option<String>,

    /// 删候选后的屏幕提示，随下一帧下发、下一次按键清。
    notice: Option<String>,

    /// 当前高亮候选在布局里的下标（跨页）。
    highlight: usize,

    /// 这轮查询里动过高亮：动过就不再拿重排结果换掉候选；注音模式数字键动过之后才选词。
    navigated: bool,

    /// 上次把学习数据落盘的时间。
    last_flush: Instant,

    /// 配置热加载状态；`None` 表示不热加载。
    reload: Option<ConfigReload>,

    /// 候选窗口输出端；Windows 上由 [`crate::ui`] 注入。
    candidates: Box<dyn CandidateSink>,

    /// 悬浮状态条输出端；Windows 上由 [`crate::ui`] 注入。
    status: Box<dyn StatusSink>,

    /// 全局中英模式（`true` 英文），所有应用共用。DLL 切了报来，激活 / 获焦 / 轮询时取走。
    english: bool,

    /// 当前输入法是不是青简：有 DLL 来取模式就是，切成别的输入法时收起。状态条只在这时显示；
    /// 应用退出不影响它，状态条是桌面常驻的。
    ime_active: bool,

    /// 聚焦会话最近报来的光标矩形；云联想异步到达时按它原地重摆候选窗口。
    last_rect: Option<ScreenRect>,

    /// 上次真正显示的帧与位置：没变就不重画（组字期间的空转 Poll 很多）。
    last_shown: Option<(Frame, ScreenRect)>,

    /// 本地整句模型（`.qjm` 或三件套目录）；没有模型文件为 `None`。
    model_path: Option<PathBuf>,

    /// 形码码表（`wubi/wubi86.tsv`，启动时找好的，见 [`code::find_code_table`]）；没有为 `None`。
    code_table: Option<PathBuf>,

    /// 进行中的模型加载；加载完接到 Engine 上就清掉。
    model_loader: Option<ModelLoader>,

    /// 上次套用的 `[model]`，变了才重载 / 卸载。
    applied_model: LocalModelConfig,

    /// 重排的防抖 / 轮询进行态。
    rescore: RescoreState,
}

impl Router {
    pub fn new(engine: Engine, config: RouterConfig) -> Self {
        Self {
            engine,
            config: RouterConfig {
                page_size: config.page_size.max(1),
                ..config
            },
            sessions: HashMap::new(),
            focused: None,
            composed: None,
            translation: None,
            recall_session: None,
            passive_translation: None,
            passive_translation_target: None,
            passive_learning_card: None,
            learning_mode: config.learning_mode,
            previous_learning_mode: config.learning_mode,
            translation_start_time: None,
            compare_start_time: None,
            daily_show_english: false,
            generation_id: 0,
            pending_selection: None,
            selection_seq: 0,
            sentence: None,
            notice: None,
            highlight: 0,
            navigated: false,
            last_flush: Instant::now(),
            reload: None,
            candidates: Box::new(NoopSink),
            status: Box::new(NoopStatusSink),
            english: false,
            ime_active: false,
            last_rect: None,
            last_shown: None,
            model_path: None,
            code_table: None,
            model_loader: None,
            applied_model: LocalModelConfig::default(),
            rescore: RescoreState::default(),
        }
    }

    /// 当前英语学习模式。
    pub fn learning_mode(&self) -> qingjian_platform::LearningMode {
        self.learning_mode
    }

    /// 开启 Drill 练习会话：暂存 previous_learning_mode，切入 Drill 模式（状态条学变橙色），绑定候选词并启动输入阶段。
    pub(crate) fn start_drill_session(&mut self) {
        if self.learning_mode != qingjian_platform::LearningMode::Drill {
            self.previous_learning_mode = self.learning_mode;
            self.learning_mode = qingjian_platform::LearningMode::Drill;
        }
        let target_text = self
            .passive_learning_card
            .as_ref()
            .map(|c| c.target_text.clone())
            .or_else(|| self.passive_translation_target.clone())
            .or_else(|| self.layout_candidate(self.highlight).map(|c| c.text))
            .unwrap_or_else(|| "当前表达".to_string());

        let primary = self
            .passive_learning_card
            .as_ref()
            .and_then(|c| c.primary.clone())
            .or_else(|| {
                self.passive_translation.as_ref().map(|s| {
                    qingjian_platform::protocol::LearningExpression {
                        text: s.clone(),
                        tag: "Natural".to_string(),
                        meaning: target_text.clone(),
                        tone: "Casual".to_string(),
                        scenario: "Daily conversation".to_string(),
                        commonality: "Common".to_string(),
                        why_natural: "Natural spoken English".to_string(),
                    }
                })
            });

        let alternatives = self
            .passive_learning_card
            .as_ref()
            .map(|c| c.alternatives.clone())
            .unwrap_or_default();

        let due = self.today_due_count();
        let upgrade = self
            .passive_learning_card
            .as_ref()
            .and_then(|c| c.upgrade.clone());

        self.passive_translation_target = Some(target_text.clone());
        if primary.is_none() && self.engine.prediction_enabled() {
            self.translation_start_time = Some(Instant::now());
            if let Some(seq) = self.engine.request_translation(&target_text) {
                self.generation_id = seq;
            }
        }

        self.passive_learning_card = Some(qingjian_platform::protocol::LearningCardFrame {
            target_text,
            state: qingjian_platform::protocol::LearningState::Ready,
            primary,
            alternatives,
            expanded: false,
            selected_index: None,
            drill_stage: Some(qingjian_platform::protocol::DrillStage::Input),
            drill_input: String::new(),
            drill_attempt: None,
            drill_notice: None,
            today_due_count: due,
            review_current_index: 0,
            review_total_count: due,
            upgrade,
        });
        self.reconcile_status();
    }

    /// 结束 Drill 练习会话：恢复 previous_learning_mode（Daily 或 Learn），清空 Drill 交互状态并更新状态条。
    pub(crate) fn end_drill_session(&mut self) {
        if self.learning_mode == qingjian_platform::LearningMode::Drill {
            self.learning_mode = self.previous_learning_mode;
        }
        if let Some(card) = self.passive_learning_card.as_mut() {
            card.drill_stage = None;
            card.drill_input.clear();
            card.drill_attempt = None;
            card.drill_notice = None;
        }
        if self.learning_mode == qingjian_platform::LearningMode::Daily {
            self.passive_learning_card = None;
        }
        self.reconcile_status();
    }

    /// 下发给 DLL 的按键行为设置：`OpenSession` 的回包带一次，之后每拍 `SyncMode` 也跟着走，
    /// 所以 DLL 不用自己读配置文件，配置改了也不用重开会话。
    pub(super) fn input_settings(&self) -> InputSettings {
        InputSettings {
            switch_mode: self.config.switch_mode,
            english_mode: self.config.english_mode,
            shift_letter_compose: self.config.shift_letter_compose,
            english_direct: self.config.english_direct,
        }
    }

    /// 任务栏图标右键菜单打勾用的开关状态，随 `ModeSync` 每一拍下发。
    pub(super) fn indicator_state(&self) -> IndicatorState {
        IndicatorState {
            full_width_punctuation: self.config.full_width,
            english_full_width_punctuation: self.config.english_full_width,
            status_bar: self.config.status_enabled,
            update_available: self.update_available(),
        }
    }

    pub fn set_candidate_sink(&mut self, sink: Box<dyn CandidateSink>) {
        sink.configure(self.config.render_settings());
        self.candidates = sink;
    }

    /// 直接碰 Engine：测试里改模式键这类启动时才设的开关。
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn set_status_sink(&mut self, sink: Box<dyn StatusSink>) {
        self.status = sink;
    }

    /// 处理一条消息；`None` 表示不用回话。到点顺带把学习数据落盘。
    pub fn handle(&mut self, message: ClientMessage) -> Option<ServerMessage> {
        let response = self.dispatch(message);
        if self.last_flush.elapsed() >= LEARNING_FLUSH_INTERVAL {
            self.flush_learning();
        }
        response
    }

    pub fn flush_learning(&mut self) {
        self.engine.flush_learning();
        self.last_flush = Instant::now();
    }

    /// 处理候选卡片上的鼠标交互操作。
    pub fn handle_candidate_action(&mut self, action: CandidateAction) {
        tracing::info!(?action, mode = ?self.learning_mode, "处理候选卡片动作");
        match action {
            CandidateAction::ToggleExpand => {
                if self.learning_mode == qingjian_platform::LearningMode::Daily {
                    if !self.daily_show_english {
                        self.daily_show_english = true;
                        let target = self
                            .layout_candidate(self.highlight)
                            .map(|c| c.text)
                            .or_else(|| self.layout_candidate(0).map(|c| c.text));
                        if let Some(target) = target {
                            self.passive_translation_target = Some(target.clone());
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
                            self.translation_start_time = Some(Instant::now());
                            if let Some(seq) = self.engine.request_translation(&target) {
                                self.generation_id = seq;
                            }
                        }
                    } else if let Some(card) = self.passive_learning_card.as_mut() {
                        card.expanded = !card.expanded;
                        if !card.expanded {
                            card.selected_index = None;
                        }
                    }
                } else if let Some(card) = self.passive_learning_card.as_mut() {
                    card.expanded = !card.expanded;
                    if !card.expanded {
                        card.selected_index = None;
                    }
                }
            }
            CandidateAction::ToggleAlternative(idx) => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    if card.selected_index == Some(idx) {
                        card.selected_index = None;
                    } else {
                        card.selected_index = Some(idx);
                    }
                }
            }
            CandidateAction::StartDrill => {
                self.start_drill_session();
            }
            CandidateAction::Reveal => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    let reference = card
                        .primary
                        .as_ref()
                        .map(|p| p.text.as_str())
                        .unwrap_or("");
                    card.drill_notice = Some(format!("参考答案: {}", reference));
                }
            }
            CandidateAction::Compare => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    let input = card.drill_input.trim();
                    if input.is_empty() {
                        card.drill_notice = Some("请输入英文表达进行对比".to_string());
                    } else {
                        let reference = card
                            .primary
                            .as_ref()
                            .map(|p| p.text.as_str())
                            .unwrap_or("");
                        card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Comparing);
                        card.drill_notice = None;
                        self.compare_start_time = Some(Instant::now());
                        self.engine.request_compare(
                            &card.target_text,
                            input,
                            reference,
                        );
                    }
                }
            }
            CandidateAction::Retry => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Input);
                    card.drill_input.clear();
                    card.drill_attempt = None;
                    card.drill_notice = None;
                }
            }
            CandidateAction::Save => {
                if let Some(card) = self.passive_learning_card.as_mut() {
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
                            .map(|a| a.comparison.natural_expression.clone())
                            .or_else(|| card.primary.as_ref().map(|p| p.text.clone()))
                            .unwrap_or_default();
                        if !saved_expr.is_empty() {
                            let tag = card.primary.as_ref().map(|p| p.tag.as_str());
                            vault.save_expression(&card.target_text, &saved_expr, tag, None);
                            let _ = vault.flush();
                            card.drill_notice = Some(format!("⭐ 已保存到表达本: {}", saved_expr));
                        } else {
                            card.drill_notice = Some("⭐ 已保存到表达本".to_string());
                        }
                    } else {
                        card.drill_notice = Some("⭐ 已保存到表达本".to_string());
                    }
                }
            }
            CandidateAction::ShowProfile => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    let profile_text = if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                        let vault = qingjian_learning::LearningVault::open(&user_dir);
                        vault.calculate_profile().format_ascii_bars()
                    } else {
                        "暂无用户目录数据".to_string()
                    };
                    card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Profile);
                    card.drill_notice = Some(profile_text);
                }
            }
            CandidateAction::StartReview => {
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
            }
            CandidateAction::RateAgain | CandidateAction::RateGood | CandidateAction::RateEasy => {
                let rating = match action {
                    CandidateAction::RateAgain => qingjian_learning::ReviewRating::Again,
                    CandidateAction::RateEasy => qingjian_learning::ReviewRating::Easy,
                    _ => qingjian_learning::ReviewRating::Good,
                };
                if let Some(card) = self.passive_learning_card.as_mut() {
                    if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                        let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                        let now = jiff::Zoned::now().timestamp().as_second();
                        let target_expr = card.primary.as_ref().map(|p| p.text.clone()).unwrap_or_default();
                        vault.process_review(&card.target_text, &target_expr, rating, now);
                        let _ = vault.flush();

                        let remaining = vault.today_due_reviews(now);
                        if remaining.is_empty() {
                            card.drill_notice = Some("🎉 今日复习已全部完成！记忆已固化。\n请按 Esc 返回。".to_string());
                            card.drill_attempt = None;
                            card.drill_input.clear();
                            card.today_due_count = 0;
                        } else {
                            let next = remaining[0];
                            card.today_due_count = remaining.len();
                            card.review_current_index += 1;
                            card.target_text = next.prompt.clone();
                            card.primary = Some(qingjian_platform::protocol::LearningExpression {
                                text: next.expression.clone(),
                                ..Default::default()
                            });
                            card.drill_input.clear();
                            card.drill_attempt = None;
                            card.drill_notice = None;
                        }
                    }
                }
            }
            CandidateAction::ClearVault => {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                        let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                        let _ = vault.clear_all();
                    }
                    card.drill_notice = Some("已彻底清空个人学习画像与错题记录。\n请按 Esc 返回。".to_string());
                }
            }
            CandidateAction::Finish => {
                self.end_drill_session();
            }
        }
        let shown = self.self_drawn_frame();
        self.reconcile_candidates(&shown);
        self.reconcile_status();
    }
}
