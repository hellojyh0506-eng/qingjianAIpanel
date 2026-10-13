//! 协议分派：把 DLL 发来的 [`ClientMessage`] 交给 Engine，产出回给 DLL 的 [`ServerMessage`]。
//! 消息分派在 [`message`]，会话在 [`session`]，组句展示状态在 [`composed`]，按键在 [`key`]，
//! 候选窗口输出在 [`candidates`]，状态条在 [`status`]，翻译选中文字在 [`translate`]，配置热加载在 [`reload`]，
//! 本地整句模型在 [`rescore`]，形码码表在 [`code`]。

mod ai_chat;
mod candidate_action;
mod candidates;
mod code;
mod companion;
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use qingjian_predict::ChatMessage;
use qingjian_core::{Engine, SurroundingText};
use qingjian_platform::LocalModelConfig;
use qingjian_platform::protocol::{
    ClientMessage, Frame, IndicatorState, InputSettings, ScreenRect, ServerMessage, SessionId,
};

pub use self::ai_chat::AiPanelState;
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

    /// TSF 隐私检查后送来的光标前文（最多 64 字）与后文（最多 32 字）；仅在当前组句期间用于本地重排与云端 Resolve，不写入日志。
    surrounding: Option<SurroundingText>,

    /// 当前组句的展示状态；没在组句时为 `None`。
    composed: Option<Composed>,

    /// 「翻译选中文字」进行态；与 `composed` 互斥。
    translation: Option<Translation>,

    /// 记录最近一次主动翻译请求的源文本，便于在云端异常时本地离线词典兜底
    pub(crate) last_translation_source: Option<String>,

    /// 「主动回忆」进行态；与 `composed` 互斥。
    recall_session: Option<RecallSession>,

    /// 当前组句的被动英文学习翻译提示；缓冲变化时清空，仅作为学习旁路信息。
    pub(crate) passive_translation: Option<String>,

    /// 当前英语学习模式（Daily / Learn / Drill）。
    pub(crate) learning_mode: qingjian_platform::LearningMode,

    /// 开启 Drill 临时训练会话前的英语学习模式（Daily 或 Learn），退出时恢复。
    #[allow(dead_code)]
    pub(crate) previous_learning_mode: qingjian_platform::LearningMode,

    /// 记录当前被动翻译触发时刻（T0），用于端到端延迟诊断。
    pub(crate) translation_start_time: Option<Instant>,

    /// 疑似敲错时被动翻译延后到 Resolve 回来再发；回来之后补发并清掉。
    pub(crate) translation_deferred: bool,
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

    /// 学习助手伴侣窗当前是否展开
    pub(crate) assistant_open: bool,

    /// 学习助手请求代次；旧流式请求只能写入其启动时对应的代次。
    pub(crate) companion_generation: Arc<std::sync::atomic::AtomicU64>,

    /// 当前打开的学习助手会话；面板关了为 `None`。
    companion: Option<Arc<companion::CompanionSession>>,

    /// 学习助手结果的磁盘缓存，第一次打开面板时才读。
    companion_cache: Option<Arc<Mutex<qingjian_predict::companion::BlockCache>>>,

    /// 上次切换助手的时间戳（防按键连击 repeat）
    pub(crate) last_assistant_toggle_time: Instant,

    /// 独立 AI Assistant 对话窗口当前是否展开
    pub(crate) ai_assistant_open: bool,

    /// 上次切换 AI 对话助手的时间戳（防按键连击 repeat）
    pub(crate) last_ai_assistant_toggle_time: Instant,

    /// 运行流式网络请求与联网搜索的多线程 Tokio Runtime
    pub(crate) ai_runtime: Arc<tokio::runtime::Runtime>,

    /// AI Assistant 多轮对话历史记录
    pub(crate) ai_messages: Arc<Mutex<Vec<ChatMessage>>>,

    /// AI Assistant 流式中止信号
    pub(crate) ai_stop_signal: Arc<AtomicBool>,

    /// AI Assistant 的会话仓库（当前会话 + 磁盘历史）
    pub(crate) ai_store: Arc<ai_chat::AiStore>,
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
            surrounding: None,
            composed: None,
            translation: None,
            last_translation_source: None,
            recall_session: None,
            passive_translation: None,
            passive_translation_target: None,
            passive_learning_card: None,
            learning_mode: config.learning_mode,
            previous_learning_mode: config.learning_mode,
            translation_start_time: None,
            translation_deferred: false,
            compare_start_time: None,
            daily_show_english: true,
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
            assistant_open: false,
            companion_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            companion: None,
            companion_cache: None,
            last_assistant_toggle_time: Instant::now() - std::time::Duration::from_secs(10),
            ai_assistant_open: false,
            last_ai_assistant_toggle_time: Instant::now() - std::time::Duration::from_secs(10),
            ai_runtime: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .unwrap_or_else(|_| tokio::runtime::Runtime::new().expect("启动 Tokio 运行时失败"))
            ),
            ai_messages: Arc::new(Mutex::new(Vec::new())),
            ai_stop_signal: Arc::new(AtomicBool::new(false)),
            ai_store: Arc::new(ai_chat::AiStore::new(None)),
        }
    }

    /// 单键切换独立 AI Assistant 对话窗口（Ctrl+Alt+D）
    pub(crate) fn toggle_ai_assistant(&mut self) {
        if self.ai_assistant_open {
            self.close_ai_assistant();
        } else {
            self.open_ai_assistant();
        }
    }

    pub fn is_ai_assistant_open(&self) -> bool {
        self.ai_assistant_open
    }

    pub fn set_last_ai_assistant_toggle_time(&mut self, time: Instant) {
        self.last_ai_assistant_toggle_time = time;
    }

    pub fn is_assistant_open(&self) -> bool {
        self.assistant_open
    }

    pub fn set_last_assistant_toggle_time(&mut self, time: Instant) {
        self.last_assistant_toggle_time = time;
    }

    /// 返回 TSF 已做隐私过滤的当前光标前文，作为受约束候选重排上下文。
    pub(super) fn rerank_surrounding(&self) -> Option<SurroundingText> {
        if self.engine.is_private() {
            return None;
        }
        self.surrounding.clone()
    }

    pub fn set_pending_selection(&mut self, session: SessionId, req: u64) {
        self.focused = Some(session);
        self.pending_selection = Some(req);
    }

    pub fn active_frame(&self) -> Frame {
        self.self_drawn_frame()
    }

    pub fn is_composing(&self) -> bool {
        self.composing()
    }

    pub fn ai_messages_count(&self) -> usize {
        self.ai_messages.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn last_ai_message(&self) -> Option<ChatMessage> {
        self.ai_messages.lock().ok().and_then(|m| m.last().cloned())
    }

    pub fn push_ai_message(&self, msg: ChatMessage) {
        if let Ok(mut m) = self.ai_messages.lock() {
            m.push(msg);
        }
    }

    pub fn is_ai_stop_signaled(&self) -> bool {
        self.ai_stop_signal.load(Ordering::Relaxed)
    }

    /// 当前英语学习模式。
    pub fn learning_mode(&self) -> qingjian_platform::LearningMode {
        self.learning_mode
    }

    /// 单键切换独立 English Assistant（打开/关闭学习助手）
    pub(crate) fn toggle_assistant(&mut self) {
        if self.assistant_open {
            self.close_assistant();
        } else {
            self.open_assistant();
        }
    }

    /// 开启 Drill 练习会话：弹出独立 Drill 练习窗口。
    pub(crate) fn start_drill_session(&mut self) {
        let rect = self.last_rect.unwrap_or(qingjian_platform::protocol::ScreenRect {
            left: 200,
            top: 200,
            right: 400,
            bottom: 230,
        });

        let target_text = self
            .passive_learning_card
            .as_ref()
            .map(|c| c.target_text.clone())
            .or_else(|| self.passive_translation_target.clone())
            .or_else(|| self.layout_candidate(self.highlight).map(|c| c.text))
            .or_else(|| self.layout_candidate(0).map(|c| c.text))
            .unwrap_or_else(|| "确实有道理".to_string());

        let reference_str = self
            .passive_learning_card
            .as_ref()
            .and_then(|c| c.primary.as_ref().map(|p| p.text.clone()))
            .or_else(|| self.passive_translation.clone())
            .or_else(|| {
                self.engine.translate_word(&target_text).and_then(|t| {
                    t.senses().first().map(|s| s.text.clone())
                })
            })
            .unwrap_or_else(|| "That makes perfect sense.".to_string());

        self.begin_drill(target_text, reference_str, None, rect);
    }

    /// 弹出练习窗口并把学习卡切到输入态；`cloze` 有值就是挖空补全，没有是整句翻译。
    pub(crate) fn begin_drill(
        &mut self,
        target_text: String,
        reference_str: String,
        cloze: Option<String>,
        rect: qingjian_platform::protocol::ScreenRect,
    ) {
        let mut card = self.passive_learning_card.take().unwrap_or_else(|| {
            qingjian_platform::protocol::LearningCardFrame {
                target_text: target_text.clone(),
                state: qingjian_platform::protocol::LearningState::Ready,
                primary: Some(qingjian_platform::protocol::LearningExpression {
                    text: reference_str.clone(),
                    tag: "Natural".to_string(),
                    meaning: target_text.clone(),
                    tone: "Casual".to_string(),
                    scenario: String::new(),
                    commonality: "High".to_string(),
                    why_natural: "地道日常口语表达".to_string(),
                }),
                ..Default::default()
            }
        });
        card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Input);
        card.drill_input.clear();
        self.passive_learning_card = Some(card);

        self.candidates.hide_companion();
        self.candidates.start_drill(target_text, reference_str, cloze, rect);
    }

    /// 结束 Drill 练习会话：收起 Drill 练习窗口。
    pub(crate) fn end_drill_session(&mut self) {
        if let Some(card) = &mut self.passive_learning_card {
            card.drill_stage = None;
            card.drill_input.clear();
        }
        self.candidates.hide_drill();
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
            CandidateAction::OpenAssistant => {
                self.open_assistant();
            }
            CandidateAction::CloseAssistant => {
                self.close_assistant();
            }
            CandidateAction::CompanionSelectTab(tab) => self.companion_select_tab(tab),
            CandidateAction::CompanionSelectScene(scene) => self.companion_select_scene(scene),
            CandidateAction::CompanionRetry { tab, scene } => self.companion_retry(tab, scene),
            CandidateAction::CompanionSave => self.companion_save(),
            CandidateAction::CompanionPractice => self.companion_practice(),
            CandidateAction::StartDrillSession | CandidateAction::StartDrill => {
                self.start_drill_session();
            }
            CandidateAction::DrillCompare(prompt, input, reference) => {
                self.compare_start_time = Some(Instant::now());
                self.engine.request_compare(&prompt, &input, &reference);
            }
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
            CandidateAction::OpenAiAssistant => {
                self.open_ai_assistant();
            }
            CandidateAction::CloseAiAssistant => {
                self.close_ai_assistant();
            }
            CandidateAction::AiChatNewSession => self.ai_chat_new(),
            CandidateAction::AiChatSwitch { session_id } => self.ai_chat_switch(&session_id),
            CandidateAction::AiChatClear => self.ai_chat_clear(),
            CandidateAction::AiChatStop { session_id: _ } => self.ai_chat_stop(),
            CandidateAction::AiChatSend {
                prompt,
                deep,
                web_search,
                session_id: _,
            } => self.ai_chat_send(prompt, deep, web_search),
        }
        let shown = self.self_drawn_frame();
        self.reconcile_candidates(&shown);
        self.reconcile_status();
    }
}
