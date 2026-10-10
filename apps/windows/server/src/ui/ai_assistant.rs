//! Qingjian AI 对话窗口 (Qwen 2.5 7B 专享推理流式对话)
//! 集成 Win32 原生 Caret、TSF 光标锚定、全功能 Markdown 渲染、Claude/DeepSeek 极简现代化界面。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetDC, GetTextExtentPoint32W, HDC, HFONT, IntersectClipRect,
    ReleaseDC, RestoreDC, SaveDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::Ime::{
    ImmGetContext, ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow,
    CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateCaret, CreateWindowExW, DefWindowProcW, DestroyCaret, GetWindowRect,
    HTCAPTION, HTCLIENT, IDC_ARROW, IsIconic, LoadCursorW, MA_ACTIVATE,
    SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SendMessageW, SetCaretPos, SetForegroundWindow,
    SetWindowPos, ShowCaret, ShowWindow, WNDCLASSEXW, CS_DBLCLKS, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_TOPMOST, WS_POPUP, HWND_TOPMOST, HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
    WM_CHAR, WM_DESTROY, WM_EXITSIZEMOVE, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_MOVE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_SETFOCUS,
    WM_IME_COMPOSITION, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION,
};
use windows::core::{PCWSTR, Result, w};

use qingjian_predict::SearchResult;

use super::layered::{self, Layered};
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianAiAssistantWindow");
static CLASS: WindowClass = WindowClass::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    Fast,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingState {
    Idle,
    Sending,
    Streaming,
    Error,
}

#[derive(Debug, Clone)]
pub struct ChatBubble {
    pub is_user: bool,
    pub text: String,
    pub web_sources: Vec<SearchResult>,
    pub ttft_ms: Option<u128>,
    pub total_ms: Option<u128>,
}

#[derive(Debug, Clone)]
pub struct AiAssistantData {
    pub session_id: String,
    pub messages: Vec<ChatBubble>,
    pub input_text: String,
    pub chat_mode: ChatMode,
    pub web_search_enabled: bool,
    pub state: StreamingState,
    pub error_message: Option<String>,
    pub is_pinned: bool,
}

impl Default for AiAssistantData {
    fn default() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            input_text: String::new(),
            chat_mode: ChatMode::Fast,
            web_search_enabled: false,
            state: StreamingState::Idle,
            error_message: None,
            is_pinned: true,
        }
    }
}

pub struct AiAssistantWindow {
    pub hwnd: HWND,
    pub data: RefCell<AiAssistantData>,
    last_anchor: Cell<Option<RECT>>,
    user_pos: Cell<Option<(i32, i32)>>,
    current_width: Cell<i32>,
    current_height: Cell<i32>,
    scroll_y: Cell<i32>,
    scroll_to_bottom_pending: Cell<bool>,
    dark: Cell<bool>,
    is_maximized: Cell<bool>,
    normal_bounds: Cell<Option<(i32, i32, i32, i32)>>,
    is_visible: Cell<bool>,
    last_render_time: Cell<Instant>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl AiAssistantWindow {
    pub fn new(action_cb: Box<dyn Fn(CandidateAction)>) -> Result<Rc<Self>> {
        CLASS.ensure(|| WNDCLASSEXW {
            lpfnWndProc: Some(wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            style: CS_DBLCLKS,
            ..Default::default()
        })?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW | WS_EX_LAYERED | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Qingjian AI"),
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
        let instance = Rc::new(Self {
            hwnd,
            data: RefCell::new(AiAssistantData::default()),
            last_anchor: Cell::new(None),
            user_pos: Cell::new(None),
            current_width: Cell::new(760),
            current_height: Cell::new(820),
            scroll_y: Cell::new(0),
            scroll_to_bottom_pending: Cell::new(false),
            dark: Cell::new(false),
            is_maximized: Cell::new(false),
            normal_bounds: Cell::new(None),
            is_visible: Cell::new(false),
            last_render_time: Cell::new(Instant::now()),
            action_cb,
        });
        AI_WINDOWS.with(|w| w.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&instance)));
        Ok(instance)
    }

    /// 打开或聚焦 AI 助手窗口
    pub fn show_or_focus(&self, anchor: Option<RECT>, initial_prompt: Option<String>) {
        self.is_visible.set(true);
        if let Some(p) = initial_prompt {
            if !p.is_empty() {
                let mut d = self.data.borrow_mut();
                d.input_text = p;
            }
        }
        self.scroll_to_bottom_pending.set(true);
        let _ = self.render_and_show(anchor);
        unsafe {
            if IsIconic(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
            }
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
            let _ = BringWindowToTop(self.hwnd);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
        self.update_caret_pos();
    }

    /// 计算输入框内光标在窗口客户区中的精确像素位置
    pub fn get_caret_position(&self) -> (i32, i32) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        let scale = dpi as f32 / 96.0;
        let px = |val: i32| (val as f32 * scale).round() as i32;
        let is_max = self.is_maximized.get();
        let margin = if is_max { 0 } else { layered::shadow_margin(dpi) };
        let content_w = self.current_width.get();
        let content_h = self.current_height.get();
        let dock_top = content_h - px(132);
        let text_start_x = px(28);
        let text_start_y = dock_top + px(20);

        let data = self.data.borrow();
        if data.input_text.is_empty() {
            return (margin + text_start_x, margin + text_start_y);
        }

        let hdc = unsafe { GetDC(Some(self.hwnd)) };
        if hdc.is_invalid() {
            return (margin + text_start_x, margin + text_start_y);
        }
        let font = create_font(px(13), FW_NORMAL, false);
        let max_w = content_w - px(68);
        let lines = wrap_text_lines(hdc, font, max_w, &data.input_text);
        let line_count = lines.len().max(1).min(3);
        let last_line = lines.get(line_count - 1).cloned().unwrap_or_default();
        let utf16: Vec<u16> = last_line.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            SelectObject(hdc, font.into());
            let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
            let _ = DeleteObject(font.into());
            let _ = ReleaseDC(Some(self.hwnd), hdc);
        }
        let cur_x = text_start_x + size.cx;
        let cur_y = text_start_y + ((line_count - 1) as i32) * px(20);
        (margin + cur_x, margin + cur_y)
    }

    /// 同步更新 Win32 原生 Caret 与 IMM32 IME 锚定，确保 Windows TSF / IME 准确锚定候选窗口在输入框内
    pub fn update_caret_pos(&self) {
        if !self.is_visible.get() {
            return;
        }
        let (cx, cy) = self.get_caret_position();
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        let scale = dpi as f32 / 96.0;
        let px = |val: i32| (val as f32 * scale).round() as i32;
        let caret_h = px(18);
        unsafe {
            let _ = CreateCaret(self.hwnd, None, 2, caret_h);
            let _ = SetCaretPos(cx, cy);
            let _ = ShowCaret(Some(self.hwnd));

            let himc = ImmGetContext(self.hwnd);
            if !himc.is_invalid() {
                let mut comp = COMPOSITIONFORM {
                    dwStyle: CFS_POINT,
                    ptCurrentPos: POINT { x: cx, y: cy },
                    rcArea: RECT::default(),
                };
                let _ = ImmSetCompositionWindow(himc, &mut comp);

                let mut cand = CANDIDATEFORM {
                    dwIndex: 0,
                    dwStyle: CFS_CANDIDATEPOS,
                    ptCurrentPos: POINT { x: cx, y: cy + caret_h + 2 },
                    rcArea: RECT::default(),
                };
                let _ = ImmSetCandidateWindow(himc, &mut cand);
                let _ = ImmReleaseContext(self.hwnd, himc);
            }
        }
    }

    /// 追加流式文本增量 (节流刷新 + 自动贴底滚动 + 关闭态防意外唤醒)
    pub fn append_stream_chunk(&self, delta: &str) {
        let mut d = self.data.borrow_mut();
        d.state = StreamingState::Streaming;
        if let Some(last) = d.messages.last_mut() {
            if !last.is_user {
                last.text.push_str(delta);
            } else {
                d.messages.push(ChatBubble {
                    is_user: false,
                    text: delta.to_string(),
                    web_sources: Vec::new(),
                    ttft_ms: None,
                    total_ms: None,
                });
            }
        } else {
            d.messages.push(ChatBubble {
                is_user: false,
                text: delta.to_string(),
                web_sources: Vec::new(),
                ttft_ms: None,
                total_ms: None,
            });
        }
        drop(d);

        // 若面板已被用户关闭或隐藏，静默累加文本，绝不强行弹窗打扰用户
        if !self.is_visible.get() || unsafe { IsIconic(self.hwnd).as_bool() } {
            return;
        }

        // 35ms 节流刷新：避免每个 chunk 高频触发 GDI 位图全量合成导致的 UI 卡顿
        let now = Instant::now();
        if now.duration_since(self.last_render_time.get()) < std::time::Duration::from_millis(35) {
            return;
        }
        self.last_render_time.set(now);

        self.scroll_to_bottom_pending.set(true);
        let _ = self.render_and_show(self.last_anchor.get());
    }

    /// 设置联网搜索来源
    pub fn set_web_sources(&self, sources: Vec<SearchResult>) {
        let mut d = self.data.borrow_mut();
        if let Some(last) = d.messages.last_mut() {
            if !last.is_user {
                last.web_sources = sources;
            }
        }
        drop(d);
        if self.is_visible.get() && !unsafe { IsIconic(self.hwnd).as_bool() } {
            self.scroll_to_bottom_pending.set(true);
            let _ = self.render_and_show(self.last_anchor.get());
        }
    }

    /// 流式完成通知
    pub fn finish_stream(&self, ttft_ms: u128, total_ms: u128) {
        let mut d = self.data.borrow_mut();
        d.state = StreamingState::Idle;
        if let Some(last) = d.messages.last_mut() {
            if !last.is_user {
                last.ttft_ms = Some(ttft_ms);
                last.total_ms = Some(total_ms);
            }
        }
        drop(d);
        if self.is_visible.get() && !unsafe { IsIconic(self.hwnd).as_bool() } {
            self.scroll_to_bottom_pending.set(true);
            let _ = self.render_and_show(self.last_anchor.get());
        }
    }

    /// 流式错误通知
    pub fn set_stream_error(&self, error: &str) {
        let mut d = self.data.borrow_mut();
        d.state = StreamingState::Error;
        d.error_message = Some(error.to_string());
        if let Some(last) = d.messages.last_mut() {
            if !last.is_user {
                if last.text.is_empty() {
                    last.text = format!("❌ 请求失败: {}", error);
                } else {
                    last.text.push_str(&format!("\n\n[中断: {}]", error));
                }
            }
        }
        drop(d);
        if self.is_visible.get() && !unsafe { IsIconic(self.hwnd).as_bool() } {
            self.scroll_to_bottom_pending.set(true);
            let _ = self.render_and_show(self.last_anchor.get());
        }
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible.get()
    }

    /// 隐藏窗口
    pub fn hide(&self) {
        self.is_visible.set(false);
        self.last_anchor.set(None);
        unsafe {
            let _ = DestroyCaret();
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// 用户主动关闭窗口并向 Router 报告
    pub fn close_and_notify(&self) {
        self.hide();
        (self.action_cb)(CandidateAction::CloseAiAssistant);
    }

    /// 最小化窗口至任务栏
    pub fn minimize(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
        }
    }

    /// 切换置顶状态
    pub fn toggle_pin(&self) {
        let pinned = {
            let mut d = self.data.borrow_mut();
            d.is_pinned = !d.is_pinned;
            d.is_pinned
        };
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                if pinned { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) },
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        let _ = self.render_and_show(self.last_anchor.get());
    }

    /// 切换最大化/还原
    pub fn toggle_maximize(&self) {
        if self.is_maximized.get() {
            if let Some((nx, ny, nw, nh)) = self.normal_bounds.get() {
                self.current_width.set(nw);
                self.current_height.set(nh);
                self.user_pos.set(Some((nx, ny)));
            } else {
                let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
                let scale = dpi as f32 / 96.0;
                self.current_width.set((760.0 * scale).round() as i32);
                self.current_height.set((820.0 * scale).round() as i32);
            }
            self.is_maximized.set(false);
        } else {
            let mut rc = RECT::default();
            let _ = unsafe { GetWindowRect(self.hwnd, &mut rc) };
            let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
            let margin = layered::shadow_margin(dpi);
            let cur_w = self.current_width.get();
            let cur_h = self.current_height.get();
            let cur_pos = self.user_pos.get().unwrap_or((rc.left + margin, rc.top + margin));
            self.normal_bounds.set(Some((cur_pos.0, cur_pos.1, cur_w, cur_h)));

            let center = POINT {
                x: (rc.left + rc.right) / 2,
                y: (rc.top + rc.bottom) / 2,
            };
            let work = monitor::work_area_near(center);
            let max_w = (work.right - work.left).max(400);
            let max_h = (work.bottom - work.top).max(300);
            self.current_width.set(max_w);
            self.current_height.set(max_h);
            self.user_pos.set(Some((work.left, work.top)));
            self.is_maximized.set(true);
        }
        let _ = self.render_and_show(self.last_anchor.get());
        self.update_caret_pos();
    }

    /// 开启新会话
    pub fn new_session(&self) {
        let mut d = self.data.borrow_mut();
        d.session_id = uuid::Uuid::new_v4().to_string();
        d.messages.clear();
        d.input_text.clear();
        d.state = StreamingState::Idle;
        d.error_message = None;
        drop(d);
        self.scroll_y.set(0);
        self.scroll_to_bottom_pending.set(false);
        (self.action_cb)(CandidateAction::AiChatNewSession);
        let _ = self.render_and_show(self.last_anchor.get());
        self.update_caret_pos();
    }

    /// 发送消息或中断生成
    pub fn send_or_stop(&self) {
        let mut d = self.data.borrow_mut();
        if d.state == StreamingState::Streaming {
            d.state = StreamingState::Idle;
            let sid = d.session_id.clone();
            drop(d);
            (self.action_cb)(CandidateAction::AiChatStop { session_id: sid });
        } else {
            let prompt = d.input_text.trim().to_string();
            if !prompt.is_empty() {
                d.messages.push(ChatBubble {
                    is_user: true,
                    text: prompt.clone(),
                    web_sources: Vec::new(),
                    ttft_ms: None,
                    total_ms: None,
                });
                d.input_text.clear();
                d.state = StreamingState::Sending;
                let sid = d.session_id.clone();
                drop(d);
                self.scroll_to_bottom_pending.set(true);
                // 统一全智能模式发送，直出高质量深邃且结构化的 Markdown 解答
                (self.action_cb)(CandidateAction::AiChatSend {
                    prompt,
                    fast_mode: false,
                    web_search: false,
                    session_id: sid,
                });
            }
        }
        let _ = self.render_and_show(self.last_anchor.get());
        self.update_caret_pos();
    }

    pub fn render_and_show(&self, anchor: Option<RECT>) -> Result<()> {
        if let Some(a) = anchor {
            self.last_anchor.set(Some(a));
        }
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        let scale = dpi as f32 / 96.0;
        let is_max = self.is_maximized.get();
        let margin = if is_max { 0 } else { layered::shadow_margin(dpi) };
        let corner_radius = if is_max { 0 } else { 12 };

        if !is_max && self.user_pos.get().is_none() {
            let logical_w = 760;
            let logical_h = 820;
            let content_w = (logical_w as f32 * scale).round() as i32;
            let content_h = (logical_h as f32 * scale).round() as i32;
            self.current_width.set(content_w);
            self.current_height.set(content_h);
        }

        let content = (self.current_width.get(), self.current_height.get());

        let (pos_x, pos_y) = if is_max {
            self.user_pos.get().unwrap_or((0, 0))
        } else if let Some(up) = self.user_pos.get() {
            clamp_position_to_monitor(up, content, margin)
        } else if let Some(a) = self.last_anchor.get() {
            calculate_ai_position(a, content, margin)
        } else {
            let work = monitor::work_area_near(POINT { x: 500, y: 300 });
            let cx = (work.right - content.0 - margin - 40).max(work.left + margin);
            let cy = (work.bottom - content.1 - margin - 60).max(work.top + margin);
            (cx, cy)
        };

        let data = self.data.borrow();
        let bg_color = if self.dark.get() {
            COLORREF(0x001E1E1E)
        } else {
            COLORREF(0x00FFFFFF)
        };

        layered::composite(
            self.hwnd,
            &Layered {
                content,
                margin,
                win_pos: if is_max { (pos_x, pos_y) } else { (pos_x - margin, pos_y - margin) },
                win_size: if is_max { content } else { (content.0 + margin * 2, content.1 + margin * 2) },
                background: bg_color,
                corner_radius,
                paint: &|hdc, client| {
                    paint_ai_assistant(
                        hdc,
                        &data,
                        client,
                        self.dark.get(),
                        scale,
                        &self.scroll_y,
                        &self.scroll_to_bottom_pending,
                        is_max,
                    );
                },
            },
        )?;

        let is_pinned = data.is_pinned;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                if is_pinned {
                    Some(HWND_TOPMOST)
                } else {
                    Some(HWND_NOTOPMOST)
                },
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            if is_pinned {
                let _ = BringWindowToTop(self.hwnd);
            }
        }
        Ok(())
    }
}

thread_local! {
    static AI_WINDOWS: RefCell<std::collections::HashMap<isize, Rc<AiAssistantWindow>>> = RefCell::new(std::collections::HashMap::new());
}

fn clamp_position_to_monitor(pos: (i32, i32), content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(POINT {
        x: pos.0,
        y: pos.1,
    });
    let x = pos.0.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
    let y = pos.1.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
    (x, y)
}

fn calculate_ai_position(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let right_x = anchor.right + 16;
    if right_x + content.0 + margin <= work.right {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (right_x, y);
    }
    let cx = (work.right - content.0 - margin - 30).max(work.left + margin);
    let cy = (anchor.top - 80).clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
    (cx, cy)
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

fn create_font(size: i32, weight: windows::Win32::Graphics::Gdi::FONT_WEIGHT, italic: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight.0 as i32,
            italic as u32,
            0,
            0,
            windows::Win32::Graphics::Gdi::FONT_CHARSET(1),
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(5), // CLEARTYPE_QUALITY
            0,
            w!("Microsoft YaHei UI"),
        )
    }
}

fn create_code_font(size: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FONT_CHARSET(1),
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(5),
            0,
            w!("Consolas"),
        )
    }
}

fn draw_text_gdi(hdc: HDC, font: HFONT, color: COLORREF, x: i32, y: i32, text: &str) -> i32 {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        let _ = TextOutW(hdc, x, y, &utf16);
        let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
    }
    size.cx
}

pub use super::markdown::*;

fn paint_ai_assistant(
    hdc: HDC,
    data: &AiAssistantData,
    client: RECT,
    dark: bool,
    scale: f32,
    scroll_y_cell: &Cell<i32>,
    scroll_pending_cell: &Cell<bool>,
    is_maximized: bool,
) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };
    let px = |val: i32| (val as f32 * scale).round() as i32;

    // 配色体系：Claude / DeepSeek 极简现代化 Slate 高对比度色调
    let header_bg = if dark { rgb(30, 41, 59) } else { rgb(248, 250, 252) };
    let header_border = if dark { rgb(51, 65, 85) } else { rgb(229, 231, 235) };
    let text_color = if dark { rgb(241, 245, 249) } else { rgb(17, 24, 39) };
    let sub_color = if dark { rgb(107, 114, 128) } else { rgb(75, 85, 99) };
    let muted_color = if dark { rgb(148, 163, 184) } else { rgb(156, 163, 175) };
    let brand_color = rgb(37, 99, 235); // #2563EB shared with learning panel
    let brand_light = if dark { rgb(30, 58, 138) } else { rgb(235, 242, 255) };
    let brand_border = rgb(37, 99, 235);

    let card_bg = if dark { rgb(30, 41, 59) } else { rgb(248, 250, 252) };
    let card_border = if dark { rgb(51, 65, 85) } else { rgb(229, 231, 235) };
    let user_bubble_bg = if dark { rgb(30, 58, 138) } else { rgb(235, 242, 255) };
    let user_bubble_border = if dark { rgb(51, 102, 225) } else { rgb(219, 230, 255) };
    let user_text = if dark { rgb(241, 245, 249) } else { rgb(17, 24, 39) };

    let code_bg = if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) };
    let code_color = if dark { rgb(147, 197, 253) } else { rgb(37, 99, 235) };
    let code_block_bg = if dark { rgb(15, 23, 42) } else { rgb(248, 250, 252) };
    let code_block_border = if dark { rgb(51, 65, 85) } else { rgb(229, 231, 235) };

    let font_brand = create_font(px(14), FW_BOLD, false);
    let font_header_title = create_font(px(15), FW_BOLD, false);
    let font_header_sub = create_font(px(14), FW_BOLD, false);
    let font_btn = create_font(px(12), FW_NORMAL, false);
    let font_msg = create_font(px(14), FW_NORMAL, false);
    let font_msg_bold = create_font(px(14), FW_BOLD, false);
    let font_code = create_code_font(px(12));
    let font_small = create_font(px(11), FW_NORMAL, false);
    let font_small_bold = create_font(px(11), FW_BOLD, false);

    // ==========================================
    // 1. 顶部标题栏 (高度 px(44))
    // ==========================================
    let header_rect = RECT { left: 0, top: 0, right: client.right, bottom: px(44) };
    super::candidates::view::fill_rect(hdc, header_rect, header_bg);
    let sep_top = RECT { left: 0, top: px(44), right: client.right, bottom: px(45) };
    super::candidates::view::fill_rect(hdc, sep_top, header_border);

    let mut hx = px(16);
    draw_text_gdi(hdc, font_brand, brand_color, hx, px(12), "✦");
    hx += px(16);
    draw_text_gdi(hdc, font_brand, text_color, hx, px(12), "青简 AI 助手");
    hx += px(96);
    draw_text_gdi(hdc, font_small, muted_color, hx, px(14), "· Qwen3 14B");

    // 右侧操作群：新会话 ✎ | 置顶 📌 | 最小化 — | 最大化/还原 🗖/⧉ | 关闭 ✕
    let btn_w = px(28);
    let gap = px(4);
    let mr = px(12);
    let by = px(8);
    let bh = px(28);

    // 1.1 新对话 ✎
    let b5_x = client.right - mr - 5 * btn_w - 4 * gap;
    draw_text_gdi(hdc, font_btn, sub_color, b5_x + px(7), by + px(5), "✎");

    // 1.2 置顶 📌
    let b4_x = client.right - mr - 4 * btn_w - 3 * gap;
    if data.is_pinned {
        let pin_r = RECT { left: b4_x, top: by, right: b4_x + btn_w, bottom: by + bh };
        super::candidates::view::fill_round_rect(hdc, pin_r, brand_light, 4);
        let border_pin = RECT { left: b4_x, top: by, right: b4_x + btn_w, bottom: by + 1 };
        super::candidates::view::fill_rect(hdc, border_pin, brand_border);
        draw_text_gdi(hdc, font_btn, brand_color, b4_x + px(7), by + px(5), "📌");
    } else {
        draw_text_gdi(hdc, font_btn, sub_color, b4_x + px(7), by + px(5), "📌");
    }

    // 1.3 最小化 —
    let b3_x = client.right - mr - 3 * btn_w - 2 * gap;
    draw_text_gdi(hdc, font_btn, sub_color, b3_x + px(8), by + px(5), "—");

    // 1.4 最大化/还原 🗖 / ⧉
    let b2_x = client.right - mr - 2 * btn_w - gap;
    let max_icon = if is_maximized { "⧉" } else { "🗖" };
    draw_text_gdi(hdc, font_btn, sub_color, b2_x + px(7), by + px(5), max_icon);

    // 1.5 关闭 ✕
    let b1_x = client.right - mr - btn_w;
    draw_text_gdi(hdc, font_btn, sub_color, b1_x + px(8), by + px(5), "✕");

    // ==========================================
    // 2. 测量消息流并计算视口滚动几何
    // ==========================================
    if data.messages.is_empty() {
        let max_w = (client.right - px(80)).max(px(180));
        let hero_y = px(190);
        draw_text_gdi(hdc, font_header_title, text_color, px(36), hero_y, "今天想学点什么？");
        draw_text_gdi(hdc, font_msg, sub_color, px(36), hero_y + px(32), "输入问题、短语或中文表达，青简会帮你拆解含义与用法。");
        let cards = [
            ("口语表达", "这句话怎么说得更自然？"),
            ("语法与语感", "解释语气差异并给出例句"),
            ("雅思口语", "用这个表达练习一段对话"),
        ];
        let mut cy = hero_y + px(76);
        for (label, detail) in cards {
            let rect = RECT { left: px(36), top: cy, right: px(36) + max_w, bottom: cy + px(56) };
            super::candidates::view::fill_round_rect(hdc, rect, card_bg, 8);
            let edge = RECT { left: rect.left, top: rect.top, right: rect.left + px(3), bottom: rect.bottom };
            super::candidates::view::fill_round_rect(hdc, edge, brand_color, 2);
            draw_text_gdi(hdc, font_small_bold, brand_color, rect.left + px(14), cy + px(9), label);
            draw_text_gdi(hdc, font_msg, text_color, rect.left + px(14), cy + px(30), detail);
            cy += px(66);
        }
    }
    let view_top = px(56);
    let view_bottom = client.bottom - px(132);
    let view_h = (view_bottom - view_top).max(100);

    let mut total_content_height = 0;
    for msg in &data.messages {
        if msg.is_user {
            let max_w = (client.right * 7 / 10).min(px(440));
            let lines = wrap_text_lines(hdc, font_msg, max_w - px(24), &msg.text);
            let text_h = (lines.len() as i32 * px(20)).max(px(20));
            let bubble_h = text_h + px(16);
            total_content_height += bubble_h + px(16);
        } else {
            let max_w = client.right - px(48);
            let blocks = parse_markdown_blocks(&msg.text);
            let mut item_h = px(24);
            for b in &blocks {
                match b {
                    MarkdownBlock::Header { .. } => {
                        item_h += px(28);
                    }
                    MarkdownBlock::BulletItem { text: spans }
                    | MarkdownBlock::NumberedItem { text: spans, .. }
                    | MarkdownBlock::Quote { text: spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w - px(20), spans);
                        item_h += (lines.len() as i32 * px(22)).max(px(22)) + px(4);
                    }
                    MarkdownBlock::Paragraph { spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w, spans);
                        item_h += (lines.len() as i32 * px(22)).max(px(22)) + px(6);
                    }
                    MarkdownBlock::CodeBlock { lines, .. } => {
                        item_h += (lines.len() as i32 * px(18)).max(px(18)) + px(16);
                    }
                }
            }
            if !msg.web_sources.is_empty() {
                item_h += px(20) + (msg.web_sources.len().min(3) as i32) * px(18);
            }
            if msg.ttft_ms.is_some() || msg.total_ms.is_some() {
                item_h += px(20);
            }
            total_content_height += item_h + px(20);
        }
    }

    let max_scroll = (total_content_height - view_h).max(0);
    if scroll_pending_cell.get() {
        scroll_y_cell.set(max_scroll);
        scroll_pending_cell.set(false);
    }
    let scroll_offset = scroll_y_cell.get().clamp(0, max_scroll);
    scroll_y_cell.set(scroll_offset);

    // ==========================================
    // 3. 中间消息滚动视口 (SaveDC + IntersectClipRect 绝对裁剪隔离)
    // ==========================================
    let saved_dc = unsafe { SaveDC(hdc) };
    unsafe {
        let _ = IntersectClipRect(hdc, 0, view_top, client.right, view_bottom);
    }

    let mut my = view_top - scroll_offset;
    for msg in &data.messages {
        if msg.is_user {
            let max_w = (client.right * 7 / 10).min(px(440));
            let lines = wrap_text_lines(hdc, font_msg, max_w - px(24), &msg.text);
            let text_h = (lines.len() as i32 * px(20)).max(px(20));
            let bubble_w = max_w;
            let bubble_h = text_h + px(16);
            let bx = client.right - px(20) - bubble_w;

            if my + bubble_h >= view_top && my <= view_bottom {
                let r = RECT { left: bx, top: my, right: bx + bubble_w, bottom: my + bubble_h };
                super::candidates::view::fill_round_rect(hdc, r, user_bubble_bg, 8);
                let border_r = RECT { left: bx, top: my, right: bx + bubble_w, bottom: my + 1 };
                super::candidates::view::fill_rect(hdc, border_r, user_bubble_border);

                let mut ly = my + px(8);
                for line in &lines {
                    draw_text_gdi(hdc, font_msg, user_text, bx + px(12), ly, line);
                    ly += px(20);
                }
            }
            my += bubble_h + px(16);
        } else {
            let label = if data.state == StreamingState::Streaming {
                "✦ QINGJIAN AI · 正在思考生成……"
            } else {
                "✦ QINGJIAN AI"
            };
            if my >= view_top - px(24) && my <= view_bottom {
                draw_text_gdi(hdc, font_small_bold, brand_color, px(20), my, label);
            }
            my += px(24);

            let max_w = client.right - px(48);
            let blocks = parse_markdown_blocks(&msg.text);
            for b in &blocks {
                match b {
                    MarkdownBlock::Header { level, text } => {
                        let font = if *level <= 2 { font_header_title } else { font_header_sub };
                        if my >= view_top - px(28) && my <= view_bottom {
                            draw_text_gdi(hdc, font, text_color, px(20), my, text);
                        }
                        my += px(28);
                    }
                    MarkdownBlock::BulletItem { text: spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w - px(20), spans);
                        for (l_idx, line) in lines.iter().enumerate() {
                            if my >= view_top - px(22) && my <= view_bottom {
                                if l_idx == 0 {
                                    draw_text_gdi(hdc, font_msg_bold, brand_color, px(20), my, "•");
                                }
                                render_spans_line(hdc, font_msg, font_msg_bold, font_code, text_color, code_color, code_bg, px(32), my, &line.spans, px);
                            }
                            my += px(22);
                        }
                        my += px(4);
                    }
                    MarkdownBlock::NumberedItem { number, text: spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w - px(24), spans);
                        let num_prefix = format!("{}.", number);
                        for (l_idx, line) in lines.iter().enumerate() {
                            if my >= view_top - px(22) && my <= view_bottom {
                                if l_idx == 0 {
                                    draw_text_gdi(hdc, font_small_bold, sub_color, px(20), my, &num_prefix);
                                }
                                render_spans_line(hdc, font_msg, font_msg_bold, font_code, text_color, code_color, code_bg, px(36), my, &line.spans, px);
                            }
                            my += px(22);
                        }
                        my += px(4);
                    }
                    MarkdownBlock::Quote { text: spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w - px(24), spans);
                        let quote_h = (lines.len() as i32 * px(22)).max(px(22));
                        if my >= view_top - quote_h && my <= view_bottom {
                            let bar_r = RECT { left: px(20), top: my, right: px(23), bottom: my + quote_h };
                            super::candidates::view::fill_rect(hdc, bar_r, brand_color);
                        }
                        for line in &lines {
                            if my >= view_top - px(22) && my <= view_bottom {
                                render_spans_line(hdc, font_msg, font_msg_bold, font_code, sub_color, code_color, code_bg, px(30), my, &line.spans, px);
                            }
                            my += px(22);
                        }
                        my += px(4);
                    }
                    MarkdownBlock::Paragraph { spans } => {
                        let lines = wrap_formatted_spans(hdc, font_msg, font_msg_bold, font_code, max_w, spans);
                        for line in &lines {
                            if my >= view_top - px(22) && my <= view_bottom {
                                render_spans_line(hdc, font_msg, font_msg_bold, font_code, text_color, code_color, code_bg, px(20), my, &line.spans, px);
                            }
                            my += px(22);
                        }
                        my += px(6);
                    }
                    MarkdownBlock::CodeBlock { lines, .. } => {
                        let block_h = (lines.len() as i32 * px(18)).max(px(18)) + px(16);
                        if my + block_h >= view_top && my <= view_bottom {
                            let r = RECT { left: px(20), top: my, right: client.right - px(20), bottom: my + block_h };
                            super::candidates::view::fill_round_rect(hdc, r, code_block_bg, 6);
                            let border_r = RECT { left: px(20), top: my, right: client.right - px(20), bottom: my + 1 };
                            super::candidates::view::fill_rect(hdc, border_r, code_block_border);
                            let mut cy = my + px(8);
                            for l in lines {
                                draw_text_gdi(hdc, font_code, text_color, px(28), cy, l);
                                cy += px(18);
                            }
                        }
                        my += block_h + px(8);
                    }
                }
            }
            my += px(4);

            // 网络检索来源卡片
            if !msg.web_sources.is_empty() {
                if my >= view_top - px(20) && my <= view_bottom {
                    draw_text_gdi(hdc, font_small_bold, brand_color, px(20), my, "🌐 参考来源 (真实网络检索)：");
                }
                my += px(18);
                for (s_idx, src) in msg.web_sources.iter().take(3).enumerate() {
                    let s_txt = format!("{}. {} ({})", s_idx + 1, src.title, src.url);
                    if my >= view_top - px(20) && my <= view_bottom {
                        draw_text_gdi(hdc, font_small, sub_color, px(28), my, &s_txt);
                    }
                    my += px(18);
                }
            }

            // 延迟统计
            if let (Some(ttft), Some(tot)) = (msg.ttft_ms, msg.total_ms) {
                let stat_txt = format!("⚡ 首字延迟: {}ms · 总耗时: {}ms", ttft, tot);
                if my >= view_top - px(20) && my <= view_bottom {
                    draw_text_gdi(hdc, font_small, muted_color, px(20), my, &stat_txt);
                }
                my += px(18);
            }
            my += px(18);
        }
    }

    unsafe {
        let _ = RestoreDC(hdc, saved_dc);
    }

    // 滚动条滑块
    if total_content_height > view_h {
        let track_top = view_top + px(4);
        let track_bottom = view_bottom - px(4);
        let track_h = track_bottom - track_top;
        let thumb_h = ((view_h as f32 / total_content_height as f32) * track_h as f32).round() as i32;
        let thumb_h = thumb_h.clamp(px(24), track_h);
        let scroll_ratio = if max_scroll > 0 { scroll_offset as f32 / max_scroll as f32 } else { 0.0 };
        let thumb_y = track_top + (scroll_ratio * (track_h - thumb_h) as f32).round() as i32;
        let thumb_rect = RECT {
            left: client.right - px(8),
            top: thumb_y,
            right: client.right - px(4),
            bottom: thumb_y + thumb_h,
        };
        super::candidates::view::fill_round_rect(hdc, thumb_rect, if dark { rgb(71, 85, 105) } else { rgb(148, 163, 184) }, 2);
    }

    // =========================================================================
    // 4. 底部输入 Dock：与学习面板共享蓝灰色、轻边框和统一留白规范
    // =========================================================================
    let dock_top = client.bottom - px(132);
    let dock_rect = RECT {
        left: 0,
        top: dock_top,
        right: client.right,
        bottom: client.bottom,
    };
    super::candidates::view::fill_rect(hdc, dock_rect, card_bg);

    let dock_border = RECT {
        left: 0,
        top: dock_top,
        right: client.right,
        bottom: dock_top + 1,
    };
    super::candidates::view::fill_rect(hdc, dock_border, card_border);

    // 统一输入容器卡片
    let in_box_rect = RECT {
        left: px(24),
        top: dock_top + px(12),
        right: client.right - px(24),
        bottom: client.bottom - px(12),
    };
    super::candidates::view::fill_round_rect(
        hdc,
        in_box_rect,
        if dark { rgb(15, 23, 42) } else { rgb(255, 255, 255) },
        8,
    );
    let in_border_rect = RECT {
        left: px(24),
        top: dock_top + px(12),
        right: client.right - px(24),
        bottom: dock_top + px(13),
    };
    super::candidates::view::fill_rect(hdc, in_border_rect, card_border);

    // 文本排版
    let input_y = dock_top + px(22);
    let mut cur_caret_x = px(28);
    let mut cur_caret_y = input_y;
    if data.input_text.is_empty() {
        draw_text_gdi(hdc, font_msg, muted_color, px(28), input_y, "向青简 AI 提问任何问题…… (Enter 发送，Shift+Enter 换行)");
    } else {
        let max_in_w = client.right - px(68);
        let in_lines = wrap_text_lines(hdc, font_msg, max_in_w, &data.input_text);
        let mut cur_y = input_y;
        let line_count = in_lines.len().max(1).min(3);
        for (i, line) in in_lines.iter().take(3).enumerate() {
            let lw = draw_text_gdi(hdc, font_msg, text_color, px(28), cur_y, line);
            if i == line_count - 1 {
                cur_caret_x = px(28) + lw;
                cur_caret_y = cur_y;
            }
            cur_y += px(20);
        }
    }

    // 绘制输入框内呼吸光标 (消除“光标焦点不在框里”的视觉缺失)
    let caret_rect = RECT {
        left: cur_caret_x + px(1),
        top: cur_caret_y + px(1),
        right: cur_caret_x + px(3),
        bottom: cur_caret_y + px(17),
    };
    super::candidates::view::fill_rect(hdc, caret_rect, brand_color);

    // 底部状态标识与发送/停止按钮 (在输入卡片内部右下角)
    let bottom_action_y = client.bottom - px(42);

    // 左侧极简模型标识
    draw_text_gdi(hdc, font_small, sub_color, px(28), bottom_action_y + px(4), "✦ Qwen3 14B · 极速高性价比专享推理");

    // 右侧现代圆角发送/停止 Action 按钮
    let send_x = client.right - px(48);
    let send_y = bottom_action_y;
    let send_rect = RECT {
        left: send_x,
        top: send_y,
        right: send_x + px(28),
        bottom: send_y + px(28),
    };

    if data.state == StreamingState::Streaming {
        // 生成中：停止按钮 (红色方形 ■)
        super::candidates::view::fill_round_rect(hdc, send_rect, rgb(220, 38, 38), 6);
        draw_text_gdi(hdc, font_brand, rgb(255, 255, 255), send_x + px(9), send_y + px(4), "■");
    } else {
        let has_text = !data.input_text.trim().is_empty();
        let btn_color = if has_text { brand_color } else { if dark { rgb(51, 65, 85) } else { rgb(203, 213, 225) } };
        let icon_color = if has_text { rgb(255, 255, 255) } else { if dark { rgb(100, 116, 139) } else { rgb(148, 163, 184) } };
        super::candidates::view::fill_round_rect(hdc, send_rect, btn_color, 6);
        draw_text_gdi(hdc, font_brand, icon_color, send_x + px(9), send_y + px(4), "↑");
    }

    unsafe {
        let _ = DeleteObject(font_brand.into());
        let _ = DeleteObject(font_header_title.into());
        let _ = DeleteObject(font_header_sub.into());
        let _ = DeleteObject(font_btn.into());
        let _ = DeleteObject(font_msg.into());
        let _ = DeleteObject(font_msg_bold.into());
        let _ = DeleteObject(font_code.into());
        let _ = DeleteObject(font_small.into());
        let _ = DeleteObject(font_small_bold.into());
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        wndproc_inner(hwnd, msg, wparam, lparam)
    }));
    match result {
        Ok(lr) => lr,
        Err(e) => {
            tracing::error!(?e, "AI Assistant wndproc panic caught; protected host UI thread");
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

unsafe fn wndproc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_IME_SETCONTEXT | WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION => {
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                win.update_caret_pos();
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETFOCUS => {
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                win.update_caret_pos();
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            unsafe {
                let _ = DestroyCaret();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = DestroyCaret();
            }
            AI_WINDOWS.with(|w| w.borrow_mut().remove(&(hwnd.0 as isize)));
            LRESULT(0)
        }
        WM_MOVE | WM_EXITSIZEMOVE => {
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                if !win.is_maximized.get() {
                    let mut rc = RECT::default();
                    if unsafe { GetWindowRect(hwnd, &mut rc) }.is_ok() {
                        let margin = layered::shadow_margin(unsafe { GetDpiForWindow(hwnd) }.max(96));
                        win.user_pos.set(Some((rc.left + margin, rc.top + margin)));
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16;
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                win.scroll_to_bottom_pending.set(false);
                let cur = win.scroll_y.get();
                let next = (cur - (delta as i32 / 3)).max(0);
                win.scroll_y.set(next);
                let _ = win.render_and_show(win.last_anchor.get());
            }
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                let mut d = win.data.borrow_mut();
                if ch == '\x08' {
                    d.input_text.pop();
                } else if ch == '\r' || ch == '\n' {
                    let shift = (unsafe { GetKeyState(0x10) } as i16) < 0;
                    if shift {
                        d.input_text.push('\n');
                    } else {
                        drop(d);
                        win.send_or_stop();
                        return LRESULT(0);
                    }
                } else if ch == '\x1b' {
                    drop(d);
                    win.close_and_notify();
                    return LRESULT(0);
                } else if !ch.is_control() {
                    d.input_text.push(ch);
                }
                drop(d);
                let _ = win.render_and_show(win.last_anchor.get());
                win.update_caret_pos();
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                let margin = if win.is_maximized.get() { 0 } else { layered::shadow_margin(dpi) };
                let scale = dpi as f32 / 96.0;
                let px = |val: i32| (val as f32 * scale).round() as i32;
                let cx = x - margin;
                let cy = y - margin;
                let w = win.current_width.get();
                let btn_w = px(28);
                let gap = px(4);
                let mr = px(12);
                let b5_x = w - mr - 5 * btn_w - 4 * gap;
                if cy >= 0 && cy <= px(44) && cx < b5_x {
                    win.toggle_maximize();
                    return LRESULT(0);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                let is_max = win.is_maximized.get();
                let margin = if is_max { 0 } else { layered::shadow_margin(dpi) };
                let scale = dpi as f32 / 96.0;
                let px = |val: i32| (val as f32 * scale).round() as i32;
                let cx = x - margin;
                let cy = y - margin;
                let w = win.current_width.get();
                let h = win.current_height.get();

                // 1. 顶部操作栏
                if cy >= 0 && cy <= px(44) {
                    let btn_w = px(28);
                    let gap = px(4);
                    let mr = px(12);

                    // 1.1 关闭 ✕
                    let b1_x = w - mr - btn_w;
                    if cx >= b1_x {
                        win.close_and_notify();
                        return LRESULT(0);
                    }

                    // 1.2 最大化/还原 🗖/⧉
                    let b2_x = w - mr - 2 * btn_w - gap;
                    if cx >= b2_x && cx < b1_x {
                        win.toggle_maximize();
                        return LRESULT(0);
                    }

                    // 1.3 最小化 —
                    let b3_x = w - mr - 3 * btn_w - 2 * gap;
                    if cx >= b3_x && cx < b2_x {
                        win.minimize();
                        return LRESULT(0);
                    }

                    // 1.4 置顶 📌
                    let b4_x = w - mr - 4 * btn_w - 3 * gap;
                    if cx >= b4_x && cx < b3_x {
                        win.toggle_pin();
                        return LRESULT(0);
                    }

                    // 1.5 新对话 ✎
                    let b5_x = w - mr - 5 * btn_w - 4 * gap;
                    if cx >= b5_x && cx < b4_x {
                        win.new_session();
                        return LRESULT(0);
                    }

                    // 1.6 标题栏其余空白区域 -> 原生平滑拖动
                    if !win.is_maximized.get() {
                        unsafe {
                            let _ = ReleaseCapture();
                            let _ = SendMessageW(
                                hwnd,
                                WM_NCLBUTTONDOWN,
                                Some(WPARAM(HTCAPTION as usize)),
                                Some(LPARAM(0)),
                            );
                        }
                    }
                    return LRESULT(0);
                }

                // 2. 底部控制区点击
                let dock_top = h - px(132);
                let bottom_action_y = h - px(42);

                // 发送/停止按钮点击
                let send_x = w - px(48);
                let send_y = bottom_action_y;
                if cx >= send_x && cx <= send_x + px(28) && cy >= send_y && cy <= send_y + px(28) {
                    win.send_or_stop();
                    return LRESULT(0);
                }

                // 输入卡片点击：聚焦并设置 Caret
                if cy >= dock_top {
                    let _ = unsafe { SetFocus(Some(hwnd)) };
                    win.update_caret_pos();
                    return LRESULT(0);
                }

                let _ = unsafe { SetFocus(Some(hwnd)) };
                win.update_caret_pos();
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
