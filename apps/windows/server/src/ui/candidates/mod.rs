//! 候选窗口：不抢焦点、置顶的分层窗口，跟随光标，画拼音行与候选列表，四周柔和阴影。
//! 缺省交给青简渲染器出位图再贴（[`super::painter`]），配置 `renderer = "system"` 时走 GDI：绘制在 [`view`]，
//! 配色 / 字体在 [`theme`]。绘制内容在 [`RenderData`]，一行的展示形态在 [`row`]。设计语言对齐 macOS 端。

mod render_data;
pub(crate) mod row;
pub(crate) mod theme;
pub(crate) mod view;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{E_INVALIDARG, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HTCLIENT, IDC_ARROW, LoadCursorW,
    MA_NOACTIVATE, SW_HIDE, SW_SHOWNA, ShowWindow, WM_LBUTTONDOWN, WM_MOUSEACTIVATE,
    WM_NCHITTEST, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result, w};

use qingjian_platform::ThemeMode;
use qingjian_platform::protocol::Frame;

pub(crate) use self::render_data::RenderData;
use self::theme::Theme;
use super::layered::{self, Layered};
use super::monitor;
use super::painter::SharedPainter;
use super::window_class::WindowClass;

const CLASS_NAME: PCWSTR = w!("QingjianCandidateWindow");
static CLASS: WindowClass = WindowClass::new();

/// 光标行与候选窗之间的间隙（逻辑像素）。
const CARET_GAP: i32 = 2;

/// 按外观模式解析深浅；`System` 读系统主题。
pub(super) fn resolve_dark(mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::System => system_prefers_dark(),
    }
}

/// `HKCU\...\Themes\Personalize\AppsUseLightTheme` 为 0 是深色；读不到当浅色。
fn system_prefers_dark() -> bool {
    windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("AppsUseLightTheme"))
        .is_ok_and(|value| value == 0)
}

thread_local! {
    /// 本线程活着的候选窗口：HWND → CandidateWindowInner。
    static WINDOWS: RefCell<HashMap<isize, Rc<CandidateWindowInner>>> = RefCell::new(HashMap::new());
}

fn candidate_window_of(hwnd: HWND) -> Option<Rc<CandidateWindowInner>> {
    WINDOWS.with(|map| map.borrow().get(&(hwnd.0 as isize)).cloned())
}

struct CandidateWindowInner {
    hwnd: HWND,
    data: RefCell<RenderData>,
    dpi: Cell<u32>,
    dark: Cell<bool>,
    logged_dpi: Cell<Option<(u32, Option<u32>)>>,
    last_anchor: Cell<Option<RECT>>,
    painter: SharedPainter,
}

impl CandidateWindowInner {
    fn show(&self, anchor: RECT) {
        self.last_anchor.set(Some(anchor));
        self.sync_theme(anchor);
        let rendered = {
            let data = self.data.borrow();
            self.painter.borrow_mut().as_mut().and_then(|painter| {
                painter.render_frame(
                    &data.render_frame(),
                    data.layout,
                    self.dark.get(),
                    self.dpi.get(),
                )
            })
        };
        let updated = match rendered {
            Some(rendered) => {
                let content = (
                    rendered.content_width as i32,
                    rendered.content_height as i32,
                );
                if content.0 <= 0 || content.1 <= 0 {
                    self.hide();
                    return;
                }
                let (content_x, content_y) = place(anchor, content);
                layered::present(
                    self.hwnd,
                    &rendered.pixmap,
                    (
                        content_x - rendered.content_x as i32,
                        content_y - rendered.content_y as i32,
                    ),
                )
            }
            None => self.show_gdi(anchor),
        };
        match &updated {
            Ok(()) => {
                let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNA) };
            }
            Err(error) => {
                tracing::error!(%error, "候选窗口绘制/贴图失败");
                self.hide();
            }
        }
    }

    fn show_gdi(&self, anchor: RECT) -> Result<()> {
        let margin = layered::shadow_margin(self.dpi.get());
        let content = self.preferred_size();
        if content.0 <= 0 || content.1 <= 0 {
            return Err(Error::from(E_INVALIDARG));
        }
        let (content_x, content_y) = place(anchor, content);
        let data = self.data.borrow();
        layered::composite(
            self.hwnd,
            &Layered {
                content,
                margin,
                win_pos: (content_x - margin, content_y - margin),
                win_size: (content.0 + margin * 2, content.1 + margin * 2),
                background: data.theme.background,
                corner_radius: data.theme.corner_radius,
                paint: &|hdc, client| view::paint(hdc, &data, client),
            },
        )
    }

    fn hide(&self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    fn sync_theme(&self, anchor: RECT) {
        let caret = POINT {
            x: anchor.left,
            y: anchor.top,
        };
        let monitor_dpi = monitor::dpi_near(caret);
        let window_dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let dpi = match (monitor_dpi, window_dpi) {
            (Some(dpi), _) => dpi,
            (None, 0) => self.dpi.get(),
            (None, dpi) => dpi,
        };
        self.log_dpi(caret, window_dpi, monitor_dpi, dpi);
        let dark = resolve_dark(self.data.borrow().theme_mode);
        if dpi != self.dpi.get() || dark != self.dark.get() {
            self.data.borrow_mut().theme = Rc::new(Theme::new(dpi, dark));
            self.dpi.set(dpi);
            self.dark.set(dark);
        }
    }

    fn log_dpi(&self, caret: POINT, window_dpi: u32, monitor_dpi: Option<u32>, used: u32) {
        if self.logged_dpi.replace(Some((window_dpi, monitor_dpi)))
            == Some((window_dpi, monitor_dpi))
        {
            return;
        }
        tracing::info!(
            window_dpi,
            ?monitor_dpi,
            used,
            system_dpi = unsafe { GetDpiForSystem() },
            caret_x = caret.x,
            caret_y = caret.y,
            "候选窗口缩放值"
        );
    }

    fn preferred_size(&self) -> (i32, i32) {
        let hdc = unsafe { GetDC(Some(self.hwnd)) };
        let size = view::preferred_size(hdc, &self.data.borrow());
        unsafe { ReleaseDC(Some(self.hwnd), hdc) };
        (size.cx, size.cy)
    }

    fn on_lbutton_down(&self, x: i32, y: i32) {
        let action = {
            let data = self.data.borrow();
            let hdc = unsafe { GetDC(Some(self.hwnd)) };
            let action = view::hit_test_learning_card(hdc, &data, x, y);
            unsafe { ReleaseDC(Some(self.hwnd), hdc) };
            action
        };
        if let Some(action) = action {
            {
                let mut data = self.data.borrow_mut();
                if let Some(card) = data.learning_card.as_mut() {
                    match action {
                        view::LearningHitAction::ToggleExpand => {
                            card.expanded = !card.expanded;
                            if !card.expanded {
                                card.selected_index = None;
                            }
                        }
                        view::LearningHitAction::ToggleAlternative(idx) => {
                            if card.selected_index == Some(idx) {
                                card.selected_index = None;
                            } else {
                                card.selected_index = Some(idx);
                            }
                        }
                        view::LearningHitAction::StartDrill => {
                            card.drill_stage = Some(qingjian_render::DrillStage::Input);
                            card.drill_input.clear();
                            card.drill_attempt = None;
                            card.drill_notice = None;
                        }
                        view::LearningHitAction::Reveal => {
                            let ref_str = card
                                .primary
                                .as_ref()
                                .map(|p| p.text.clone())
                                .unwrap_or_default();
                            card.drill_notice = Some(format!("参考表达: {}", ref_str));
                        }
                        view::LearningHitAction::Compare => {
                            let input = card.drill_input.trim();
                            if input.is_empty() {
                                card.drill_notice = Some("请输入英文表达进行对比".to_string());
                            } else {
                                let ref_str = card
                                    .primary
                                    .as_ref()
                                    .map(|p| p.text.as_str())
                                    .unwrap_or("");
                                let attempt = qingjian_learning::RecallBook::evaluate_attempt(
                                    &card.target_text,
                                    input,
                                    ref_str,
                                );
                                card.drill_attempt = Some(render_data::convert_attempt(&attempt));
                                card.drill_stage = Some(qingjian_render::DrillStage::Compared);
                                card.drill_notice = None;
                            }
                        }
                        view::LearningHitAction::Retry => {
                            card.drill_stage = Some(qingjian_render::DrillStage::Input);
                            card.drill_notice = None;
                            card.drill_attempt = None;
                        }
                        view::LearningHitAction::Save => {
                            card.drill_notice = Some("⭐ 已保存到生词本".to_string());
                        }
                        view::LearningHitAction::Finish => {
                            card.drill_stage = None;
                            card.drill_input.clear();
                            card.drill_attempt = None;
                            card.drill_notice = None;
                        }
                    }
                }
            }
            if let Some(anchor) = self.last_anchor.get() {
                self.show(anchor);
            }
        }
    }
}

/// 候选窗口。内容经 `UpdateLayeredWindow` 一次贴上，窗口过程只走默认处理。
pub(crate) struct CandidateWindow {
    inner: Rc<CandidateWindowInner>,
}

impl CandidateWindow {
    /// 建一个隐藏的候选窗口。
    pub(crate) fn new(painter: SharedPainter) -> Result<Self> {
        CLASS.ensure(|| WNDCLASSEXW {
            lpfnWndProc: Some(wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        let dpi = unsafe { GetDpiForSystem() }.max(96);
        let dark = resolve_dark(ThemeMode::default());
        let data = RefCell::new(RenderData::empty(Rc::new(Theme::new(dpi, dark))));
        // NOACTIVATE：显示时不抢应用焦点。
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("青简候选"),
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
        let inner = Rc::new(CandidateWindowInner {
            hwnd,
            data,
            dpi: Cell::new(dpi),
            dark: Cell::new(dark),
            logged_dpi: Cell::new(None),
            last_anchor: Cell::new(None),
            painter,
        });
        WINDOWS.with(|map| map.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&inner)));
        Ok(Self { inner })
    }

    /// 刷新内容（不定位、不显示）。
    pub(crate) fn set_content(&self, frame: &Frame) {
        self.inner.data.borrow_mut().set(frame);
    }

    /// 按光标矩形定位并显示：贴光标下方（放不下放上方），四周留出阴影。
    pub(crate) fn show(&self, anchor: RECT) {
        self.inner.show(anchor);
    }

    pub(crate) fn hide(&self) {
        self.inner.hide();
    }
}

impl Drop for CandidateWindow {
    fn drop(&mut self) {
        WINDOWS.with(|map| map.borrow_mut().remove(&(self.inner.hwnd.0 as isize)));
        let _ = unsafe { DestroyWindow(self.inner.hwnd) };
    }
}

/// 内容左上角：贴光标下方，放不下放上方，再放不下贴屏幕内；都夹在所在显示器工作区里。
fn place(anchor: RECT, content: (i32, i32)) -> (i32, i32) {
    let work = monitor::work_area_near(POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let x = anchor
        .left
        .clamp(work.left, (work.right - content.0).max(work.left));
    let below = anchor.bottom + CARET_GAP;
    let above = anchor.top - CARET_GAP - content.1;
    let y = if below + content.1 <= work.bottom {
        below
    } else if above >= work.top && above + content.1 <= work.bottom {
        above
    } else {
        (work.bottom - content.1).max(work.top)
    };
    let y = y.clamp(work.top, (work.bottom - content.1).max(work.top));
    (x, y)
}

/// 分层窗口过程：鼠标消息不抢焦点（MA_NOACTIVATE），支持点击展开更多译法。
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            if let Some(inner) = candidate_window_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                inner.on_lbutton_down(x, y);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place_bottom_overflow() {
        let content = (300, 60);
        // 模拟光标超出版面底部 (如 1164 超过 1104/1152)
        let anchor = RECT {
            left: 580,
            top: 1164,
            right: 600,
            bottom: 1184,
        };
        let (x, y) = place(anchor, content);
        let work = monitor::work_area_near(POINT {
            x: anchor.left,
            y: anchor.top,
        });
        assert!(x >= work.left && x + content.0 <= work.right.max(work.left + content.0));
        assert!(y >= work.top, "y ({y}) 必须 >= work.top ({})", work.top);
        assert!(
            y + content.1 <= work.bottom.max(work.top + content.1),
            "y + content.1 ({} + {}) 必须 <= work.bottom ({})",
            y,
            content.1,
            work.bottom
        );
    }
}

