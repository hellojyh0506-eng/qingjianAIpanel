//! Qingjian AI 对话窗口 (Qwen 7B 真实流式对话、快速/深度模式与按需联网)
//! 紧凑、现代、高级原生 Windows 浮动面板，支持拖动、拉伸、滚轮、剪贴板、多轮对话与即时停止。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetTextExtentPoint32W, HDC, HFONT, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW, FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
    HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, LoadCursorW,
    SW_HIDE, SW_SHOW, SetWindowPos, ShowWindow, WM_CHAR,
    WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_NCHITTEST, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_THICKFRAME, HWND_TOPMOST,
    HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE, MA_NOACTIVATE,
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
                    text: "I've been swamped lately.\n语气更口语，强调最近忙得不可开交、事情压得透不过气。\n\nI've been very busy lately.\n表达更中性，适合日常交谈，也适合比较正式的工作场合。\n\n如果用于雅思口语，二者均可；关键是结合具体语境展开，而不是刻意使用冷僻词汇。".to_string(),
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
    dark: Cell<bool>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl AiAssistantWindow {
    pub fn new(action_cb: Box<dyn Fn(CandidateAction)>) -> Result<Rc<Self>> {
        CLASS.ensure(|| WNDCLASSEXW {
            lpfnWndProc: Some(wndproc),
            hInstance: super::module_handle(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                CLASS_NAME,
                w!("Qingjian AI"),
                WS_POPUP | WS_THICKFRAME,
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
            dark: Cell::new(false),
            action_cb,
        });
        AI_WINDOWS.with(|w| w.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&instance)));
        Ok(instance)
    }

    /// 打开或聚焦 AI 助手窗口
    pub fn show_or_focus(&self, anchor: Option<RECT>, initial_prompt: Option<String>) {
        if let Some(p) = initial_prompt {
            if !p.is_empty() {
                let mut d = self.data.borrow_mut();
                d.input_text = p;
            }
        }
        let _ = self.render_and_show(anchor);
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
    }

    /// 追加流式文本增量
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
        let _ = self.render_and_show(self.last_anchor.get());
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
        let _ = self.render_and_show(self.last_anchor.get());
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
        let _ = self.render_and_show(self.last_anchor.get());
    }

    pub fn is_visible(&self) -> bool {
        self.last_anchor.get().is_some()
    }

    pub fn hide(&self) {
        self.last_anchor.set(None);
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    pub fn render_and_show(&self, anchor: Option<RECT>) -> Result<()> {
        if let Some(a) = anchor {
            self.last_anchor.set(Some(a));
        }
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let dpi = if dpi == 0 { 96 } else { dpi };
        let scale = dpi as f32 / 96.0;
        let margin = layered::shadow_margin(dpi);

        let logical_w = 600;
        let logical_h = 700;
        let content_w = (logical_w as f32 * scale).round() as i32;
        let content_h = (logical_h as f32 * scale).round() as i32;
        self.current_width.set(content_w);
        self.current_height.set(content_h);

        let content = (content_w, content_h);

        let (pos_x, pos_y) = if let Some(up) = self.user_pos.get() {
            clamp_position_to_monitor(up, content, margin)
        } else if let Some(a) = self.last_anchor.get() {
            calculate_ai_position(a, content, margin)
        } else {
            let work = monitor::work_area_near(windows::Win32::Foundation::POINT { x: 500, y: 300 });
            let cx = (work.right - content_w - margin - 40).max(work.left + margin);
            let cy = (work.bottom - content_h - margin - 60).max(work.top + margin);
            (cx, cy)
        };

        let data = self.data.borrow();
        let bg_color = if self.dark.get() {
            COLORREF(0x001A1A1A)
        } else {
            COLORREF(0x00FFFFFF)
        };

        layered::composite(
            self.hwnd,
            &Layered {
                content,
                margin,
                win_pos: (pos_x - margin, pos_y - margin),
                win_size: (content.0 + margin * 2, content.1 + margin * 2),
                background: bg_color,
                corner_radius: 14,
                paint: &|hdc, client| {
                    paint_ai_assistant(hdc, &data, client, self.dark.get(), scale, self.scroll_y.get());
                },
            },
        )?;

        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOW) };
        Ok(())
    }
}

thread_local! {
    static AI_WINDOWS: RefCell<std::collections::HashMap<isize, Rc<AiAssistantWindow>>> = RefCell::new(std::collections::HashMap::new());
}

fn clamp_position_to_monitor(pos: (i32, i32), content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(windows::Win32::Foundation::POINT {
        x: pos.0,
        y: pos.1,
    });
    let x = pos.0.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
    let y = pos.1.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
    (x, y)
}

fn calculate_ai_position(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(windows::Win32::Foundation::POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let right_x = anchor.right + 16;
    if right_x + content.0 + margin <= work.right {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (right_x, y);
    }
    // 居中偏右
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
            windows::Win32::Graphics::Gdi::FONT_CHARSET(0),
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(0),
            0,
            w!("Segoe UI"),
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

/// 自动折行绘制多行文本并返回总占用高度
fn draw_wrapped_text_gdi(
    hdc: HDC,
    font: HFONT,
    color: COLORREF,
    x: i32,
    mut y: i32,
    max_w: i32,
    line_h: i32,
    text: &str,
) -> i32 {
    unsafe { SelectObject(hdc, font.into()) };
    let start_y = y;
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            y += line_h / 2;
            continue;
        }
        let mut cur_line = String::new();
        for ch in paragraph.chars() {
            let mut test_line = cur_line.clone();
            test_line.push(ch);
            let utf16: Vec<u16> = test_line.encode_utf16().collect();
            let mut size = SIZE::default();
            unsafe {
                let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
            }
            if size.cx > max_w && !cur_line.is_empty() {
                draw_text_gdi(hdc, font, color, x, y, &cur_line);
                y += line_h;
                cur_line.clear();
            }
            cur_line.push(ch);
        }
        if !cur_line.is_empty() {
            draw_text_gdi(hdc, font, color, x, y, &cur_line);
            y += line_h;
        }
    }
    y - start_y
}

fn paint_ai_assistant(
    hdc: HDC,
    data: &AiAssistantData,
    client: RECT,
    dark: bool,
    scale: f32,
    scroll_offset: i32,
) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };
    let px = |val: i32| (val as f32 * scale).round() as i32;

    let text_color = if dark { rgb(226, 232, 240) } else { rgb(15, 23, 42) };
    let sub_color = if dark { rgb(148, 163, 184) } else { rgb(100, 116, 139) };
    let brand_color = rgb(0, 122, 135); // 蓝绿主色
    let user_bubble_bg = rgb(232, 240, 254); // 浅蓝背景 #E8F0FE
    let card_bg = if dark { rgb(30, 41, 59) } else { rgb(248, 250, 252) };
    let border_color = if dark { rgb(51, 65, 85) } else { rgb(226, 232, 240) };

    let font_brand = create_font(px(15), FW_BOLD, false);
    let font_model = create_font(px(12), FW_NORMAL, false);
    let font_btn = create_font(px(12), FW_NORMAL, false);
    let font_msg = create_font(px(13), FW_NORMAL, false);
    let font_small = create_font(px(11), FW_NORMAL, false);

    // ==========================================
    // 1. 顶部标题栏 (高度 px(46))
    // ==========================================
    let mut hx = px(18);
    draw_text_gdi(hdc, font_brand, brand_color, hx, px(12), "✦");
    hx += px(18);
    draw_text_gdi(hdc, font_brand, text_color, hx, px(12), "Qingjian AI");
    hx += px(96);
    draw_text_gdi(hdc, font_model, sub_color, hx, px(14), "Qwen 2.5 · 7B");

    // 右侧操作群
    let rx = client.right - px(130);
    draw_text_gdi(hdc, font_btn, sub_color, rx, px(13), "✍"); // 新对话
    draw_text_gdi(hdc, font_btn, if data.is_pinned { brand_color } else { sub_color }, rx + px(24), px(13), "📌"); // 置顶
    draw_text_gdi(hdc, font_btn, sub_color, rx + px(48), px(13), "—"); // 最小化
    draw_text_gdi(hdc, font_btn, sub_color, rx + px(72), px(13), "▢"); // 最大化
    draw_text_gdi(hdc, font_btn, sub_color, rx + px(96), px(13), "×"); // 关闭

    let sep_top = RECT { left: px(16), top: px(44), right: client.right - px(16), bottom: px(45) };
    super::candidates::view::fill_rect(hdc, sep_top, border_color);

    // ==========================================
    // 2. 中间消息滚动视口 (y: px(50) ~ client.bottom - px(155))
    // ==========================================
    let view_top = px(50);
    let view_bottom = client.bottom - px(155);
    let mut my = view_top - scroll_offset;

    for msg in &data.messages {
        if msg.is_user {
            // 用户消息：右对齐浅蓝胶囊卡片
            let max_w = px(380);
            let text_h = draw_wrapped_text_gdi(hdc, font_msg, text_color, 0, 0, max_w, px(20), &msg.text);
            let bubble_w = max_w.min(px(400)) + px(24);
            let bubble_h = text_h.max(px(32)) + px(16);
            let bx = client.right - px(20) - bubble_w;

            if my + bubble_h >= view_top && my <= view_bottom {
                let r = RECT { left: bx, top: my, right: bx + bubble_w, bottom: my + bubble_h };
                super::candidates::view::fill_round_rect(hdc, r, user_bubble_bg, 8);
                draw_wrapped_text_gdi(hdc, font_msg, rgb(26, 36, 64), bx + px(12), my + px(8), max_w, px(20), &msg.text);
            }
            my += bubble_h + px(18);
        } else {
            // AI 消息：左对齐
            let label = if data.state == StreamingState::Streaming {
                "QINGJIAN AI · 正在回答……"
            } else {
                "QINGJIAN AI"
            };
            if my >= view_top && my <= view_bottom {
                draw_text_gdi(hdc, font_small, sub_color, px(20), my, label);
            }
            my += px(20);

            let max_w = client.right - px(50);
            let text_h = draw_wrapped_text_gdi(hdc, font_msg, text_color, px(20), my, max_w, px(22), &msg.text);
            my += text_h + px(12);

            // 若有真实来源列表，渲染来源药丸卡片
            if !msg.web_sources.is_empty() {
                if my >= view_top && my <= view_bottom {
                    draw_text_gdi(hdc, font_small, brand_color, px(20), my, "🌐 参考来源 (真实网络检索)：");
                }
                my += px(18);
                for (s_idx, src) in msg.web_sources.iter().take(3).enumerate() {
                    let s_txt = format!("{}. {} ({})", s_idx + 1, src.title, src.url);
                    if my >= view_top && my <= view_bottom {
                        draw_text_gdi(hdc, font_small, sub_color, px(28), my, &s_txt);
                    }
                    my += px(18);
                }
            }

            // 指标信息 (TTFT & Total)
            if let (Some(ttft), Some(tot)) = (msg.ttft_ms, msg.total_ms) {
                let stat_txt = format!("⚡ 首字延迟: {}ms · 总耗时: {}ms", ttft, tot);
                if my >= view_top && my <= view_bottom {
                    draw_text_gdi(hdc, font_small, sub_color, px(20), my, &stat_txt);
                }
                my += px(18);
            }
            my += px(14);
        }
    }

    // ==========================================
    // 3. 底部输入与控制卡片 (高度 px(140))
    // ==========================================
    let card_top = client.bottom - px(145);
    let card_rect = RECT {
        left: px(16),
        top: card_top,
        right: client.right - px(16),
        bottom: client.bottom - px(14),
    };
    super::candidates::view::fill_round_rect(hdc, card_rect, card_bg, 10);
    // 细边框
    let border_r = RECT {
        left: px(16),
        top: card_top,
        right: client.right - px(16),
        bottom: card_top + 1,
    };
    super::candidates::view::fill_rect(hdc, border_r, border_color);

    // 输入区文字
    let input_y = card_top + px(14);
    if data.input_text.is_empty() {
        draw_text_gdi(hdc, font_msg, sub_color, px(30), input_y, "输入消息……（Enter 发送，Shift+Enter 换行）");
    } else {
        let max_in_w = client.right - px(90);
        let cursor_txt = format!("{}|", data.input_text);
        draw_wrapped_text_gdi(hdc, font_msg, text_color, px(30), input_y, max_in_w, px(20), &cursor_txt);
    }

    // 底部控制胶囊条
    let bar_y = client.bottom - px(52);
    let mut cx = px(30);

    // [+] 扩展
    let r_plus = RECT { left: cx, top: bar_y, right: cx + px(28), bottom: bar_y + px(26) };
    super::candidates::view::fill_round_rect(hdc, r_plus, if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) }, 4);
    draw_text_gdi(hdc, font_btn, sub_color, cx + px(9), bar_y + px(5), "+");
    cx += px(36);

    // [⚡ 快速回答 ∨]
    let is_fast = data.chat_mode == ChatMode::Fast;
    let fast_w = px(88);
    let r_fast = RECT { left: cx, top: bar_y, right: cx + fast_w, bottom: bar_y + px(26) };
    let fast_bg = if is_fast { user_bubble_bg } else { if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) } };
    super::candidates::view::fill_round_rect(hdc, r_fast, fast_bg, 4);
    draw_text_gdi(hdc, font_btn, if is_fast { brand_color } else { sub_color }, cx + px(10), bar_y + px(5), "⚡ 快速回答");
    cx += fast_w + px(8);

    // [🧠 深度回答]
    let is_deep = data.chat_mode == ChatMode::Deep;
    let deep_w = px(88);
    let r_deep = RECT { left: cx, top: bar_y, right: cx + deep_w, bottom: bar_y + px(26) };
    let deep_bg = if is_deep { user_bubble_bg } else { if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) } };
    super::candidates::view::fill_round_rect(hdc, r_deep, deep_bg, 4);
    draw_text_gdi(hdc, font_btn, if is_deep { brand_color } else { sub_color }, cx + px(10), bar_y + px(5), "🧠 深度回答");
    cx += deep_w + px(8);

    // [🌐 联网搜索]
    let is_web = data.web_search_enabled;
    let web_w = px(88);
    let r_web = RECT { left: cx, top: bar_y, right: cx + web_w, bottom: bar_y + px(26) };
    let web_bg = if is_web { rgb(220, 252, 231) } else { if dark { rgb(51, 65, 85) } else { rgb(241, 245, 249) } };
    super::candidates::view::fill_round_rect(hdc, r_web, web_bg, 4);
    draw_text_gdi(hdc, font_btn, if is_web { rgb(21, 128, 61) } else { sub_color }, cx + px(10), bar_y + px(5), "🌐 联网搜索");

    // 右侧发送 / 停止按钮 (圆角胶囊/方形切换)
    let send_x = client.right - px(58);
    let send_rect = RECT { left: send_x, top: bar_y - px(2), right: send_x + px(32), bottom: bar_y + px(30) };

    if data.state == StreamingState::Streaming {
        // 生成中：停止按钮 (红色/橙色方块 ■)
        super::candidates::view::fill_round_rect(hdc, send_rect, rgb(220, 38, 38), 6);
        draw_text_gdi(hdc, font_brand, rgb(255, 255, 255), send_x + px(10), bar_y + px(4), "■");
    } else {
        // 就绪态：发送按钮 (品牌蓝/青色箭头 ↑)
        super::candidates::view::fill_round_rect(hdc, send_rect, brand_color, 6);
        draw_text_gdi(hdc, font_brand, rgb(255, 255, 255), send_x + px(10), bar_y + px(4), "↑");
    }

    unsafe {
        let _ = DeleteObject(font_brand.into());
        let _ = DeleteObject(font_model.into());
        let _ = DeleteObject(font_btn.into());
        let _ = DeleteObject(font_msg.into());
        let _ = DeleteObject(font_small.into());
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(_w) = win {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let mut rect = RECT::default();
                let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
                let border = 6;
                // 边角拉伸支持
                if x < rect.left + border && y < rect.top + border { return LRESULT(HTTOPLEFT as isize); }
                if x > rect.right - border && y < rect.top + border { return LRESULT(HTTOPRIGHT as isize); }
                if x < rect.left + border && y > rect.bottom - border { return LRESULT(HTBOTTOMLEFT as isize); }
                if x > rect.right - border && y > rect.bottom - border { return LRESULT(HTBOTTOMRIGHT as isize); }
                if x < rect.left + border { return LRESULT(HTLEFT as isize); }
                if x > rect.right - border { return LRESULT(HTRIGHT as isize); }
                if y < rect.top + border { return LRESULT(HTTOP as isize); }
                if y > rect.bottom - border { return LRESULT(HTBOTTOM as isize); }
                if y < rect.top + 46 {
                    if x > rect.right - 140 {
                        return LRESULT(HTCLIENT as isize);
                    }
                    return LRESULT(HTCAPTION as isize);
                }
            }
            LRESULT(HTCLIENT as isize)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16;
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                let cur = win.scroll_y.get();
                let next = (cur - (delta as i32 / 4)).max(0);
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
                    // Backspace
                    d.input_text.pop();
                } else if ch == '\r' || ch == '\n' {
                    // 检查 Shift 是否按下
                    let shift = (unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) } as i16) < 0;
                    if shift {
                        d.input_text.push('\n');
                    } else {
                        // 发送
                        let prompt = d.input_text.trim().to_string();
                        if !prompt.is_empty() && d.state == StreamingState::Idle {
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
                            (win.action_cb)(CandidateAction::AiChatSend {
                                prompt,
                                fast_mode: fast,
                                web_search: web,
                                session_id: sid,
                            });
                        }
                        return LRESULT(0);
                    }
                } else if ch == '\x1b' {
                    // Esc -> 关闭
                    drop(d);
                    win.hide();
                    return LRESULT(0);
                } else if !ch.is_control() {
                    d.input_text.push(ch);
                }
                drop(d);
                let _ = win.render_and_show(win.last_anchor.get());
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let win = AI_WINDOWS.with(|w| w.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = win {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                let margin = layered::shadow_margin(dpi);
                let scale = dpi as f32 / 96.0;
                let px = |val: i32| (val as f32 * scale).round() as i32;
                let cx = x - margin;
                let cy = y - margin;
                let w = win.current_width.get();
                let h = win.current_height.get();

                // 1. 顶部操作栏
                if cy <= px(44) {
                    let rx = w - px(130);
                    if cx >= rx + px(90) && cx <= w {
                        // 关闭
                        win.hide();
                        return LRESULT(0);
                    } else if cx >= rx && cx <= rx + px(22) {
                        // 新对话
                        let mut d = win.data.borrow_mut();
                        d.session_id = uuid::Uuid::new_v4().to_string();
                        d.messages.clear();
                        d.input_text.clear();
                        d.state = StreamingState::Idle;
                        drop(d);
                        (win.action_cb)(CandidateAction::AiChatNewSession);
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    } else if cx >= rx + px(22) && cx <= rx + px(44) {
                        // 置顶切换 (Pin)
                        let mut d = win.data.borrow_mut();
                        d.is_pinned = !d.is_pinned;
                        let pinned = d.is_pinned;
                        drop(d);
                        unsafe {
                            let _ = SetWindowPos(
                                hwnd,
                                if pinned { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) },
                                0, 0, 0, 0,
                                SWP_NOMOVE | SWP_NOSIZE,
                            );
                        }
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }
                }

                // 2. 底部控制条点击
                let bar_y = h - px(52);
                if cy >= bar_y && cy <= bar_y + px(30) {
                    // 发送/停止按钮点击
                    let send_x = w - px(58);
                    if cx >= send_x && cx <= send_x + px(36) {
                        let mut d = win.data.borrow_mut();
                        if d.state == StreamingState::Streaming {
                            // 停止生成！
                            d.state = StreamingState::Idle;
                            let sid = d.session_id.clone();
                            drop(d);
                            (win.action_cb)(CandidateAction::AiChatStop { session_id: sid });
                        } else {
                            // 发送消息！
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
                                (win.action_cb)(CandidateAction::AiChatSend {
                                    prompt,
                                    fast_mode: fast,
                                    web_search: web,
                                    session_id: sid,
                                });
                            }
                        }
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }

                    // 切换快速回答
                    let fast_x = px(66);
                    let fast_w = px(88);
                    if cx >= fast_x && cx <= fast_x + fast_w {
                        win.data.borrow_mut().chat_mode = ChatMode::Fast;
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }

                    // 切换深度回答
                    let deep_x = fast_x + fast_w + px(8);
                    let deep_w = px(88);
                    if cx >= deep_x && cx <= deep_x + deep_w {
                        win.data.borrow_mut().chat_mode = ChatMode::Deep;
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }

                    // 切换联网搜索
                    let web_x = deep_x + deep_w + px(8);
                    let web_w = px(88);
                    if cx >= web_x && cx <= web_x + web_w {
                        let mut d = win.data.borrow_mut();
                        d.web_search_enabled = !d.web_search_enabled;
                        drop(d);
                        let _ = win.render_and_show(win.last_anchor.get());
                        return LRESULT(0);
                    }
                }

                // 聚焦到输入框
                let _ = unsafe { SetFocus(Some(hwnd)) };
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
