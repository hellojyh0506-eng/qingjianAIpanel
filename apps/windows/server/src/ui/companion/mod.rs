//! English Assistant（学习助手）面板窗口：一个无激活的置顶分层窗口，画面全部交给青简渲染器
//! （`qingjian_render::companion`），这里只管窗口、定位、滚动、转圈与点击分派。
//!
//! 视图状态在 [`model`]（纯数据），消息过程在 [`wndproc`]，落点在 [`placement`]，英文朗读在 [`tts`]。

mod clipboard;
mod model;
mod placement;
mod tts;
mod wndproc;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use qingjian_predict::companion::Block;
use qingjian_render::companion::{CompanionHit, CompanionView};
use qingjian_render::{FontLibrary, HitMap, PanelRenderer, Shadow};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW, SW_HIDE,
    SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use self::tts::Speaker;
use super::layered;
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianCompanionWindow");
static CLASS: WindowClass = WindowClass::new();

/// 提示条自动消失的定时器与时长。
const TIMER_NOTICE: usize = 1;
const NOTICE_MS: u32 = 1800;

/// 转圈动画的定时器、间隔与每帧转过的角度。
const TIMER_SPIN: usize = 2;
const SPIN_MS: u32 = 80;
const SPIN_STEP: f32 = 30.0;

/// 滚轮一格滚动的距离（点）。
const WHEEL_STEP: f32 = 36.0;

thread_local! {
    /// 本线程活着的面板窗口：HWND → 窗口。
    static WINDOWS: RefCell<HashMap<isize, Rc<CompanionWindow>>> = RefCell::new(HashMap::new());
}

fn window_of(hwnd: HWND) -> Option<Rc<CompanionWindow>> {
    WINDOWS.with(|map| map.borrow().get(&(hwnd.0 as isize)).cloned())
}

/// 最近一帧的几何，点击命中用。
struct Placed {
    hits: HitMap<CompanionHit>,

    /// 阴影留白（物理像素）。
    margin: i32,

    /// 点 → 物理像素。
    scale: f32,

    max_scroll: f32,
}

struct State {
    view: CompanionView,

    /// 惰性建：第一次画时才加载字体。
    renderer: Option<PanelRenderer>,

    placed: Option<Placed>,

    /// 本次打开时的组句矩形；`None` 表示窗口没显示。
    anchor: Option<RECT>,

    /// 内容左上角的屏幕坐标（物理像素）；`None` 表示下一帧重新定位。
    content_pos: Option<(i32, i32)>,

    /// 用户拖过的位置，之后每次打开都停在这里。
    user_pos: Option<(i32, i32)>,

    speaker: Option<Speaker>,
}

pub struct CompanionWindow {
    hwnd: HWND,
    state: RefCell<State>,
    spinning: Cell<bool>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl CompanionWindow {
    pub fn new(action_cb: Box<dyn Fn(CandidateAction)>) -> Result<Rc<Self>> {
        CLASS.ensure(|| WNDCLASSEXW {
            lpfnWndProc: Some(wndproc::wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Qingjian English Assistant"),
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
        let view = CompanionView {
            pinned: true,
            ..CompanionView::default()
        };
        let window = Rc::new(Self {
            hwnd,
            state: RefCell::new(State {
                view,
                renderer: None,
                placed: None,
                anchor: None,
                content_pos: None,
                user_pos: None,
                speaker: None,
            }),
            spinning: Cell::new(false),
            action_cb,
        });
        WINDOWS.with(|map| map.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&window)));
        Ok(window)
    }

    pub fn is_pinned(&self) -> bool {
        self.state.borrow().view.pinned
    }

    /// 设置置顶状态（面板上的图钉也走这里）。
    pub fn set_pinned(&self, pinned: bool) {
        self.state.borrow_mut().view.pinned = pinned;
        self.apply_z_order();
        self.redraw();
    }

    /// 打开一条新表达：清空旧内容，摆到 `anchor` 附近并显示。
    pub fn show(&self, zh: &str, en: &str, saved: bool, anchor: RECT) {
        {
            let mut state = self.state.borrow_mut();
            model::reset(&mut state.view, zh, en);
            state.view.saved = saved;
            state.anchor = Some(anchor);
            state.content_pos = None;
        }
        self.redraw();
        self.apply_z_order();
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNA) };
    }

    pub fn hide(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.anchor = None;
            state.view.scene_menu_open = false;
        }
        self.stop_timers();
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    /// 一个区块到了。
    pub fn apply_block(&self, tab: usize, scene: usize, block: Block) {
        model::apply_block(&mut self.state.borrow_mut().view, tab, scene, block);
        self.redraw();
    }

    pub fn finish(&self, tab: usize, scene: usize) {
        model::finish(&mut self.state.borrow_mut().view, tab, scene);
        self.redraw();
    }

    pub fn fail(&self, tab: usize, scene: usize, message: String) {
        model::fail(&mut self.state.borrow_mut().view, tab, scene, message);
        self.redraw();
    }

    /// 底部短暂提示；`saved` 为真同时把收藏按钮置为已收藏。
    pub fn set_notice(&self, text: String, saved: bool) {
        {
            let mut state = self.state.borrow_mut();
            state.view.notice = Some(text);
            if saved {
                state.view.saved = true;
            }
        }
        unsafe {
            let _ = SetTimer(Some(self.hwnd), TIMER_NOTICE, NOTICE_MS, None);
        }
        self.redraw();
    }

    /// 画一帧并贴到窗口上；窗口没显示、字体库加载失败都直接略过。
    fn redraw(&self) {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        let Some(anchor) = state.anchor else {
            return;
        };
        let probe = state.content_pos.unwrap_or((anchor.left, anchor.top));
        let dpi = monitor::dpi_near(POINT {
            x: probe.0,
            y: probe.1,
        })
        .unwrap_or(96);
        let scale = dpi as f32 / 96.0;
        if state.renderer.is_none() {
            match FontLibrary::system("zh-CN") {
                Ok(library) => state.renderer = Some(PanelRenderer::new(library)),
                Err(error) => {
                    tracing::warn!(%error, "学习助手面板字体库加载失败，不显示面板");
                    return;
                }
            }
        }
        let Some(renderer) = state.renderer.as_mut() else {
            return;
        };
        renderer.set_scale(scale);
        let frame = match qingjian_render::companion::render(
            renderer,
            &state.view,
            Some(Shadow::mac_panel()),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "学习助手面板绘制失败");
                return;
            }
        };
        state.view.scroll = frame.scroll;
        let margin = (frame.frame.margin * scale).round() as i32;
        let content = (
            frame.frame.pixmap.width() as i32 - 2 * margin,
            frame.frame.pixmap.height() as i32 - 2 * margin,
        );
        let pos = match state.content_pos {
            Some(pos) => pos,
            None => {
                let pos = match state.user_pos {
                    Some(user) => placement::clamp_to_work_area(user, content, margin),
                    None => placement::near_anchor(anchor, content, margin),
                };
                state.content_pos = Some(pos);
                pos
            }
        };
        if let Err(error) = layered::present(
            self.hwnd,
            &frame.frame.pixmap,
            (pos.0 - margin, pos.1 - margin),
        ) {
            tracing::warn!(%error, "学习助手面板贴图失败");
        }
        let busy = model::is_busy(&state.view);
        state.placed = Some(Placed {
            hits: frame.frame.hits,
            margin,
            scale,
            max_scroll: frame.max_scroll,
        });
        drop(guard);
        self.sync_spin(busy);
    }

    /// 按置顶状态摆 z 序，不抢焦点。
    fn apply_z_order(&self) {
        let pinned = self.is_pinned();
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    /// 还有标签在生成就开转圈定时器，都生成完就停。
    fn sync_spin(&self, busy: bool) {
        if busy && !self.spinning.get() {
            self.spinning.set(true);
            unsafe {
                let _ = SetTimer(Some(self.hwnd), TIMER_SPIN, SPIN_MS, None);
            }
        } else if !busy && self.spinning.get() {
            self.spinning.set(false);
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_SPIN);
            }
        }
    }

    fn stop_timers(&self) {
        self.spinning.set(false);
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SPIN);
            let _ = KillTimer(Some(self.hwnd), TIMER_NOTICE);
        }
    }
}
