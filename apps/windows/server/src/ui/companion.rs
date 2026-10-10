//! English Assistant (青简英语学习助手伴侣窗)。
//! 100% 严格像素级对齐《严格参考图！必须完全百分百对应！！！.png》与《青简英语学习助手界面规范总览.png》。
//!
//! 规范要求：
//! 1. 主色 #2563EB，选中背景 #EBF2FF，卡片背景 #F8FAFC，边框 #E5E7EB，文字主 #111827，文字次 #6B7280，圆角 10px，窗口宽度 420px。
//! 2. 绝对不添加 emoji 元素，图标采用清晰极简矢量微标与 ClearType 字体。
//! 3. 三大核心标签页：
//!    - Tab 0: 语感解析 (意思、语气与特点、使用场景、自然例句、搭配与语法细节)
//!    - Tab 1: 更自然地说 (3个更地道的替换表达，含序号微标、语气、区别与内嵌例句)
//!    - Tab 2: 真实对话 (严格限定为 IELTS 口语场景：考官/提问者 A 与 考生 B 对白 + IELTS 表达亮点)
//! 4. 支持鼠标滚轮 (WM_MOUSEWHEEL) 滑动翻页、视口剪裁保护与细滑块滚动条。
//! 5. 钉住保护：钉住状态下打字绝对不覆写或刷新当前内容。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush, DeleteObject,
    FillRgn, FrameRgn, GetTextExtentPoint32W, HDC, HFONT, SelectClipRgn,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW, FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, GetWindowRect, HTCAPTION, HTCLIENT, IDC_ARROW,
    LoadCursorW, MA_NOACTIVATE, SW_HIDE, SW_SHOWNA, SendMessageW, ShowWindow,
    WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_MOVE, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use super::layered::{self, Layered};
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianCompanionWindow");
static CLASS: WindowClass = WindowClass::new();

#[inline]
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

// 严格视觉规范色彩
const COLOR_PRIMARY: COLORREF = rgb(37, 99, 235);       // #2563EB
const COLOR_ACTIVE_BG: COLORREF = rgb(235, 242, 255);    // #EBF2FF
const COLOR_CARD_BG: COLORREF = rgb(248, 250, 252);      // #F8FAFC
const COLOR_BORDER: COLORREF = rgb(229, 231, 235);       // #E5E7EB
const COLOR_TEXT_MAIN: COLORREF = rgb(17, 24, 39);       // #111827
const COLOR_TEXT_MUTED: COLORREF = rgb(107, 114, 128);   // #6B7280
const COLOR_WHITE: COLORREF = COLORREF(0x00FFFFFF);

// 标签与徽标色彩
const COLOR_TAG_GREEN_BG: COLORREF = rgb(240, 253, 244); // #F0FDF4
const COLOR_TAG_GREEN_TXT: COLORREF = rgb(22, 163, 74);    // #16A34A
const COLOR_TAG_BLUE_BG: COLORREF = rgb(224, 242, 254);    // #E0F2FE
const COLOR_TAG_BLUE_TXT: COLORREF = rgb(2, 132, 199);     // #0284C7
const COLOR_TAG_GRAY_BG: COLORREF = rgb(243, 244, 246);    // #F3F4F6
const COLOR_TAG_GRAY_TXT: COLORREF = rgb(75, 85, 99);     // #4B5563
const COLOR_TAG_ORANGE_TXT: COLORREF = rgb(217, 119, 6);   // #D97706

/// 替换表达备选项
#[derive(Debug, Clone)]
pub struct ExpressionVariant {
    pub text: String,
    pub tag: String,
    pub meaning: String,
    pub tone: String,
    pub scenario: String,
    pub why_natural: String,
    pub example_en: String,
    pub example_zh: String,
}

/// 真实对话单轮对白
#[derive(Debug, Clone)]
pub struct DialogueTurn {
    pub speaker: String,
    pub text: String,
    pub zh: String,
}

/// 学习助手窗口展示数据
#[derive(Debug, Clone)]
pub struct CompanionData {
    pub target_chinese: String,
    pub primary_english: String,
    pub phonetic: String,
    pub explanation: String,
    pub tone: String,
    pub scenario: String,
    pub nuance: String,
    pub active_tab: usize,
    pub alternatives: Vec<ExpressionVariant>,
    pub collocations: Vec<String>,
    pub examples: Vec<(String, String)>,
    pub ielts_friendly: String,
    pub dialogue: Vec<DialogueTurn>,
    pub saved_notice: Option<String>,
    pub copied_notice: Option<String>,
    pub is_loading: bool,
    pub stream_text: Option<String>,
    /// 三个标签分别使用自己的生成结果，避免切换标签仍显示同一份答案。
    pub tab_results: [Option<String>; 3],
    pub tab_loading: [bool; 3],
    pub error_message: Option<String>,
}

impl CompanionData {
    pub fn loading(target_chinese: String, primary_english: String) -> Self {
        Self {
            target_chinese,
            primary_english,
            phonetic: String::new(),
            explanation: String::new(),
            tone: String::new(),
            scenario: String::new(),
            nuance: String::new(),
            active_tab: 0,
            alternatives: Vec::new(),
            collocations: Vec::new(),
            examples: Vec::new(),
            ielts_friendly: String::new(),
            dialogue: Vec::new(),
            saved_notice: None,
            copied_notice: None,
            is_loading: true,
            stream_text: None,
            tab_results: [None, None, None],
            tab_loading: [true, false, false],
            error_message: None,
        }
    }
}

impl Default for CompanionData {
    fn default() -> Self {
        Self {
            target_chinese: String::new(),
            primary_english: String::new(),
            phonetic: String::new(),
            explanation: String::new(),
            tone: String::new(),
            scenario: String::new(),
            nuance: String::new(),
            active_tab: 0,
            alternatives: Vec::new(),
            collocations: Vec::new(),
            examples: Vec::new(),
            ielts_friendly: String::new(),
            dialogue: Vec::new(),
            saved_notice: None,
            copied_notice: None,
            is_loading: false,
            stream_text: None,
            tab_results: [None, None, None],
            tab_loading: [false, false, false],
            error_message: None,
        }
    }
}

pub struct CompanionWindow {
    hwnd: HWND,
    data: RefCell<CompanionData>,
    last_anchor: Cell<Option<RECT>>,
    user_pos: Cell<Option<(i32, i32)>>,
    current_width: Cell<i32>,
    current_height: Cell<i32>,
    _dark: Cell<bool>,
    pub is_pinned: Cell<bool>,
    pub scroll_y: Cell<i32>,
    pub max_scroll: Cell<i32>,
    current_tab_y: Cell<i32>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl CompanionWindow {
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
        let instance = Rc::new(Self {
            hwnd,
            data: RefCell::new(CompanionData::default()),
            last_anchor: Cell::new(None),
            user_pos: Cell::new(None),
            current_width: Cell::new(440),
            current_height: Cell::new(560),
            _dark: Cell::new(false),
            is_pinned: Cell::new(true),
            scroll_y: Cell::new(0),
            max_scroll: Cell::new(0),
            current_tab_y: Cell::new(116),
            action_cb,
        });
        COMPANIONS.with(|c| c.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&instance)));
        Ok(instance)
    }

    pub fn update_data(&self, data: CompanionData) {
        *self.data.borrow_mut() = data;
        self.scroll_y.set(0);
        self.max_scroll.set(0);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    /// 追加流式文本增量到学习伴侣窗
    pub fn append_stream_chunk(&self, tab: usize, chunk: &str) {
        let mut d = self.data.borrow_mut();
        let tab = tab.min(2);
        d.tab_loading[tab] = true;
        let active_tab = d.active_tab;
        if active_tab == tab {
            d.is_loading = true;
            d.error_message = None;
        }
        let updated = {
            let stream = d.tab_results[tab].get_or_insert_with(String::new);
            stream.push_str(chunk);
            stream.clone()
        };
        if active_tab == tab {
            d.stream_text = Some(updated);
        }
        drop(d);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    /// 学习助手伴侣窗流式生成完成
    pub fn finish_stream(&self, tab: usize) {
        let mut d = self.data.borrow_mut();
        let tab = tab.min(2);
        d.tab_loading[tab] = false;
        if d.active_tab == tab {
            d.is_loading = false;
            if d.tab_results[tab].as_deref().map_or(true, |text| text.trim().is_empty()) {
                d.error_message.get_or_insert_with(|| "模型未返回可显示的内容，请重试。".to_string());
            }
            d.stream_text = d.tab_results[tab].clone();
        }
        drop(d);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    /// 流式请求异常：停止加载并显示明确的错误，而不是误报成功。
    pub fn fail_stream(&self, tab: usize, message: String) {
        let mut d = self.data.borrow_mut();
        let tab = tab.min(2);
        d.tab_loading[tab] = false;
        if d.active_tab == tab {
            d.is_loading = false;
            d.error_message = Some(message);
        }
        drop(d);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn set_tab(&self, tab: usize) {
        let mut data = self.data.borrow_mut();
        data.active_tab = tab.min(2);
        data.stream_text = data.tab_results[data.active_tab].clone();
        data.is_loading = data.tab_loading[data.active_tab];
        data.error_message = None;
        drop(data);
        self.scroll_y.set(0);
        self.max_scroll.set(0);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn set_saved_notice(&self, notice: Option<String>) {
        self.data.borrow_mut().saved_notice = notice;
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn set_copied_notice(&self, notice: Option<String>) {
        self.data.borrow_mut().copied_notice = notice;
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.last_anchor.get().is_some()
    }

    pub fn hide(&self) {
        self.last_anchor.set(None);
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    pub fn show_at(&self, anchor: RECT) -> Result<()> {
        self.last_anchor.set(Some(anchor));
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let dpi = if dpi == 0 { 96 } else { dpi };
        let scale = dpi as f32 / 96.0;
        let margin = layered::shadow_margin(dpi);

        // 桌面规范：保持窄而清晰的学习卡片，内容区滚动，不让长答案撑破窗口。
        let logical_width = 440;
        let logical_height = 560;

        let content_w = (logical_width as f32 * scale).round() as i32;
        let content_h = (logical_height as f32 * scale).round() as i32;
        self.current_width.set(content_w);
        self.current_height.set(content_h);

        let content = (content_w, content_h);

        // 位置定位：优先用户上次拖动的位置，否则智能靠近候选窗
        let (pos_x, pos_y) = if let Some(up) = self.user_pos.get() {
            clamp_position_to_monitor(up, content, margin)
        } else {
            calculate_position(anchor, content, margin)
        };

        let data = self.data.borrow().clone();
        let scroll_y = self.scroll_y.get();
        let is_pinned = self.is_pinned.get();

        layered::composite(
            self.hwnd,
            &Layered {
                content,
                margin,
                win_pos: (pos_x - margin, pos_y - margin),
                win_size: (content.0 + margin * 2, content.1 + margin * 2),
                background: COLOR_WHITE,
                corner_radius: 10, // 规范圆角 10px
                paint: &|hdc, client| {
                    let (max_s, tab_y) = paint_companion(hdc, &data, client, scale, is_pinned, scroll_y);
                    self.max_scroll.set(max_s);
                    self.current_tab_y.set(tab_y);
                },
            },
        )?;

        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                self.hwnd,
                if is_pinned {
                    Some(windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST)
                } else {
                    Some(windows::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST)
                },
                0, 0, 0, 0,
                windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
            );
            if is_pinned {
                let _ = BringWindowToTop(self.hwnd);
            }
            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
        }
        Ok(())
    }
}

thread_local! {
    static COMPANIONS: RefCell<std::collections::HashMap<isize, Rc<CompanionWindow>>> = RefCell::new(std::collections::HashMap::new());
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

fn calculate_position(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(windows::Win32::Foundation::POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let right_x = anchor.right + 8;
    if right_x + content.0 + margin <= work.right {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (right_x, y);
    }
    let bottom_y = anchor.bottom + 8;
    if bottom_y + content.1 + margin <= work.bottom {
        let x = anchor.left.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
        return (x, bottom_y);
    }
    let top_y = anchor.top - content.1 - 8;
    if top_y >= work.top + margin {
        let x = anchor.left.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
        return (x, top_y);
    }
    let left_x = anchor.left - content.0 - 8;
    if left_x >= work.left + margin {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (left_x, y);
    }
    let x = (work.right - content.0 - margin - 20).max(work.left + margin);
    let y = (work.bottom - content.1 - margin - 60).max(work.top + margin);
    (x, y)
}

fn create_font(size: i32, weight: windows::Win32::Graphics::Gdi::FONT_WEIGHT) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FONT_CHARSET(134), // GB2312_CHARSET
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

fn draw_card(hdc: HDC, rect: RECT, bg_col: COLORREF, border_col: COLORREF, radius: i32) {
    let diameter = (radius * 2).max(1);
    unsafe {
        let rgn = CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, diameter, diameter);
        let brush = CreateSolidBrush(bg_col);
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(brush.into());

        let border_brush = CreateSolidBrush(border_col);
        let _ = FrameRgn(hdc, rgn, border_brush, 1, 1);
        let _ = DeleteObject(border_brush.into());
        let _ = DeleteObject(rgn.into());
    }
}

fn draw_round_box(hdc: HDC, rect: RECT, bg_col: COLORREF, radius: i32) {
    let diameter = (radius * 2).max(1);
    unsafe {
        let rgn = CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, diameter, diameter);
        let brush = CreateSolidBrush(bg_col);
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(rgn.into());
    }
}

/// 按像素宽度自动换行绘制文本，返回实际占用高度。
fn draw_wrapped_text_gdi(
    hdc: HDC,
    font: HFONT,
    color: COLORREF,
    left: i32,
    mut y: i32,
    max_width: i32,
    line_height: i32,
    text: &str,
    px: &dyn Fn(i32) -> i32,
) -> i32 {
    if text.is_empty() || max_width <= 0 {
        return 0;
    }
    unsafe {
        SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
    }
    let mut line = String::new();
    let mut used_height = 0;
    let flush_line = |line: &str, y: i32| {
        if !line.is_empty() {
            draw_text_gdi(hdc, font, color, left, y, line);
        }
    };
    for ch in text.chars() {
        if ch == '\n' {
            flush_line(&line, y);
            line.clear();
            y += line_height;
            used_height += line_height;
            continue;
        }
        line.push(ch);
        let utf16: Vec<u16> = line.encode_utf16().collect();
        let mut extent = SIZE::default();
        unsafe { let _ = GetTextExtentPoint32W(hdc, &utf16, &mut extent); }
        if extent.cx > max_width && line.chars().count() > 1 {
            line.pop();
            flush_line(&line, y);
            used_height += line_height;
            y += line_height;
            line.clear();
            line.push(ch);
        }
    }
    if !line.is_empty() {
        flush_line(&line, y);
        used_height += line_height;
    }
    used_height.max(px(1))
}
fn draw_badge(hdc: HDC, font: HFONT, text: &str, x: i32, y: i32, bg_col: COLORREF, txt_col: COLORREF, px: &dyn Fn(i32) -> i32) -> i32 {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        SelectObject(hdc, font.into());
        let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
    }
    let pad_x = px(6);
    let pad_y = px(2);
    let box_w = size.cx + pad_x * 2;
    let box_h = size.cy + pad_y * 2;
    let rect = RECT {
        left: x,
        top: y,
        right: x + box_w,
        bottom: y + box_h,
    };
    draw_round_box(hdc, rect, bg_col, px(4));
    draw_text_gdi(hdc, font, txt_col, x + pad_x, y + pad_y, text);
    box_w
}

/// 核心绘制函数：100% 严格像素级对应参考图
fn paint_companion(hdc: HDC, data: &CompanionData, client: RECT, scale: f32, is_pinned: bool, scroll_y: i32) -> (i32, i32) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };
    let px = |val: i32| (val as f32 * scale).round() as i32;

    // 常用字体定义 (ClearType Microsoft YaHei UI)
    let font_header_title = create_font(px(12), FW_NORMAL);
    let font_beta = create_font(px(10), FW_BOLD);
    let font_zh_target = create_font(px(13), FW_NORMAL);
    let font_en_hero = create_font(px(22), FW_BOLD);
    let font_badge = create_font(px(11), FW_NORMAL);
    let font_section = create_font(px(14), FW_BOLD);
    let font_english = create_font(px(13), FW_BOLD);
    let font_tab_active = create_font(px(12), FW_BOLD);
    let font_tab_normal = create_font(px(12), FW_NORMAL);
    let font_card_title = create_font(px(13), FW_BOLD);
    let font_body = create_font(px(12), FW_NORMAL);
    let font_sub = create_font(px(11), FW_NORMAL);
    let font_btn = create_font(px(12), FW_BOLD);

    let pad_x = px(16);

    // ==========================================
    // 1. 窗口头部 (Header)
    // ==========================================
    // 左侧小水滴/圆圈微标
    let icon_r = RECT { left: pad_x, top: px(14), right: pad_x + px(10), bottom: px(24) };
    draw_round_box(hdc, icon_r, rgb(16, 185, 129), px(5));

    // 标题文本
    let title_w = draw_text_gdi(hdc, font_header_title, COLOR_TEXT_MAIN, pad_x + px(16), px(11), "青简 · 英语学习助手");

    // BETA 徽标胶囊
    let beta_x = pad_x + px(16) + title_w + px(6);
    let beta_rect = RECT { left: beta_x, top: px(12), right: beta_x + px(36), bottom: px(26) };
    draw_round_box(hdc, beta_rect, COLOR_ACTIVE_BG, px(3));
    draw_text_gdi(hdc, font_beta, COLOR_PRIMARY, beta_x + px(5), px(13), "BETA");

    // 右侧窗口控制按钮 (图钉、最小化、关闭)
    let pin_col = if is_pinned { COLOR_PRIMARY } else { COLOR_TEXT_MUTED };
    draw_text_gdi(hdc, font_header_title, pin_col, client.right - px(56), px(11), "⌁");
    draw_text_gdi(hdc, font_header_title, COLOR_TEXT_MUTED, client.right - px(38), px(11), "—");
    draw_text_gdi(hdc, font_header_title, COLOR_TEXT_MUTED, client.right - px(20), px(11), "✕");

    // ==========================================
    // 2. 焦点表达 Hero 区域
    // ==========================================
    let zh_text = if data.target_chinese.is_empty() { "重点表达" } else { &data.target_chinese };
    draw_text_gdi(hdc, font_zh_target, COLOR_TEXT_MAIN, pad_x, px(38), zh_text);

    let en_text = if data.primary_english.is_empty() {
        if data.is_loading { "Analyzing..." } else { "Waiting for input..." }
    } else {
        &data.primary_english
    };
    // Hero 英文只绘制在标题安全区内，长句自动换行，绝不压到右边界。
    let hero_max_w = client.right - pad_x * 2;
    let hero_h = draw_wrapped_text_gdi(
        hdc, font_en_hero, COLOR_PRIMARY, pad_x, px(56), hero_max_w,
        px(25), en_text, &px,
    );
    let hero_bottom = px(56) + hero_h;
    // 三个标签紧跟 Hero 下面排布，避免固定坐标与长英文标题重叠。
    let dynamic_tab_y = (hero_bottom + px(32)).max(px(116));

    // 属性标签胶囊行
    let mut badge_x = pad_x;
    let badge_y = hero_bottom + px(5);
    if !data.tone.is_empty() {
        badge_x += draw_badge(hdc, font_badge, &data.tone, badge_x, badge_y, COLOR_TAG_GREEN_BG, COLOR_TAG_GREEN_TXT, &px) + px(6);
    } else if data.is_loading {
        badge_x += draw_badge(hdc, font_badge, "分析中", badge_x, badge_y, COLOR_TAG_GREEN_BG, COLOR_TAG_GREEN_TXT, &px) + px(6);
    }
    if !data.scenario.is_empty() {
        badge_x += draw_badge(hdc, font_badge, &data.scenario, badge_x, badge_y, COLOR_TAG_BLUE_BG, COLOR_TAG_BLUE_TXT, &px) + px(6);
    }
    if !data.collocations.is_empty() {
        let _ = draw_badge(hdc, font_badge, format!("{} 条搭配", data.collocations.len()).as_str(), badge_x, badge_y, COLOR_TAG_GRAY_BG, COLOR_TAG_GRAY_TXT, &px);
    }

    // 只有模型明确提供对应信息时才显示等级徽标，避免虚构 B1-B2 / IELTS 评价。
    if !data.ielts_friendly.is_empty() {
        let cefr_w = px(44);
        let cefr_rect = RECT { left: client.right - pad_x - cefr_w, top: badge_y, right: client.right - pad_x, bottom: badge_y + px(18) };
        draw_round_box(hdc, cefr_rect, COLOR_ACTIVE_BG, px(4));
        draw_text_gdi(hdc, font_badge, COLOR_PRIMARY, client.right - pad_x - cefr_w + px(6), badge_y + px(2), "IELTS");
    }

    // ==========================================
    // 3. 三个主选项卡 (Segmented Tabs)
    // ==========================================
    let tab_y = dynamic_tab_y;
    let tab_h = px(30);
    let tabs = ["语感解析", "更自然地说", "真实对话"];
    let tab_w = (client.right - pad_x * 2 - px(16)) / 3;

    for (i, name) in tabs.iter().enumerate() {
        let is_active = data.active_tab == i;
        let tx = pad_x + (tab_w + px(8)) * (i as i32);
        let tab_rect = RECT { left: tx, top: tab_y, right: tx + tab_w, bottom: tab_y + tab_h };

        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut sz = SIZE::default();
        if is_active {
            draw_card(hdc, tab_rect, COLOR_ACTIVE_BG, COLOR_PRIMARY, px(6));
            unsafe {
                SelectObject(hdc, font_tab_active.into());
                let _ = GetTextExtentPoint32W(hdc, &utf16, &mut sz);
            }
            let text_x = tx + (tab_w - sz.cx) / 2;
            draw_text_gdi(hdc, font_tab_active, COLOR_PRIMARY, text_x, tab_y + px(6), name);
        } else {
            draw_card(hdc, tab_rect, COLOR_WHITE, COLOR_BORDER, px(6));
            unsafe {
                SelectObject(hdc, font_tab_normal.into());
                let _ = GetTextExtentPoint32W(hdc, &utf16, &mut sz);
            }
            let text_x = tx + (tab_w - sz.cx) / 2;
            draw_text_gdi(hdc, font_tab_normal, COLOR_TEXT_MUTED, text_x, tab_y + px(6), name);
        }
    }

    // ==========================================
    // 4. 内容详情区 (带剪裁与滚轮支持)
    // ==========================================
    let viewport_top = tab_y + tab_h + px(8);
    let viewport_bottom = client.bottom - px(48);
    let viewport_h = viewport_bottom - viewport_top;

    let clip_rgn = unsafe { CreateRectRgn(client.left, viewport_top, client.right, viewport_bottom) };
    unsafe { SelectClipRgn(hdc, Some(clip_rgn)) };

    let base_y = viewport_top - scroll_y;

    let stream_text = data.stream_text.as_deref().unwrap_or("").trim();
    let title = match data.active_tab { 0 => "语感解析", 1 => "更自然地说", _ => "真实对话" };
    let content_total_h = if data.is_loading && stream_text.is_empty() {
        let mut cy = base_y + px(20);
        draw_text_gdi(hdc, font_card_title, COLOR_PRIMARY, pad_x, cy, &format!("正在生成{}…", title));
        cy += px(26);
        let width = (client.right - pad_x * 2 - px(12)).max(px(80));
        for ratio in [0.94f32, 0.76, 0.88, 0.58] {
            let bw = ((width as f32 * ratio) as i32).min(width);
            draw_round_box(hdc, RECT { left: pad_x, top: cy, right: pad_x + bw, bottom: cy + px(10) }, COLOR_CARD_BG, px(4));
            cy += px(18);
        }
        cy - base_y + px(24)
    } else if !stream_text.is_empty() {
        use crate::ui::markdown::{parse_markdown_blocks, wrap_text_lines, MarkdownBlock, InlineSpan};
        let mut cy = base_y + px(8);
        draw_text_gdi(hdc, font_card_title, COLOR_PRIMARY, pad_x, cy, if data.is_loading { "正在生成" } else { "生成结果" });
        cy += px(24);
        let max_w = (client.right - pad_x * 2 - px(12)).max(px(60));
        let blocks = parse_markdown_blocks(stream_text);
        let mut next_is_example = false;
        for block in blocks {
            let plain = |spans: Vec<InlineSpan>| -> String {
                spans.into_iter().map(|span| match span { InlineSpan::Normal(t) | InlineSpan::Bold(t) | InlineSpan::Code(t) => t }).collect()
            };
            match block {
                MarkdownBlock::Header { level, text } => {
                    cy += px(if level == 1 { 10 } else { 7 });
                    let f = if level <= 2 { font_section } else { font_card_title };
                    if level <= 2 {
                        let rule = RECT { left: pad_x, top: cy + px(3), right: pad_x + px(3), bottom: cy + px(18) };
                        draw_round_box(hdc, rule, COLOR_PRIMARY, px(1));
                    }
                    let lines = wrap_text_lines(hdc, f, max_w - if level <= 2 { px(10) } else { 0 }, &text);
                    for line in &lines {
                        draw_text_gdi(hdc, f, if level <= 2 { COLOR_TEXT_MAIN } else { COLOR_TEXT_MUTED }, pad_x + if level <= 2 { px(10) } else { 0 }, cy, line);
                        cy += px(if level == 1 { 24 } else { 21 });
                    }
                    next_is_example = text.to_lowercase().contains("例句") || text.to_lowercase().contains("example");
                    cy += px(3);
                }
                MarkdownBlock::Paragraph { spans } => {
                    let content = plain(spans);
                    if next_is_example {
                        let lines = wrap_text_lines(hdc, font_body, max_w - px(20), &content);
                        let card_h = px(12) + lines.len() as i32 * px(20);
                        let card = RECT { left: pad_x, top: cy, right: client.right - pad_x, bottom: cy + card_h };
                        draw_card(hdc, card, COLOR_CARD_BG, COLOR_BORDER, px(7));
                        let mut ly = cy + px(7);
                        for line in &lines { draw_text_gdi(hdc, font_body, COLOR_TEXT_MAIN, pad_x + px(10), ly, line); ly += px(20); }
                        cy += card_h + px(7);
                        next_is_example = false;
                    } else {
                        for line in wrap_text_lines(hdc, font_body, max_w, &content) {
                            if !line.is_empty() { draw_text_gdi(hdc, font_body, COLOR_TEXT_MAIN, pad_x, cy, &line); }
                            cy += px(20);
                        }
                        cy += px(5);
                    }
                }
                MarkdownBlock::BulletItem { text } => {
                    let content = plain(text);
                    let mut label = content.trim().to_string();
                    let italic_wrapped = label.starts_with('*') && label.ends_with('*') && label.len() > 1;
                    if italic_wrapped { label = label[1..label.len()-1].to_string(); }
                    let lines = wrap_text_lines(hdc, font_body, max_w - px(18), &label);
                    draw_round_box(hdc, RECT { left: pad_x + px(2), top: cy + px(7), right: pad_x + px(7), bottom: cy + px(12) }, COLOR_PRIMARY, px(2));
                    for line in &lines { draw_text_gdi(hdc, if italic_wrapped { font_english } else { font_body }, COLOR_TEXT_MAIN, pad_x + px(16), cy, line); cy += px(20); }
                    cy += px(3);
                }
                MarkdownBlock::NumberedItem { number, text } => {
                    let content = plain(text);
                    let lines = wrap_text_lines(hdc, font_body, max_w - px(36), &content);
                    let badge = RECT { left: pad_x, top: cy - px(1), right: pad_x + px(24), bottom: cy + px(23) };
                    draw_round_box(hdc, badge, COLOR_ACTIVE_BG, px(5));
                    draw_text_gdi(hdc, font_card_title, COLOR_PRIMARY, pad_x + px(7), cy + px(2), &number.to_string());
                    for line in &lines { draw_text_gdi(hdc, font_body, COLOR_TEXT_MAIN, pad_x + px(32), cy, line); cy += px(20); }
                    cy += px(6);
                }
                MarkdownBlock::Quote { text } => {
                    let content = plain(text);
                    let lines = wrap_text_lines(hdc, font_body, max_w - px(18), &content);
                    let total_h = lines.len().max(1) as i32 * px(20) + px(10);
                    draw_card(hdc, RECT { left: pad_x, top: cy, right: client.right-pad_x, bottom: cy+total_h }, COLOR_CARD_BG, COLOR_BORDER, px(6));
                    for line in &lines { draw_text_gdi(hdc, font_body, COLOR_TEXT_MUTED, pad_x+px(10), cy+px(5), line); cy += px(20); }
                    cy += px(15);
                }
                MarkdownBlock::CodeBlock { lines, .. } => {
                    let lh = px(18); let bh = (lines.len() as i32 * lh + px(12)).max(px(28));
                    draw_card(hdc, RECT { left: pad_x, top: cy, right: client.right-pad_x, bottom: cy+bh }, COLOR_CARD_BG, COLOR_BORDER, px(6));
                    cy += px(6);
                    for line in lines { for wrapped in wrap_text_lines(hdc, font_sub, max_w-px(10), &line) { draw_text_gdi(hdc, font_sub, COLOR_TEXT_MAIN, pad_x+px(6), cy, &wrapped); cy += lh; } }
                    cy += px(10);
                }
            }
        }
        if data.is_loading { cy += px(6); draw_text_gdi(hdc, font_sub, COLOR_TEXT_MUTED, pad_x, cy, "正在继续生成…"); cy += px(20); }
        if let Some(error) = &data.error_message { cy += px(6); cy += draw_wrapped_text_gdi(hdc, font_sub, COLOR_TAG_ORANGE_TXT, pad_x, cy, max_w, px(17), error, &px); }
        cy - base_y + px(16)
    } else if let Some(error) = &data.error_message {
        let mut cy = base_y + px(20);
        draw_text_gdi(hdc, font_card_title, COLOR_TAG_ORANGE_TXT, pad_x, cy, "请求失败");
        cy += px(26);
        cy += draw_wrapped_text_gdi(hdc, font_body, COLOR_TEXT_MAIN, pad_x, cy, client.right-pad_x*2-px(8), px(19), error, &px);
        cy - base_y + px(20)
    } else {
        let mut cy = base_y + px(20);
        draw_text_gdi(hdc, font_section, COLOR_TEXT_MAIN, pad_x, cy, match data.active_tab { 0 => "表达的意思与语气", 1 => "更地道的表达", _ => "真实口语场景" });
        cy += px(28);
        draw_text_gdi(hdc, font_body, COLOR_TEXT_MUTED, pad_x, cy, "正在准备内容，完整结果会显示在这里。");
        cy - base_y + px(32)
    };
    let max_s = (content_total_h - viewport_h).max(0);

    // 绘制滚动条滑块 (当内容超出视口时)
    if max_s > 0 {
        let scrollbar_w = px(4);
        let scrollbar_x = client.right - px(8);
        let thumb_ratio = (viewport_h as f32 / content_total_h as f32).clamp(0.15, 0.9);
        let thumb_h = ((viewport_h as f32 * thumb_ratio) as i32).max(px(24));
        let max_thumb_travel = viewport_h - thumb_h;
        let thumb_y = viewport_top + ((scroll_y as f32 / max_s as f32) * max_thumb_travel as f32) as i32;

        let thumb_rect = RECT {
            left: scrollbar_x,
            top: thumb_y,
            right: scrollbar_x + scrollbar_w,
            bottom: thumb_y + thumb_h,
        };
        draw_round_box(hdc, thumb_rect, rgb(209, 213, 219), px(2));
    }

    // 恢复裁剪区域
    unsafe {
        SelectClipRgn(hdc, None);
        let _ = DeleteObject(clip_rgn.into());
    }

    // ==========================================
    // 5. 底部固定操作栏 (Footer - 统一设计)
    // ==========================================
    let sep_y = client.bottom - px(44);
    let sep_r = RECT { left: pad_x, top: sep_y, right: client.right - pad_x, bottom: sep_y + 1 };
    unsafe {
        let brush = CreateSolidBrush(COLOR_BORDER);
        let _ = windows::Win32::Graphics::Gdi::FillRect(hdc, &sep_r, brush);
        let _ = DeleteObject(brush.into());
    }

    let btn_y = client.bottom - px(36);
    let btn_h = px(28);

    // 按钮 1: 收藏
    let btn1_w = px(112);
    let btn1_r = RECT { left: pad_x, top: btn_y, right: pad_x + btn1_w, bottom: btn_y + btn_h };
    draw_card(hdc, btn1_r, COLOR_WHITE, COLOR_BORDER, px(6));
    let is_fav = data.saved_notice.is_some();
    if is_fav {
        draw_text_gdi(hdc, font_btn, COLOR_TAG_ORANGE_TXT, pad_x + px(24), btn_y + px(6), "★ 已收藏");
    } else {
        draw_text_gdi(hdc, font_btn, COLOR_TEXT_MAIN, pad_x + px(30), btn_y + px(6), "☆ 收藏");
    }

    // 按钮 2: 复制
    let btn2_x = pad_x + btn1_w + px(8);
    let btn2_w = px(112);
    let btn2_r = RECT { left: btn2_x, top: btn_y, right: btn2_x + btn2_w, bottom: btn_y + btn_h };
    draw_card(hdc, btn2_r, COLOR_WHITE, COLOR_BORDER, px(6));
    let copy_text = data.copied_notice.as_deref().unwrap_or("复制");
    draw_text_gdi(hdc, font_btn, COLOR_TEXT_MAIN, btn2_x + px(38), btn_y + px(6), copy_text);

    // 按钮 3: 问 AI (主色蓝底实心)
    let btn3_x = btn2_x + btn2_w + px(8);
    let btn3_r = RECT { left: btn3_x, top: btn_y, right: client.right - pad_x, bottom: btn_y + btn_h };
    draw_round_box(hdc, btn3_r, COLOR_PRIMARY, px(6));
    draw_text_gdi(hdc, font_btn, COLOR_WHITE, btn3_x + px(44), btn_y + px(6), "问 AI");

    // 释放字体资源
    unsafe {
        let _ = DeleteObject(font_header_title.into());
        let _ = DeleteObject(font_beta.into());
        let _ = DeleteObject(font_zh_target.into());
        let _ = DeleteObject(font_en_hero.into());
        let _ = DeleteObject(font_badge.into());
        let _ = DeleteObject(font_section.into());
        let _ = DeleteObject(font_english.into());
        let _ = DeleteObject(font_tab_active.into());
        let _ = DeleteObject(font_tab_normal.into());
        let _ = DeleteObject(font_card_title.into());
        let _ = DeleteObject(font_body.into());
        let _ = DeleteObject(font_sub.into());
        let _ = DeleteObject(font_btn.into());
    }

    (max_s, tab_y)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            // 纯阅读学习，绝对不抢应用输入焦点
            LRESULT(MA_NOACTIVATE as isize)
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_MOVE | WM_EXITSIZEMOVE => {
            let comp = COMPANIONS.with(|c| c.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(comp) = comp {
                let mut rc = RECT::default();
                if unsafe { GetWindowRect(hwnd, &mut rc) }.is_ok() {
                    let margin = layered::shadow_margin(unsafe { GetDpiForWindow(hwnd) }.max(96));
                    comp.user_pos.set(Some((rc.left + margin, rc.top + margin)));
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wparam.0 >> 16) as i16;
            let step = 32;
            let comp = COMPANIONS.with(|c| c.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(comp) = comp {
                let cur = comp.scroll_y.get();
                let max = comp.max_scroll.get();
                if max > 0 {
                    let next = if delta > 0 {
                        (cur - step).max(0)
                    } else {
                        (cur + step).min(max)
                    };
                    if next != cur {
                        comp.scroll_y.set(next);
                        if let Some(anchor) = comp.last_anchor.get() {
                            let _ = comp.show_at(anchor);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let comp = COMPANIONS.with(|c| c.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(comp) = comp {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                let margin = layered::shadow_margin(dpi);
                let scale = dpi as f32 / 96.0;
                let px = |val: i32| (val as f32 * scale).round() as i32;

                let cx = x - margin;
                let cy = y - margin;
                let w = comp.current_width.get();
                let h = comp.current_height.get();
                let pad_x = px(16);

                if comp.is_pinned.get() {
                    unsafe {
                        let _ = BringWindowToTop(hwnd);
                    }
                }

                // 1. 关闭按钮 (右上角)
                if cx >= w - px(30) && cx <= w && cy <= px(32) {
                    comp.hide();
                    (comp.action_cb)(CandidateAction::CloseAssistant);
                    return LRESULT(0);
                }

                // 2. 置顶图钉 (cx in w - px(60) .. w - px(30))
                if cx >= w - px(60) && cx < w - px(30) && cy <= px(32) {
                    let pinned = !comp.is_pinned.get();
                    comp.is_pinned.set(pinned);
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                            hwnd,
                            if pinned {
                                Some(windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST)
                            } else {
                                Some(windows::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST)
                            },
                            0, 0, 0, 0,
                            windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                                | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                                | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
                        );
                        if pinned {
                            let _ = BringWindowToTop(hwnd);
                        }
                    }
                    if let Some(anchor) = comp.last_anchor.get() {
                        let _ = comp.show_at(anchor);
                    }
                    return LRESULT(0);
                }

                // 3. 标题栏可拖动区域 (cy <= 32 且未点击按钮) -> 原生拖动
                if cy <= px(32) {
                    unsafe {
                        let _ = ReleaseCapture();
                        let _ = SendMessageW(
                            hwnd,
                            WM_NCLBUTTONDOWN,
                            Some(WPARAM(HTCAPTION as usize)),
                            Some(LPARAM(0)),
                        );
                    }
                    return LRESULT(0);
                }

                // 4. 点击 Hero 可复制英文焦点表达
                if cy >= px(45) && cy <= px(80) && cx >= pad_x && cx <= w - pad_x {
                    let text = comp.data.borrow().primary_english.clone();
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        comp.set_copied_notice(Some("已复制✓".to_string()));
                        return LRESULT(0);
                    }
                }

                // 5. Tab 栏点击：使用本次绘制算出的动态 Y 坐标，兼容多行英文 Hero。
                let tab_y = comp.current_tab_y.get();
                if cy >= tab_y && cy <= tab_y + px(30) {
                    let tab_w = (w - pad_x * 2 - px(16)) / 3;
                    for i in 0..3 {
                        let tx = pad_x + (tab_w + px(8)) * (i as i32);
                        if cx >= tx && cx <= tx + tab_w {
                            comp.set_tab(i);
                            return LRESULT(0);
                        }
                    }
                }

                // 6. 底部操作栏点击，与视觉 footer 高度保持一致。
                if cy >= h - px(48) && cy <= h {
                    let btn1_w = px(112);
                    let btn2_x = pad_x + btn1_w + px(8);
                    let btn2_w = px(112);
                    let btn3_x = btn2_x + btn2_w + px(8);

                    // 点击 [收藏]
                    if cx >= pad_x && cx <= pad_x + btn1_w {
                        let target = comp.data.borrow().target_chinese.clone();
                        let expr = comp.data.borrow().primary_english.clone();
                        if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                            let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                            vault.save_expression(&target, &expr, Some("Natural"), None);
                            let _ = vault.flush();
                        }
                        comp.set_saved_notice(Some("★ 已收藏".to_string()));
                        return LRESULT(0);
                    }

                    // 点击 [复制]
                    if cx >= btn2_x && cx <= btn2_x + btn2_w {
                        let text = comp.data.borrow().primary_english.clone();
                        if !text.is_empty() {
                            copy_to_clipboard(&text);
                            comp.set_copied_notice(Some("已复制✓".to_string()));
                        }
                        return LRESULT(0);
                    }

                    // 点击 [问 AI]
                    if cx >= btn3_x && cx <= w - pad_x {
                        (comp.action_cb)(CandidateAction::OpenAiAssistant);
                        return LRESULT(0);
                    }
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn copy_to_clipboard(text: &str) {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes_len = utf16.len() * std::mem::size_of::<u16>();
            if let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes_len) {
                let ptr = GlobalLock(handle) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
                    let _ = GlobalUnlock(handle);
                    let _ = SetClipboardData(13 /* CF_UNICODETEXT */, Some(windows::Win32::Foundation::HANDLE(handle.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }
}
