//! AI 问答面板窗口：一个可激活的置顶分层窗口（要接收键盘输入），画面全部交给青简渲染器
//! （`qingjian_render::chat`），这里只管窗口、定位、滚动、输入、定时器与点击分派。
//!
//! 视图状态变换在 [`model`]（纯函数），输入框编辑在 [`input`]，消息过程在 [`wndproc`]，
//! 点击分派在 [`actions`]，绘制与节流在 [`paint`]，输入法锚定在 [`ime`]。

mod actions;
mod ime;
mod input;
mod model;
mod paint;
mod wndproc;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use qingjian_learning::chat_history::SessionSummary;
use qingjian_predict::SearchResult;
use qingjian_render::chat::{ChatView, Phase};
use qingjian_render::{HitMap, PanelRenderer};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CS_DBLCLKS, CreateWindowExW, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW,
    IsIconic, KillTimer, LoadCursorW, SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, WNDCLASSEXW,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use super::window_class::WindowClass;
use crate::dispatch::{AiPanelState, CandidateAction};

const CLASS_NAME: PCWSTR = w!("QingjianAiAssistantWindow");
static CLASS: WindowClass = WindowClass::new();

/// 窗口默认尺寸（点）。
const DEFAULT_SIZE: (f32, f32) = (720.0, 520.0);

/// 提示条自动消失的定时器与时长。
const TIMER_NOTICE: usize = 1;
const NOTICE_MS: u32 = 1800;

/// 转圈动画的定时器、间隔与每帧转过的角度。
const TIMER_SPIN: usize = 2;
const SPIN_MS: u32 = 80;
const SPIN_STEP: f32 = 30.0;

/// 输入光标闪烁。
const TIMER_CARET: usize = 3;
const CARET_BLINK_MS: u32 = 530;

/// 流式内容的重绘节流：两帧之间至少隔这么久，被挡下的最后一块由补画定时器画出来。
const TIMER_FLUSH: usize = 4;
const FRAME_MS: u32 = 33;

/// 滚轮一格滚动的距离（点）。
const WHEEL_STEP: f32 = 36.0;

thread_local! {
    /// 本线程活着的面板窗口：HWND → 窗口。
    static WINDOWS: RefCell<HashMap<isize, Rc<AiAssistantWindow>>> = RefCell::new(HashMap::new());
}

fn window_of(hwnd: HWND) -> Option<Rc<AiAssistantWindow>> {
    WINDOWS.with(|map| map.borrow().get(&(hwnd.0 as isize)).cloned())
}

/// 最近一帧的几何，点击命中、滚轮、输入法锚定用。
struct Placed {
    hits: HitMap<qingjian_render::chat::ChatHit>,

    /// 阴影留白（物理像素）。
    margin: i32,

    /// 点 → 物理像素。
    scale: f32,

    max_scroll: f32,

    sidebar_max_scroll: f32,

    /// 输入光标 `(x, y, 高)`（点，内容坐标）。
    caret: (f32, f32, f32),
}

/// 最大化之前的尺寸与位置，还原用。
struct Restore {
    size: (f32, f32),
    pos: Option<(i32, i32)>,
}

struct State {
    view: ChatView,

    /// 惰性建：第一次画时才加载字体。
    renderer: Option<PanelRenderer>,

    placed: Option<Placed>,

    /// 当前会话的摘要与侧栏各行对应的会话 id。
    current: SessionSummary,
    session_ids: Vec<String>,

    /// 联网来源比助手消息先到时暂存在这。
    pending_sources: Vec<String>,

    visible: bool,

    /// 本次打开时的组句矩形，决定落在哪个显示器。
    anchor: Option<RECT>,

    /// 内容左上角的屏幕坐标（物理像素）；`None` 表示下一帧重新定位。
    content_pos: Option<(i32, i32)>,

    /// 用户拖过的位置，之后每次打开都停在这里。
    user_pos: Option<(i32, i32)>,

    /// 正常（非最大化）时的内容尺寸（点）。
    normal_size: (f32, f32),
    restore: Option<Restore>,

    /// 上一帧的时间，流式节流用。
    last_paint: Option<Instant>,

    /// 有内容变化还没画出来（节流、最小化时都会攒下）。
    dirty: bool,

    /// 窗口有键盘焦点。
    focused: bool,

    spinning: bool,

    /// 等低代理项的前半个字符（WM_CHAR 按 UTF-16 码元到）。
    high_surrogate: Option<u16>,
}

pub struct AiAssistantWindow {
    hwnd: HWND,
    state: RefCell<State>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl AiAssistantWindow {
    pub fn new(action_cb: Box<dyn Fn(CandidateAction)>) -> Result<Rc<Self>> {
        CLASS.ensure(|| WNDCLASSEXW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc::wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW | WS_EX_LAYERED | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Qingjian AI Assistant"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(super::module_handle()),
                None,
            )?
        };
        let view = ChatView {
            width: DEFAULT_SIZE.0,
            height: DEFAULT_SIZE.1,
            pinned: true,
            ..ChatView::default()
        };
        let window = Rc::new(Self {
            hwnd,
            state: RefCell::new(State {
                view,
                renderer: None,
                placed: None,
                current: SessionSummary {
                    id: String::new(),
                    title: "新对话".to_string(),
                    message_count: 0,
                    updated_at: 0,
                },
                session_ids: Vec::new(),
                pending_sources: Vec::new(),
                visible: false,
                anchor: None,
                content_pos: None,
                user_pos: None,
                normal_size: DEFAULT_SIZE,
                restore: None,
                last_paint: None,
                dirty: false,
                focused: false,
                spinning: false,
                high_surrogate: None,
            }),
            action_cb,
        });
        WINDOWS.with(|map| map.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&window)));
        Ok(window)
    }

    pub fn is_visible(&self) -> bool {
        self.state.borrow().visible
    }

    pub fn is_pinned(&self) -> bool {
        self.state.borrow().view.pinned
    }

    /// 显示并抢到键盘焦点。窗口已开着且这一轮还在流式（同一会话）就保留现场，不重载。
    pub fn show_or_focus(&self, anchor: Option<RECT>, panel: AiPanelState) {
        {
            let mut state = self.state.borrow_mut();
            let keep = state.visible
                && model::is_busy(&state.view)
                && state.current.id == panel.session.id;
            if !keep {
                state.apply(&panel);
            }
            if !state.visible {
                state.content_pos = None;
            }
            state.anchor = anchor;
            state.visible = true;
            state.dirty = true;
        }
        self.redraw();
        unsafe {
            if IsIconic(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
            }
            self.apply_z_order(true);
            let _ = BringWindowToTop(self.hwnd);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
        self.on_focus_changed(true);
    }

    pub fn hide(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.visible = false;
            state.anchor = None;
            state.focused = false;
            state.view.show_caret = false;
        }
        self.stop_timers();
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    /// 用户主动关闭窗口并向 Router 报告。
    pub fn close_and_notify(&self) {
        self.hide();
        (self.action_cb)(CandidateAction::CloseAiAssistant);
    }

    pub fn minimize(&self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_MINIMIZE) };
    }

    pub fn toggle_pin(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.view.pinned = !state.view.pinned;
        }
        self.apply_z_order(false);
        self.redraw();
    }

    /// 业务层推来整面状态（换会话 / 清空 / 新对话之后）。
    pub fn load_state(&self, panel: AiPanelState) {
        self.state.borrow_mut().apply(&panel);
        self.redraw();
    }

    /// 只刷新侧栏会话列表（会话落盘之后）。
    pub fn set_sessions(&self, sessions: Vec<SessionSummary>) {
        {
            let mut state = self.state.borrow_mut();
            let current = state.current.clone();
            state.view.sessions = model::sidebar_items(&sessions, &current);
            state.session_ids = model::sidebar_ids(&sessions, &current);
        }
        self.redraw();
    }

    /// 回答文字增量（流式，节流重绘）。
    pub fn append_stream_chunk(&self, delta: &str) {
        let accepted = {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::append_chunk(&mut state.view, &mut state.pending_sources, delta)
        };
        if accepted {
            self.redraw_throttled();
        }
    }

    /// 思考过程增量（流式，节流重绘）。
    pub fn append_reasoning(&self, delta: &str) {
        let accepted = {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::append_reasoning(&mut state.view, &mut state.pending_sources, delta)
        };
        if accepted {
            self.redraw_throttled();
        }
    }

    /// 联网搜索的真实来源到了。
    pub fn set_web_sources(&self, sources: Vec<SearchResult>) {
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            let lines = model::source_lines(&sources);
            model::attach_sources(&mut state.view, &mut state.pending_sources, lines);
        }
        self.redraw_throttled();
    }

    pub fn finish_stream(&self, ttft_ms: u128, total_ms: u128) {
        tracing::debug!(ttft_ms, total_ms, "AI 问答回答结束");
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::finish(&mut state.view, &mut state.pending_sources);
        }
        self.redraw();
    }

    pub fn set_stream_error(&self, error: &str) {
        let shown = {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::fail(&mut state.view, &mut state.pending_sources, error)
        };
        if shown {
            self.redraw();
        }
    }

    /// 新对话：本地先清空消息与草稿，再通知业务层开新会话（随后会推来整面状态）。
    pub fn new_session(&self) {
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::clear_messages(&mut state.view, &mut state.pending_sources);
            state.view.input.clear();
            state.view.cursor = 0;
        }
        self.redraw();
        (self.action_cb)(CandidateAction::AiChatNewSession);
    }

    /// 最大化 / 还原。最大化铺满窗口所在显示器的工作区，且不画阴影。
    pub fn toggle_maximize(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.view.maximized {
                state.view.maximized = false;
                if let Some(restore) = state.restore.take() {
                    state.normal_size = restore.size;
                    state.user_pos = restore.pos;
                }
                state.content_pos = None;
            } else {
                state.restore = Some(Restore {
                    size: state.normal_size,
                    pos: state.content_pos.or(state.user_pos),
                });
                state.view.maximized = true;
            }
            state.dirty = true;
        }
        self.redraw();
    }

    pub fn input_text(&self) -> String {
        self.state.borrow().view.input.clone()
    }

    pub fn set_input_text(&self, text: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.view.input = text.to_string();
            state.view.cursor = state.view.input.len();
        }
        self.redraw();
    }

    pub fn deep(&self) -> bool {
        self.state.borrow().view.deep
    }

    pub fn toggle_deep(&self) {
        self.update_view(|view| view.deep = !view.deep);
    }

    pub fn web(&self) -> bool {
        self.state.borrow().view.web
    }

    pub fn toggle_web(&self) {
        self.update_view(|view| view.web = !view.web);
    }

    pub fn message_count(&self) -> usize {
        self.state.borrow().view.messages.len()
    }

    pub(crate) fn update_view(&self, change: impl FnOnce(&mut ChatView)) {
        change(&mut self.state.borrow_mut().view);
        self.redraw();
    }

    /// 底部短暂提示，约 1.8 秒后消失。
    pub(crate) fn set_notice(&self, text: &str) {
        self.state.borrow_mut().view.notice = Some(text.to_string());
        unsafe {
            let _ = SetTimer(Some(self.hwnd), TIMER_NOTICE, NOTICE_MS, None);
        }
        self.redraw();
    }

    /// 按置顶状态摆 z 序；`activate` 为假时不抢焦点。
    fn apply_z_order(&self, activate: bool) {
        let pinned = self.is_pinned();
        let flags = if activate {
            SWP_NOMOVE | SWP_NOSIZE
        } else {
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
        };
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                0,
                0,
                0,
                0,
                flags,
            );
        }
    }

    pub(crate) fn stop_timers(&self) {
        self.state.borrow_mut().spinning = false;
        for id in [TIMER_NOTICE, TIMER_SPIN, TIMER_CARET, TIMER_FLUSH] {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), id);
            }
        }
    }
}

impl State {
    /// 装入业务层的整面状态，记下当前会话与侧栏行对应的 id。
    fn apply(&mut self, panel: &AiPanelState) {
        model::load_state(&mut self.view, panel);
        self.current = panel.session.summary();
        self.session_ids = model::sidebar_ids(&panel.sessions, &self.current);
        self.pending_sources.clear();
        self.dirty = true;
    }

    fn busy(&self) -> bool {
        self.view.phase != Phase::Idle
    }
}
