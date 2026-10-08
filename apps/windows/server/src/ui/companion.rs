//! English Assistant (学习助手伴侣窗)。
//! 独立的轻量 Companion Panel，WS_EX_NOACTIVATE，小、白、圆角、轻阴影，绝不抢宿主焦点。
//! 包含：当前候选表达、更多地道口语、用法与高频搭配、场景示例、收藏到表达本与直达练习。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetTextExtentPoint32W, HDC, HFONT, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW, FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HTCLIENT, IDC_ARROW, LoadCursorW,
    MA_NOACTIVATE, SW_HIDE, SW_SHOWNA, ShowWindow, WM_LBUTTONDOWN, WM_MOUSEACTIVATE,
    WM_NCHITTEST, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use super::layered::{self, Layered};
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianCompanionWindow");
static CLASS: WindowClass = WindowClass::new();

const COMPANION_WIDTH: i32 = 430;
const COMPANION_HEIGHT: i32 = 280;

/// 更多表达备选项
#[derive(Debug, Clone)]
pub struct ExpressionVariant {
    pub text: String,
    pub tag: String,
    pub meaning: String,
    pub why_natural: String,
}

/// 学习助手窗口展示数据
#[derive(Debug, Clone)]
pub struct CompanionData {
    pub target_chinese: String,
    pub primary_english: String,
    pub phonetic: String,
    pub explanation: String,
    pub active_tab: usize,
    pub alternatives: Vec<ExpressionVariant>,
    pub collocations: Vec<String>,
    pub examples: Vec<(String, String)>,
    pub ielts_friendly: String,
    pub saved_notice: Option<String>,
}

impl Default for CompanionData {
    fn default() -> Self {
        Self {
            target_chinese: "确实有道理".to_string(),
            primary_english: "That makes perfect sense.".to_string(),
            phonetic: "/meɪk sens/".to_string(),
            explanation: "表示“有道理/讲得通，能够理解”。这是日常交流中最常用、最自然的方式。".to_string(),
            active_tab: 0,
            alternatives: vec![
                ExpressionVariant {
                    text: "That totally makes sense.".to_string(),
                    tag: "Casual".to_string(),
                    meaning: "完全讲得通".to_string(),
                    why_natural: "口语常用强调".to_string(),
                },
                ExpressionVariant {
                    text: "I see what you mean.".to_string(),
                    tag: "Natural".to_string(),
                    meaning: "我明白你的意思".to_string(),
                    why_natural: "日常沟通赞同".to_string(),
                },
                ExpressionVariant {
                    text: "I get where you're coming from.".to_string(),
                    tag: "Natural".to_string(),
                    meaning: "我理解你的立场".to_string(),
                    why_natural: "共情地道口语".to_string(),
                },
                ExpressionVariant {
                    text: "That's a good point.".to_string(),
                    tag: "Neutral".to_string(),
                    meaning: "说得有道理".to_string(),
                    why_natural: "探讨或开会通用".to_string(),
                },
                ExpressionVariant {
                    text: "That makes a lot of sense.".to_string(),
                    tag: "Stronger".to_string(),
                    meaning: "非常有道理".to_string(),
                    why_natural: "程度加强表达".to_string(),
                },
            ],
            collocations: vec![
                "make perfect sense".to_string(),
                "in a way".to_string(),
                "I see your point".to_string(),
                "that explains it".to_string(),
                "it adds up".to_string(),
            ],
            examples: vec![
                ("That makes perfect sense.".to_string(), "确实很有道理。".to_string()),
                ("It makes a lot of sense now.".to_string(), "现在我彻底明白了。".to_string()),
            ],
            ielts_friendly: "That is entirely logical and coherent.".to_string(),
            saved_notice: None,
        }
    }
}

pub struct CompanionWindow {
    hwnd: HWND,
    data: RefCell<CompanionData>,
    last_anchor: Cell<Option<RECT>>,
    dark: Cell<bool>,
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
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
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
            dark: Cell::new(false),
            action_cb,
        });
        COMPANIONS.with(|c| c.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&instance)));
        Ok(instance)
    }

    pub fn update_data(&self, data: CompanionData) {
        *self.data.borrow_mut() = data;
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn set_tab(&self, tab: usize) {
        self.data.borrow_mut().active_tab = tab;
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
        let margin = layered::shadow_margin(dpi);
        let content = (COMPANION_WIDTH, COMPANION_HEIGHT);

        // 优先放在候选窗右侧，若右边越界则贴在下方
        let (pos_x, pos_y) = calculate_position(anchor, content, margin);

        let data = self.data.borrow();
        let bg_color = if self.dark.get() {
            COLORREF(0x00202020)
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
                corner_radius: 12,
                paint: &|hdc, client| {
                    paint_companion(hdc, &data, client, self.dark.get());
                },
            },
        )?;
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNA) };
        Ok(())
    }
}

thread_local! {
    static COMPANIONS: RefCell<std::collections::HashMap<isize, Rc<CompanionWindow>>> = RefCell::new(std::collections::HashMap::new());
}

fn calculate_position(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(windows::Win32::Foundation::POINT {
        x: anchor.left,
        y: anchor.top,
    });
    // 1. 尝试放在 Candidate Window 右侧相距 8px
    let right_x = anchor.right + 8;
    let right_y = anchor.top;
    if right_x + content.0 + margin <= work.right {
        let y = right_y.clamp(work.top + margin, work.bottom - content.1 - margin);
        return (right_x, y);
    }
    // 2. 尝试放在 Candidate Window 下方相距 8px
    let bottom_x = anchor.left.clamp(work.top + margin, work.right - content.0 - margin);
    let bottom_y = anchor.bottom + 8;
    if bottom_y + content.1 + margin <= work.bottom {
        return (bottom_x, bottom_y);
    }
    // 3. 兜底在工作区内保证完全可见
    let x = (anchor.left - content.0 - 8).max(work.left + margin);
    let y = anchor.top.clamp(work.top + margin, work.bottom - content.1 - margin);
    (x, y)
}

fn paint_companion(hdc: HDC, data: &CompanionData, client: RECT, dark: bool) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };

    let text_color = if dark { COLORREF(0x00E0E0E0) } else { COLORREF(0x001F1F1F) };
    let sub_color = if dark { COLORREF(0x00909090) } else { COLORREF(0x006B7280) };
    let brand_color = COLORREF(0x001673F9);
    let badge_bg = if dark { COLORREF(0x00333333) } else { COLORREF(0x00F3F4F6) };
    let active_bg = if dark { COLORREF(0x003A404D) } else { COLORREF(0x00E0E7FF) };

    let font_title = create_font(12, FW_NORMAL, false);
    let font_target = create_font(13, FW_NORMAL, false);
    let font_primary = create_font(16, FW_BOLD, false);
    let font_tab = create_font(12, FW_NORMAL, false);
    let font_body = create_font(12, FW_NORMAL, false);
    let font_tag = create_font(11, FW_NORMAL, false);

    // 1. 顶部标题栏 (Header)
    draw_text_gdi(hdc, font_title, sub_color, 16, 12, "🇺🇸 English Assistant · 学习助手");
    draw_text_gdi(hdc, font_title, sub_color, client.right - 28, 12, "×");

    // 2. 焦点主词条卡片区域
    draw_text_gdi(hdc, font_target, sub_color, 16, 38, &format!("目标表达：{}", data.target_chinese));
    let prim_w = draw_text_gdi(hdc, font_primary, brand_color, 16, 58, &data.primary_english);
    draw_text_gdi(hdc, font_target, sub_color, 16 + prim_w + 10, 60, "🔊   📋");

    // 分隔线
    let sep = RECT { left: 16, top: 88, right: client.right - 16, bottom: 89 };
    super::candidates::view::fill_rect(hdc, sep, if dark { COLORREF(0x00333333) } else { COLORREF(0x00E5E7EB) });

    // 3. 导航 Tab 栏
    let tabs = ["更多译法", "用法说明", "相关搭配", "场景示例"];
    let mut tab_x = 16;
    for (i, name) in tabs.iter().enumerate() {
        let is_active = data.active_tab == i;
        let tab_w = 64;
        let tab_rect = RECT { left: tab_x, top: 96, right: tab_x + tab_w, bottom: 120 };
        if is_active {
            super::candidates::view::fill_round_rect(hdc, tab_rect, active_bg, 4);
            draw_text_gdi(hdc, font_tab, brand_color, tab_x + 8, 100, name);
        } else {
            draw_text_gdi(hdc, font_tab, sub_color, tab_x + 8, 100, name);
        }
        tab_x += tab_w + 6;
    }

    // 右侧功能键：[⭐ 收藏] 与 [🚀 进入练习]
    let save_text = if data.saved_notice.is_some() { "⭐ 已收藏" } else { "⭐ 收藏" };
    let save_rect = RECT { left: client.right - 165, top: 96, right: client.right - 105, bottom: 120 };
    super::candidates::view::fill_round_rect(hdc, save_rect, badge_bg, 4);
    draw_text_gdi(hdc, font_tag, text_color, client.right - 160, 101, save_text);

    let drill_rect = RECT { left: client.right - 98, top: 96, right: client.right - 16, bottom: 120 };
    super::candidates::view::fill_round_rect(hdc, drill_rect, brand_color, 4);
    draw_text_gdi(hdc, font_tag, COLORREF(0x00FFFFFF), client.right - 92, 101, "🚀 进入练习");

    // 4. 内容详情展示区 (y = 128..265)
    let content_y = 130;
    match data.active_tab {
        0 => {
            // 更多译法
            let mut y = content_y;
            for (idx, alt) in data.alternatives.iter().take(4).enumerate() {
                let idx_str = format!("{}.", idx + 1);
                draw_text_gdi(hdc, font_body, sub_color, 16, y, &idx_str);
                let w = draw_text_gdi(hdc, font_body, text_color, 32, y, &alt.text);
                
                // Register 胶囊
                let tag_rect = RECT { left: 32 + w + 8, top: y, right: 32 + w + 8 + 50, bottom: y + 16 };
                super::candidates::view::fill_round_rect(hdc, tag_rect, active_bg, 3);
                draw_text_gdi(hdc, font_tag, brand_color, 32 + w + 14, y + 1, &alt.tag);

                if !alt.meaning.is_empty() {
                    draw_text_gdi(hdc, font_tag, sub_color, 32 + w + 66, y + 1, &format!("› {}", alt.meaning));
                }
                draw_text_gdi(hdc, font_tag, sub_color, client.right - 34, y, "🔊");
                y += 24;
            }
        }
        1 => {
            // 用法说明
            let mut y = content_y;
            let title = format!("{}  {}  [高频搭配]", data.target_chinese, data.phonetic);
            draw_text_gdi(hdc, font_body, brand_color, 16, y, &title);
            y += 22;
            draw_text_gdi(hdc, font_tag, text_color, 16, y, &data.explanation);
            y += 22;
            if let Some((ex_en, ex_zh)) = data.examples.first() {
                draw_text_gdi(hdc, font_tag, sub_color, 16, y, &format!("例句：• {} ({})", ex_en, ex_zh));
                y += 20;
            }
            if let Some((ex_en, ex_zh)) = data.examples.get(1) {
                draw_text_gdi(hdc, font_tag, sub_color, 16, y, &format!("     • {} ({})", ex_en, ex_zh));
                y += 20;
            }
            // 雅思替换说明
            if !data.ielts_friendly.is_empty() {
                draw_text_gdi(hdc, font_tag, COLORREF(0x000F8A5F), 16, y, &format!("雅思高分替换：{}", data.ielts_friendly));
            }
        }
        2 => {
            // 相关搭配 (Collocations)
            let mut y = content_y;
            draw_text_gdi(hdc, font_body, text_color, 16, y, "常用黄金搭配 (Common Collocations)：");
            y += 24;
            let mut col_x = 16;
            for col in &data.collocations {
                let col_w = (col.len() as i32 * 7) + 16;
                if col_x + col_w > client.right - 16 {
                    col_x = 16;
                    y += 24;
                }
                let rect = RECT { left: col_x, top: y, right: col_x + col_w, bottom: y + 20 };
                super::candidates::view::fill_round_rect(hdc, rect, badge_bg, 4);
                draw_text_gdi(hdc, font_tag, brand_color, col_x + 8, y + 2, col);
                col_x += col_w + 8;
            }
        }
        3 => {
            // 场景示例
            let mut y = content_y;
            draw_text_gdi(hdc, font_body, text_color, 16, y, "场景会话示例与语域转换：");
            y += 24;
            draw_text_gdi(hdc, font_tag, sub_color, 16, y, "💬 职场与生活场景：在认同对方观点或理解工作方案时使用。");
            y += 22;
            draw_text_gdi(hdc, font_tag, text_color, 16, y, &format!("A: If we batch requests, the latency drops by 50%."));
            y += 20;
            draw_text_gdi(hdc, font_tag, brand_color, 16, y, &format!("B: Exactly, that makes perfect sense."));
            y += 22;
            draw_text_gdi(hdc, font_tag, COLORREF(0x000F8A5F), 16, y, &format!("IELTS Speaking 替换: {}", data.ielts_friendly));
        }
        _ => {}
    }

    // 释放 GDI 字体
    unsafe {
        let _ = DeleteObject(font_title.into());
        let _ = DeleteObject(font_target.into());
        let _ = DeleteObject(font_primary.into());
        let _ = DeleteObject(font_tab.into());
        let _ = DeleteObject(font_body.into());
        let _ = DeleteObject(font_tag.into());
    }
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

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            // 纯阅读/学习，绝对不抢焦点，不激活窗口，防止破坏正在输入的宿主程序焦点与拼音
            LRESULT(MA_NOACTIVATE as isize)
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let comp = COMPANIONS.with(|c| c.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(comp) = comp {
                let margin = layered::shadow_margin(unsafe { GetDpiForWindow(hwnd) }.max(96));
                let cx = x - margin;
                let cy = y - margin;

                // 1. 关闭按钮 (右上角)
                if cx >= COMPANION_WIDTH - 36 && cx <= COMPANION_WIDTH && cy <= 30 {
                    comp.hide();
                    return LRESULT(0);
                }

                // 2. 复制到剪贴板图标 (cx in 240..300, cy in 50..80)
                if cy >= 50 && cy <= 80 && cx >= 180 && cx <= 280 {
                    let text = comp.data.borrow().primary_english.clone();
                    copy_to_clipboard(&text);
                    comp.set_saved_notice(Some("已复制到剪贴板".to_string()));
                    return LRESULT(0);
                }

                // 3. Tab 栏点击 (y in 92..124)
                if cy >= 92 && cy <= 124 {
                    // Tab 0..3: 每个宽度 70
                    if cx >= 16 && cx <= 16 + 70 * 4 {
                        let tab_idx = ((cx - 16) / 70) as usize;
                        if tab_idx < 4 {
                            comp.set_tab(tab_idx);
                            return LRESULT(0);
                        }
                    }
                    // 点击 [⭐ 收藏]
                    if cx >= COMPANION_WIDTH - 165 && cx <= COMPANION_WIDTH - 105 {
                        let target = comp.data.borrow().target_chinese.clone();
                        let expr = comp.data.borrow().primary_english.clone();
                        if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                            let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                            vault.save_expression(&target, &expr, Some("Natural"), None);
                            let _ = vault.flush();
                        }
                        comp.set_saved_notice(Some("⭐ 已收藏到表达本".to_string()));
                        return LRESULT(0);
                    }
                    // 点击 [🚀 进入练习]
                    if cx >= COMPANION_WIDTH - 98 && cx <= COMPANION_WIDTH - 16 {
                        (comp.action_cb)(CandidateAction::StartDrillSession);
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
