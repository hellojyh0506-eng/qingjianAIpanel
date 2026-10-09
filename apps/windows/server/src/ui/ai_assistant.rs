//! Qingjian AI 对话窗口 (Qwen 2.5 7B 真实流式对话、快速/深度模式与按需联网)
//! 高对比度、防发白、视口隔离裁剪、自动贴底滚动、故障沙箱防护的原生 Windows 浮动面板。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetTextExtentPoint32W, HDC, HFONT, IntersectClipRect,
    RestoreDC, SaveDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowRect, IDC_ARROW, LoadCursorW,
    SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SetForegroundWindow, SetWindowPos, ShowWindow,
    IsIconic, WM_CHAR, WM_DESTROY, WM_EXITSIZEMOVE, WM_MOVE, WM_LBUTTONDOWN, WM_LBUTTONDBLCLK,
    WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_NCLBUTTONDOWN, WNDCLASSEXW, CS_DBLCLKS,
    WS_EX_LAYERED, WS_EX_APPWINDOW, WS_POPUP, HWND_TOPMOST, HWND_NOTOPMOST, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOACTIVATE, MA_NOACTIVATE, SendMessageW, HTCAPTION, HTCLIENT,
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
    Fast, // ⚡ 快速回答 (简洁、直接、极速)
    Deep, // 🧠 深度回答 (多维对比、底层原理、完整推演)
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
            messages: vec![
                ChatBubble {
                    is_user: true,
                    text: "帮我解释一下 “I've been swamped lately” 和 “I've been very busy lately” 的区别。".to_string(),
                    web_sources: Vec::new(),
                    ttft_ms: None,
                    total_ms: None,
                },
                ChatBubble {
                    is_user: false,
                    text: "“I've been swamped lately.”\n语气更为地道口语化，生动传达事务像潮水般涌来、忙得焦头烂额的真实状态。\n\n“I've been very busy lately.”\n表达严谨中性，适合大部分日常或职场工作汇报场合。\n\n在口语交流与写作中，根据亲疏关系与交谈语境选择最贴切的表达即可。".to_string(),
                    web_sources: Vec::new(),
                    ttft_ms: Some(180),
                    total_ms: Some(950),
                },
            ],
            input_text: String::new(),
            chat_mode: ChatMode::Fast,
            web_search_enabled: false,
            state: StreamingState::Idle,
            error_message: None,
            is_pinned: false,
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
                WS_EX_APPWINDOW | WS_EX_LAYERED,
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
            current_width: Cell::new(600),
            current_height: Cell::new(700),
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
            let _ = SetFocus(Some(self.hwnd));
            let _ = SetForegroundWindow(self.hwnd);
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

    /// 隐藏窗口（不主动回传通知，用于响应 Router 命令）
    pub fn hide(&self) {
        self.is_visible.set(false);
        self.last_anchor.set(None);
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    /// 用户主动关闭窗口并向 Router 报告，切断在途流式推理
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
            // 还原窗口
            if let Some((nx, ny, nw, nh)) = self.normal_bounds.get() {
                self.current_width.set(nw);
                self.current_height.set(nh);
                self.user_pos.set(Some((nx, ny)));
            } else {
                let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
                let scale = dpi as f32 / 96.0;
                self.current_width.set((600.0 * scale).round() as i32);
                self.current_height.set((700.0 * scale).round() as i32);
            }
            self.is_maximized.set(false);
        } else {
            // 最大化展开至当前所在屏幕工作区
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
                let fast = d.chat_mode == ChatMode::Fast;
                let web = d.web_search_enabled;
                let sid = d.session_id.clone();
                drop(d);
                self.scroll_to_bottom_pending.set(true);
                (self.action_cb)(CandidateAction::AiChatSend {
                    prompt,
                    fast_mode: fast,
                    web_search: web,
                    session_id: sid,
                });
            }
        }
        let _ = self.render_and_show(self.last_anchor.get());
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
            let logical_w = 600;
            let logical_h = 700;
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
            windows::Win32::Graphics::Gdi::FONT_CHARSET(1), // DEFAULT_CHARSET for full Unicode fallback
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(5), // CLEARTYPE_QUALITY
            0,
            w!("Microsoft YaHei UI"),
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

/// 优雅折行：按空格与 CJK 词边界换行，绝不在英文单词内部生硬折断
fn wrap_text_lines(hdc: HDC, font: HFONT, max_w: i32, text: &str) -> Vec<String> {
    unsafe { SelectObject(hdc, font.into()) };
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let p_trimmed = paragraph.trim_end();
        if p_trimmed.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut tokens: Vec<String> = Vec::new();
        let mut cur_word = String::new();

        for ch in p_trimmed.chars() {
            if ch.is_ascii_whitespace() {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(" ".to_string());
            } else if ch > '\u{2E80}' {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(ch.to_string());
            } else {
                cur_word.push(ch);
            }
        }
        if !cur_word.is_empty() {
            tokens.push(cur_word);
        }

        let mut current_line = String::new();
        for token in tokens {
            let mut test_line = current_line.clone();
            test_line.push_str(&token);
            let utf16: Vec<u16> = test_line.encode_utf16().collect();
            let mut size = SIZE::default();
            unsafe {
                let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
            }

            if size.cx > max_w && !current_line.is_empty() {
                lines.push(current_line.trim_end().to_string());
                current_line = if token == " " { String::new() } else { token };
            } else {
                current_line = test_line;
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line.trim_end().to_string());
        }
    }
    lines
}

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

    // 配色体系：拒绝浮夸 AI 味与发白泛虚，采用符合耐看型工业美学的 Slate + Deep Teal 高对比度体系
    let header_bg = if dark { rgb(30, 41, 59) } else { rgb(248, 250, 252) };
    let header_border = if dark { rgb(51, 65, 85) } else { rgb(226, 232, 240) };
    let text_color = if dark { rgb(241, 245, 249) } else { rgb(15, 23, 42) }; // Slate 900
    let sub_color = if dark { rgb(148, 163, 184) } else { rgb(71, 85, 105) }; // Slate 600
    let muted_color = if dark { rgb(100, 116, 139) } else { rgb(148, 163, 184) };
    let brand_color = rgb(15, 118, 110); // 深青 #0F766E
    let brand_light = if dark { rgb(19, 78, 74) } else { rgb(204, 251, 241) }; // #CCFBF1
    let brand_border = rgb(20, 184, 166);

    let card_bg = if dark { rgb(30, 41, 59) } else { rgb(248, 250, 252) };
    let card_border = if dark { rgb(51, 65, 85) } else { rgb(203, 213, 225) }; // Slate 300
    let user_bubble_bg = if dark { rgb(30, 58, 138) } else { rgb(241, 245, 249) }; // Slate 100
    let user_bubble_border = if dark { rgb(59, 130, 246) } else { rgb(203, 213, 225) };
    let user_text = if dark { rgb(241, 245, 249) } else { rgb(15, 23, 42) };

    let font_brand = create_font(px(14), FW_BOLD, false);
    let font_model = create_font(px(11), FW_NORMAL, false);
    let font_btn = create_font(px(12), FW_NORMAL, false);
    let font_msg = create_font(px(13), FW_NORMAL, false);
    let font_msg_bold = create_font(px(13), FW_BOLD, false);
    let font_small = create_font(px(11), FW_NORMAL, false);
    let font_small_bold = create_font(px(11), FW_BOLD, false);

    // ==========================================
    // 1. 顶部标题栏 (高度 px(44)，实体底色与边框)
    // ==========================================
    let header_rect = RECT { left: 0, top: 0, right: client.right, bottom: px(44) };
    super::candidates::view::fill_rect(hdc, header_rect, header_bg);
    let sep_top = RECT { left: 0, top: px(44), right: client.right, bottom: px(45) };
    super::candidates::view::fill_rect(hdc, sep_top, header_border);

    let mut hx = px(16);
    draw_text_gdi(hdc, font_brand, brand_color, hx, px(12), "✦");
    hx += px(16);
    draw_text_gdi(hdc, font_brand, text_color, hx, px(12), "Qingjian AI");
    hx += px(96);

    // 模型 Badge
    let badge_rect = RECT { left: hx, top: px(11), right: hx + px(90), bottom: px(33) };
    super::candidates::view::fill_round_rect(hdc, badge_rect, if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) }, 4);
    let badge_border = RECT { left: hx, top: px(11), right: hx + px(90), bottom: px(12) };
    super::candidates::view::fill_rect(hdc, badge_border, card_border);
    draw_text_gdi(hdc, font_model, sub_color, hx + px(8), px(14), "Qwen 2.5 · 7B");

    // 右侧 5 键操作群：新对话 ✎ | 置顶 📌 | 最小化 — | 最大化/还原 🗖/⧉ | 关闭 ✕
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
    let view_top = px(48);
    let view_bottom = client.bottom - px(148);
    let view_h = (view_bottom - view_top).max(100);

    let mut total_content_height = 0;
    for msg in &data.messages {
        if msg.is_user {
            let max_w = (client.right * 7 / 10).min(px(420));
            let lines = wrap_text_lines(hdc, font_msg, max_w - px(24), &msg.text);
            let text_h = (lines.len() as i32 * px(20)).max(px(20));
            let bubble_h = text_h + px(16);
            total_content_height += bubble_h + px(16);
        } else {
            let max_w = client.right - px(48);
            let lines = wrap_text_lines(hdc, font_msg, max_w, &msg.text);
            let text_h = (lines.len() as i32 * px(22)).max(px(22));
            let mut item_h = px(24) + text_h;
            if !msg.web_sources.is_empty() {
                item_h += px(20) + (msg.web_sources.len().min(3) as i32) * px(18);
            }
            if msg.ttft_ms.is_some() || msg.total_ms.is_some() {
                item_h += px(20);
            }
            total_content_height += item_h + px(18);
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
            let max_w = (client.right * 7 / 10).min(px(420));
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
                "✦ QINGJIAN AI · 正在思考回答……"
            } else {
                "✦ QINGJIAN AI"
            };
            if my >= view_top - px(24) && my <= view_bottom {
                draw_text_gdi(hdc, font_small_bold, brand_color, px(20), my, label);
            }
            my += px(24);

            let max_w = client.right - px(48);
            let lines = wrap_text_lines(hdc, font_msg, max_w, &msg.text);
            for line in &lines {
                if my >= view_top - px(22) && my <= view_bottom {
                    if line.starts_with("### ") || line.starts_with("## ") {
                        let h_txt = line.trim_start_matches('#').trim();
                        draw_text_gdi(hdc, font_msg_bold, text_color, px(20), my, h_txt);
                    } else if line.starts_with("- ") || line.starts_with("* ") {
                        let bullet_txt = format!("• {}", &line[2..]);
                        draw_text_gdi(hdc, font_msg, text_color, px(28), my, &bullet_txt);
                    } else {
                        draw_text_gdi(hdc, font_msg, text_color, px(20), my, line);
                    }
                }
                my += px(22);
            }
            my += px(8);

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

    // ==========================================
    // 4. 底部输入与控制卡片 (高度 px(148)，全宽遮蔽正文，防穿透防重叠)
    // ==========================================
    let dock_top = client.bottom - px(148);
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

    // 输入框卡片
    let in_box_rect = RECT {
        left: px(16),
        top: dock_top + px(10),
        right: client.right - px(16),
        bottom: dock_top + px(92),
    };
    super::candidates::view::fill_round_rect(hdc, in_box_rect, if dark { rgb(15, 23, 42) } else { rgb(255, 255, 255) }, 6);
    let in_border_rect = RECT {
        left: px(16),
        top: dock_top + px(10),
        right: client.right - px(16),
        bottom: dock_top + px(11),
    };
    super::candidates::view::fill_rect(hdc, in_border_rect, card_border);

    let input_y = dock_top + px(18);
    if data.input_text.is_empty() {
        draw_text_gdi(hdc, font_msg, muted_color, px(28), input_y, "输入消息……（Enter 发送，Shift+Enter 换行）");
    } else {
        let max_in_w = client.right - px(64);
        let cursor_txt = format!("{}|", data.input_text);
        let in_lines = wrap_text_lines(hdc, font_msg, max_in_w, &cursor_txt);
        let mut cur_y = input_y;
        for line in in_lines.iter().take(3) {
            draw_text_gdi(hdc, font_msg, text_color, px(28), cur_y, line);
            cur_y += px(20);
        }
    }

    // 底部控制胶囊条
    let bar_y = client.bottom - px(44);
    let mut cx = px(16);

    // [+] 扩展
    let r_plus = RECT { left: cx, top: bar_y, right: cx + px(28), bottom: bar_y + px(28) };
    super::candidates::view::fill_round_rect(hdc, r_plus, if dark { rgb(51, 65, 85) } else { rgb(255, 255, 255) }, 4);
    let r_plus_b = RECT { left: cx, top: bar_y, right: cx + px(28), bottom: bar_y + 1 };
    super::candidates::view::fill_rect(hdc, r_plus_b, card_border);
    draw_text_gdi(hdc, font_btn, sub_color, cx + px(9), bar_y + px(5), "+");
    cx += px(36);

    // [⚡ 快速回答]
    let is_fast = data.chat_mode == ChatMode::Fast;
    let fast_w = px(92);
    let r_fast = RECT { left: cx, top: bar_y, right: cx + fast_w, bottom: bar_y + px(28) };
    let fast_bg = if is_fast { brand_light } else { if dark { rgb(51, 65, 85) } else { rgb(255, 255, 255) } };
    super::candidates::view::fill_round_rect(hdc, r_fast, fast_bg, 4);
    let r_fast_b = RECT { left: cx, top: bar_y, right: cx + fast_w, bottom: bar_y + 1 };
    super::candidates::view::fill_rect(hdc, r_fast_b, if is_fast { brand_border } else { card_border });
    draw_text_gdi(hdc, if is_fast { font_small_bold } else { font_btn }, if is_fast { brand_color } else { sub_color }, cx + px(10), bar_y + px(5), "⚡ 快速回答");
    cx += fast_w + px(8);

    // [🧠 深度回答]
    let is_deep = data.chat_mode == ChatMode::Deep;
    let deep_w = px(92);
    let r_deep = RECT { left: cx, top: bar_y, right: cx + deep_w, bottom: bar_y + px(28) };
    let deep_bg = if is_deep { brand_light } else { if dark { rgb(51, 65, 85) } else { rgb(255, 255, 255) } };
    super::candidates::view::fill_round_rect(hdc, r_deep, deep_bg, 4);
    let r_deep_b = RECT { left: cx, top: bar_y, right: cx + deep_w, bottom: bar_y + 1 };
    super::candidates::view::fill_rect(hdc, r_deep_b, if is_deep { brand_border } else { card_border });
    draw_text_gdi(hdc, if is_deep { font_small_bold } else { font_btn }, if is_deep { brand_color } else { sub_color }, cx + px(10), bar_y + px(5), "🧠 深度回答");
    cx += deep_w + px(8);

    // [🌐 联网搜索]
    let is_web = data.web_search_enabled;
    let web_w = px(92);
    let r_web = RECT { left: cx, top: bar_y, right: cx + web_w, bottom: bar_y + px(28) };
    let web_bg = if is_web { rgb(220, 252, 231) } else { if dark { rgb(51, 65, 85) } else { rgb(255, 255, 255) } };
    super::candidates::view::fill_round_rect(hdc, r_web, web_bg, 4);
    let r_web_b = RECT { left: cx, top: bar_y, right: cx + web_w, bottom: bar_y + 1 };
    super::candidates::view::fill_rect(hdc, r_web_b, if is_web { rgb(34, 197, 94) } else { card_border });
    draw_text_gdi(hdc, if is_web { font_small_bold } else { font_btn }, if is_web { rgb(21, 128, 61) } else { sub_color }, cx + px(10), bar_y + px(5), "🌐 联网搜索");

    // 右侧发送 / 停止按钮
    let send_x = client.right - px(48);
    let send_rect = RECT { left: send_x, top: bar_y, right: send_x + px(32), bottom: bar_y + px(28) };

    if data.state == StreamingState::Streaming {
        // 生成中：停止按钮 (红色方形 ■)
        super::candidates::view::fill_round_rect(hdc, send_rect, rgb(220, 38, 38), 4);
        draw_text_gdi(hdc, font_brand, rgb(255, 255, 255), send_x + px(10), bar_y + px(5), "■");
    } else {
        // 就绪态：发送按钮 (深青箭头 ↑)
        super::candidates::view::fill_round_rect(hdc, send_rect, brand_color, 4);
        draw_text_gdi(hdc, font_brand, rgb(255, 255, 255), send_x + px(10), bar_y + px(5), "↑");
    }

    unsafe {
        let _ = DeleteObject(font_brand.into());
        let _ = DeleteObject(font_model.into());
        let _ = DeleteObject(font_btn.into());
        let _ = DeleteObject(font_msg.into());
        let _ = DeleteObject(font_msg_bold.into());
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
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_DESTROY => {
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
                    let shift = (unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) } as i16) < 0;
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

                // 1. 顶部操作栏 (cy in 0..px(44))
                if cy >= 0 && cy <= px(44) {
                    let btn_w = px(28);
                    let gap = px(4);
                    let mr = px(12);

                    // 1.1 关闭 ✕
                    let b1_x = w - mr - btn_w;
                    if cx >= b1_x && cx <= w - mr {
                        win.close_and_notify();
                        return LRESULT(0);
                    }

                    // 1.2 最大化/还原 🗖/⧉
                    let b2_x = w - mr - 2 * btn_w - gap;
                    if cx >= b2_x && cx < b1_x {
                        win.toggle_maximize();
                        return LRESULT(0);
                    }

                    // 1.3 最小化 — (真正最小化至任务栏，绝不强退会话)
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

                // 2. 底部控制条点击 (cy in h - px(52) .. h - px(10))
                let bar_y = h - px(44);
                if cy >= bar_y - px(4) && cy <= bar_y + px(32) {
                    // 2.1 发送/停止按钮点击
                    let send_x = w - px(48);
                    if cx >= send_x - px(4) && cx <= send_x + px(36) {
                        win.send_or_stop();
                        return LRESULT(0);
                    }

                    // 2.2 切换快速回答
                    let fast_x = px(16) + px(36);
                    let fast_w = px(92);
                    if cx >= fast_x && cx <= fast_x + fast_w {
                        win.data.borrow_mut().chat_mode = ChatMode::Fast;
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }

                    // 2.3 切换深度回答
                    let deep_x = fast_x + fast_w + px(8);
                    let deep_w = px(92);
                    if cx >= deep_x && cx <= deep_x + deep_w {
                        win.data.borrow_mut().chat_mode = ChatMode::Deep;
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }

                    // 2.4 切换联网搜索
                    let web_x = deep_x + deep_w + px(8);
                    let web_w = px(92);
                    if cx >= web_x && cx <= web_x + web_w {
                        let mut d = win.data.borrow_mut();
                        d.web_search_enabled = !d.web_search_enabled;
                        drop(d);
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }
                }

                // 聚焦到窗口与输入区
                let _ = unsafe { SetFocus(Some(hwnd)) };
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
