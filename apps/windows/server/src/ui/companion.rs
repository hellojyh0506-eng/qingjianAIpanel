//! English Assistant (学习助手伴侣窗)。
//! 独立的轻量 Companion Panel，WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE，
//! 支持鼠标拖动、记忆位置、动态内容自适应尺寸（AutoFit）、高 DPI 多屏适配。
//! 包含：当前候选表达、更多地道口语、用法与高频搭配、场景示例、收藏到表达本与直达练习。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetTextExtentPoint32W, HDC, HFONT, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW, FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowRect, HTCAPTION, HTCLIENT, IDC_ARROW,
    LoadCursorW, MA_NOACTIVATE, SW_HIDE, SW_SHOWNA, SendMessageW, ShowWindow,
    WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOVE, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use super::layered::{self, Layered};
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianCompanionWindow");
static CLASS: WindowClass = WindowClass::new();

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
    pub is_loading: bool,
}

impl CompanionData {
    pub fn loading(target_chinese: String, primary_english: String) -> Self {
        Self {
            target_chinese,
            primary_english,
            phonetic: String::new(),
            explanation: String::new(),
            active_tab: 0,
            alternatives: Vec::new(),
            collocations: Vec::new(),
            examples: Vec::new(),
            ielts_friendly: String::new(),
            saved_notice: None,
            is_loading: true,
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
            active_tab: 0,
            alternatives: Vec::new(),
            collocations: Vec::new(),
            examples: Vec::new(),
            ielts_friendly: String::new(),
            saved_notice: None,
            is_loading: false,
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
    dark: Cell<bool>,
    pub is_pinned: Cell<bool>,
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
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
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
            current_width: Cell::new(420),
            current_height: Cell::new(280),
            dark: Cell::new(false),
            is_pinned: Cell::new(false),
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
        let scale = dpi as f32 / 96.0;
        let margin = layered::shadow_margin(dpi);

        // 动态计算 AutoFit 尺寸 (Logical px 经过 DPI 缩放)
        let logical_width = 430;
        let logical_height = {
            let d = self.data.borrow();
            if d.is_loading {
                230
            } else {
                match d.active_tab {
                    0 => {
                        // 更多译法：每条约 44px + 头部与底栏
                        let items_h = (d.alternatives.len().clamp(2, 5) as i32) * 44;
                        (170 + items_h).clamp(300, 440)
                    }
                    1 => 360, // 用法说明
                    2 => 320, // 相关搭配
                    3 => 300, // 场景示例
                    _ => 300,
                }
            }
        };

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
                    paint_companion(hdc, &data, client, self.dark.get(), scale, self.is_pinned.get());
                },
            },
        )?;
        let is_pinned = self.is_pinned.get();
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
    // 1. 优先尝试放在 Candidate Window 右侧相距 8px
    let right_x = anchor.right + 8;
    if right_x + content.0 + margin <= work.right {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (right_x, y);
    }
    // 2. 尝试放在 Candidate Window 下方相距 8px
    let bottom_y = anchor.bottom + 8;
    if bottom_y + content.1 + margin <= work.bottom {
        let x = anchor.left.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
        return (x, bottom_y);
    }
    // 3. 尝试放在 Candidate Window 上方相距 8px
    let top_y = anchor.top - content.1 - 8;
    if top_y >= work.top + margin {
        let x = anchor.left.clamp(work.left + margin, (work.right - content.0 - margin).max(work.left));
        return (x, top_y);
    }
    // 4. 尝试放在 Candidate Window 左侧相距 8px
    let left_x = anchor.left - content.0 - 8;
    if left_x >= work.left + margin {
        let y = anchor.top.clamp(work.top + margin, (work.bottom - content.1 - margin).max(work.top));
        return (left_x, y);
    }
    // 5. 最终兜底：在屏幕右下角安全展示，避免居中挡住输入焦点
    let x = (work.right - content.0 - margin - 20).max(work.left + margin);
    let y = (work.bottom - content.1 - margin - 60).max(work.top + margin);
    (x, y)
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

fn paint_companion(hdc: HDC, data: &CompanionData, client: RECT, dark: bool, scale: f32, is_pinned: bool) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };

    let px = |val: i32| (val as f32 * scale).round() as i32;

    let text_color = if dark { rgb(224, 224, 224) } else { rgb(31, 31, 31) };
    let sub_color = if dark { rgb(144, 144, 144) } else { rgb(107, 114, 128) };
    let brand_color = rgb(26, 115, 232); // Google Blue #1A73E8
    let badge_bg = if dark { rgb(51, 51, 51) } else { rgb(243, 244, 246) };
    let active_bg = if dark { rgb(58, 64, 77) } else { rgb(232, 240, 254) };
    let green_color = rgb(19, 115, 51);

    let font_title = create_font(px(12), FW_NORMAL, false);
    let font_target = create_font(px(13), FW_NORMAL, false);
    let font_primary = create_font(px(15), FW_BOLD, false);
    let font_tab = create_font(px(12), FW_NORMAL, false);
    let font_body = create_font(px(12), FW_NORMAL, false);
    let font_tag = create_font(px(11), FW_NORMAL, false);

    // 1. 顶部标题栏 (可拖动区域)
    let title_text = match data.active_tab {
        0 => "English Assistant · 更多译法",
        1 => "English Assistant · 用法说明",
        2 => "English Assistant · 相关搭配",
        3 => "English Assistant · 场景示例",
        _ => "English Assistant · 学习助手",
    };
    draw_text_gdi(hdc, font_title, sub_color, px(16), px(10), title_text);
    draw_text_gdi(hdc, font_title, if is_pinned { brand_color } else { sub_color }, client.right - px(48), px(10), "📌");
    draw_text_gdi(hdc, font_title, sub_color, client.right - px(22), px(10), "×");

    // 2. 焦点表达信息
    draw_text_gdi(hdc, font_target, text_color, px(16), px(34), &data.target_chinese);
    let prim_text = if data.primary_english.is_empty() {
        "正在分析当前表达……"
    } else {
        &data.primary_english
    };
    let prim_w = draw_text_gdi(hdc, font_primary, brand_color, px(16), px(54), prim_text);
    if !data.primary_english.is_empty() && !data.is_loading {
        draw_text_gdi(hdc, font_target, sub_color, px(16) + prim_w + px(8), px(56), "🔊");
    }

    // 分隔线
    let sep = RECT { left: px(16), top: px(80), right: client.right - px(16), bottom: px(81) };
    super::candidates::view::fill_rect(hdc, sep, if dark { rgb(51, 51, 51) } else { rgb(229, 231, 235) });

    if data.is_loading && data.alternatives.is_empty() {
        // 加载中轻量提示
        let load_box = RECT { left: px(16), top: px(96), right: client.right - px(16), bottom: px(160) };
        super::candidates::view::fill_round_rect(hdc, load_box, badge_bg, 6);
        draw_text_gdi(hdc, font_body, brand_color, px(32), px(118), "⏳ 正在分析当前表达并生成地道美式变体与用法……");
    } else {
        // 3. Tab 栏 (完全舒展排列 4 个选项卡，绝不挤压截断)
        let tabs = ["更多译法", "用法说明", "相关搭配", "场景示例"];
        let mut tab_x = px(16);
        let tab_w = px(76);
        let tab_h = px(26);
        let gap = px(8);
        for (i, name) in tabs.iter().enumerate() {
            let is_active = data.active_tab == i;
            let tab_rect = RECT { left: tab_x, top: px(88), right: tab_x + tab_w, bottom: px(88) + tab_h };
            if is_active {
                super::candidates::view::fill_round_rect(hdc, tab_rect, active_bg, 4);
                draw_text_gdi(hdc, font_tab, brand_color, tab_x + px(12), px(93), name);
            } else {
                draw_text_gdi(hdc, font_tab, sub_color, tab_x + px(12), px(93), name);
            }
            tab_x += tab_w + gap;
        }

        // 4. 内容详情展示区 (y >= 122)
        let content_y = px(122);
        match data.active_tab {
            0 => {
                // 更多译法 (Panel 4)
                let mut y = content_y;
                for (idx, alt) in data.alternatives.iter().take(5).enumerate() {
                    let idx_str = format!("{}.", idx + 1);
                    draw_text_gdi(hdc, font_body, sub_color, px(16), y, &idx_str);
                    let w = draw_text_gdi(hdc, font_body, text_color, px(32), y, &alt.text);

                    // Register 胶囊 (Direct / Nuanced / Idiomatic / Natural / Casual / Professional / Stronger / Neutral)
                    let (badge_bg_col, badge_text_col) = match alt.tag.as_str() {
                        "Direct" | "直接" => (rgb(224, 242, 254), rgb(2, 132, 199)),
                        "Nuanced" | "微殊" => (rgb(237, 233, 254), rgb(124, 58, 237)),
                        "Idiomatic" | "习语" => (rgb(254, 243, 199), rgb(217, 119, 6)),
                        "Casual" | "口语" => (rgb(254, 247, 224), rgb(176, 96, 0)),
                        "Natural" | "地道" => (rgb(230, 244, 234), rgb(19, 115, 51)),
                        "Neutral" | "中性" => (rgb(232, 240, 254), rgb(26, 115, 232)),
                        "Stronger" | "强调" => (rgb(243, 232, 253), rgb(123, 31, 162)),
                        _ => (active_bg, brand_color),
                    };
                    let tag_w = (alt.tag.len() as i32 * px(7) + px(12)).max(px(52));
                    let tag_rect = RECT { left: px(32) + w + px(8), top: y - px(1), right: px(32) + w + px(8) + tag_w, bottom: y + px(15) };
                    super::candidates::view::fill_round_rect(hdc, tag_rect, badge_bg_col, 3);
                    draw_text_gdi(hdc, font_tag, badge_text_col, px(32) + w + px(14), y, &alt.tag);

                    draw_text_gdi(hdc, font_tag, sub_color, client.right - px(32), y, "🔊");

                    // 解释/地道原因行
                    let desc = if !alt.why_natural.is_empty() {
                        &alt.why_natural
                    } else if !alt.meaning.is_empty() {
                        &alt.meaning
                    } else {
                        "地道美式口语表达"
                    };
                    draw_text_gdi(hdc, font_tag, sub_color, px(32), y + px(18), desc);

                    y += px(42);
                }
            }
            1 => {
                // 用法说明 (Panel 5)
                let mut y = content_y;
                let phonetic_str = if !data.phonetic.is_empty() {
                    format!("/ {} /", data.phonetic.trim_matches('/'))
                } else {
                    String::new()
                };
                let title = if phonetic_str.is_empty() {
                    format!("{}   [高频地道表达]", data.primary_english)
                } else {
                    format!("{}   {}   [高频搭配]", data.primary_english, phonetic_str)
                };
                draw_text_gdi(hdc, font_body, brand_color, px(16), y, &title);
                y += px(22);

                // 中文释义 (自动多行换行保护，绝不裁剪截断)
                let expl = if !data.explanation.is_empty() {
                    &data.explanation
                } else {
                    "日常交流与口语沟通中最自然、最符合语境的高频地道表达。"
                };
                let h = draw_wrapped_text_gdi(hdc, font_tag, text_color, px(16), y, client.right - px(32), px(18), expl);
                y += h + px(8);

                // 常见搭配 pills
                draw_text_gdi(hdc, font_tag, sub_color, px(16), y, "常见搭配：");
                y += px(18);
                let mut cx = px(16);
                for col in &data.collocations {
                    let col_w = (col.len() as i32 * px(7)) + px(16);
                    if cx + col_w > client.right - px(16) {
                        cx = px(16);
                        y += px(22);
                    }
                    let rect = RECT { left: cx, top: y, right: cx + col_w, bottom: y + px(18) };
                    super::candidates::view::fill_round_rect(hdc, rect, badge_bg, 4);
                    draw_text_gdi(hdc, font_tag, brand_color, cx + px(8), y + px(1), col);
                    cx += col_w + px(6);
                }
                y += px(24);

                // 例句 (双行清晰展示)
                draw_text_gdi(hdc, font_tag, sub_color, px(16), y, "例句：");
                y += px(18);
                for (ex_en, ex_zh) in data.examples.iter().take(2) {
                    draw_text_gdi(hdc, font_tag, text_color, px(24), y, &format!("• {}", ex_en));
                    y += px(16);
                    if !ex_zh.is_empty() {
                        draw_text_gdi(hdc, font_tag, sub_color, px(32), y, ex_zh);
                        y += px(18);
                    }
                }

                // 雅思高分替换
                if !data.ielts_friendly.is_empty() {
                    draw_text_gdi(hdc, font_tag, green_color, px(16), y, &format!("雅思替换：{}", data.ielts_friendly));
                }
            }
            2 => {
                // 相关搭配 (Panel 6)
                let mut y = content_y;
                draw_text_gdi(hdc, font_body, brand_color, px(16), y, "常用搭配与同义表达 (Collocations & Synonyms)：");
                y += px(22);

                if data.collocations.is_empty() {
                    draw_text_gdi(hdc, font_tag, sub_color, px(16), y, "💡 暂无专属固定搭配，该表达常作为独立口语单句使用。");
                    y += px(22);
                } else {
                    let mut col_x = px(16);
                    for col in &data.collocations {
                        let col_w = (col.len() as i32 * px(7)) + px(16);
                        if col_x + col_w > client.right - px(16) {
                            col_x = px(16);
                            y += px(22);
                        }
                        let rect = RECT { left: col_x, top: y, right: col_x + col_w, bottom: y + px(18) };
                        super::candidates::view::fill_round_rect(hdc, rect, badge_bg, 4);
                        draw_text_gdi(hdc, font_tag, brand_color, col_x + px(8), y + px(1), col);
                        col_x += col_w + px(6);
                    }
                    y += px(28);
                }

                if !data.alternatives.is_empty() {
                    let card_rect = RECT { left: px(16), top: y, right: client.right - px(16), bottom: y + px(85) };
                    super::candidates::view::fill_round_rect(hdc, card_rect, badge_bg, 6);
                    draw_text_gdi(hdc, font_tag, brand_color, px(24), y + px(8), "语域风格差异对照 (Register Comparison)：");
                    let mut ry = y + px(28);
                    for alt in data.alternatives.iter().take(2) {
                        draw_text_gdi(hdc, font_tag, text_color, px(24), ry, &format!("• [{}] {}: {}", alt.tag, alt.text, alt.meaning));
                        ry += px(20);
                    }
                    if !data.ielts_friendly.is_empty() {
                        draw_text_gdi(hdc, font_tag, green_color, px(24), ry, &format!("• [IELTS 7+] {}", data.ielts_friendly));
                    }
                }
            }
            3 => {
                // 场景示例
                let mut y = content_y;
                draw_text_gdi(hdc, font_body, text_color, px(16), y, "真实对话与语境示例 (Dialogue Scenario)：");
                y += px(24);
                if data.examples.is_empty() {
                    draw_text_gdi(hdc, font_tag, sub_color, px(16), y, "💬 正在生成场景例句，可按 Ctrl+Alt+D 在 AI Assistant 中深入讨论。");
                } else {
                    for (ex_en, ex_zh) in data.examples.iter().take(3) {
                        draw_text_gdi(hdc, font_tag, text_color, px(16), y, &format!("• {}", ex_en));
                        y += px(18);
                        if !ex_zh.is_empty() {
                            draw_text_gdi(hdc, font_tag, sub_color, px(24), y, ex_zh);
                            y += px(20);
                        }
                    }
                    if !data.ielts_friendly.is_empty() {
                        draw_text_gdi(hdc, font_tag, green_color, px(16), y, &format!("IELTS Speaking 进阶: {}", data.ielts_friendly));
                    }
                }
            }
            _ => {}
        }

        // 5. 底部操作栏 (收藏与 AI 追问直达)
        let footer_sep = RECT {
            left: px(16),
            top: client.bottom - px(38),
            right: client.right - px(16),
            bottom: client.bottom - px(37),
        };
        super::candidates::view::fill_rect(hdc, footer_sep, if dark { rgb(51, 51, 51) } else { rgb(229, 231, 235) });

        let footer_y = client.bottom - px(32);
        // 左侧：收藏到表达本
        let fav_text = if let Some(notice) = &data.saved_notice {
            notice.as_str()
        } else {
            "⭐ 收藏到表达本"
        };
        let fav_box = RECT {
            left: px(16),
            top: footer_y - px(2),
            right: px(16) + px(128),
            bottom: footer_y + px(24),
        };
        super::candidates::view::fill_round_rect(hdc, fav_box, badge_bg, 4);
        draw_text_gdi(hdc, font_tag, if data.saved_notice.is_some() { brand_color } else { text_color }, px(24), footer_y + px(3), fav_text);

        // 右侧：AI 深度追问 (Ctrl+Alt+D)
        let ai_btn = RECT {
            left: client.right - px(178),
            top: footer_y - px(2),
            right: client.right - px(16),
            bottom: footer_y + px(24),
        };
        super::candidates::view::fill_round_rect(hdc, ai_btn, active_bg, 4);
        draw_text_gdi(hdc, font_tag, brand_color, client.right - px(168), footer_y + px(3), "💬 AI 深度追问 (Ctrl+Alt+D)");
    }

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

/// 自动换行文本测量与绘制：完全杜绝文字被截断或溢出
fn draw_wrapped_text_gdi(
    hdc: HDC,
    font: HFONT,
    color: COLORREF,
    left: i32,
    mut y: i32,
    max_w: i32,
    line_h: i32,
    text: &str,
) -> i32 {
    let mut current_line = String::new();
    let mut lines = 0;
    unsafe {
        SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
    }
    for ch in text.chars() {
        current_line.push(ch);
        let utf16: Vec<u16> = current_line.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
        }
        if size.cx > max_w && !current_line.is_empty() {
            current_line.pop();
            let line_utf16: Vec<u16> = current_line.encode_utf16().collect();
            unsafe {
                let _ = TextOutW(hdc, left, y, &line_utf16);
            }
            y += line_h;
            lines += 1;
            current_line.clear();
            current_line.push(ch);
        }
    }
    if !current_line.is_empty() {
        let line_utf16: Vec<u16> = current_line.encode_utf16().collect();
        unsafe {
            let _ = TextOutW(hdc, left, y, &line_utf16);
        }
        lines += 1;
    }
    lines * line_h
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            // 纯阅读/学习，绝对不抢焦点，不激活窗口，防止破坏宿主程序焦点与拼音
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

                // 1. 关闭按钮 (右上角)
                if cx >= w - px(30) && cx <= w && cy <= px(32) {
                    comp.hide();
                    return LRESULT(0);
                }

                // 2. 置顶图钉 (cx in w - px(56) .. w - px(30))
                if cx >= w - px(56) && cx < w - px(30) && cy <= px(32) {
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

                // 3. 复制到剪贴板图标 (cx in prim_w..prim_w+40, cy in 45..75)
                if cy >= px(45) && cy <= px(75) && cx >= px(120) && cx <= px(360) {
                    let text = comp.data.borrow().primary_english.clone();
                    copy_to_clipboard(&text);
                    comp.set_saved_notice(Some("已复制到剪贴板".to_string()));
                    return LRESULT(0);
                }

                // 4. Tab 栏点击 (cy in 80..118)
                if cy >= px(80) && cy <= px(118) {
                    let tab_w = px(76);
                    let gap = px(8);
                    if cx >= px(16) && cx <= px(16) + (tab_w + gap) * 4 {
                        let tab_idx = ((cx - px(16)) / (tab_w + gap)) as usize;
                        if tab_idx < 4 {
                            comp.set_tab(tab_idx);
                            return LRESULT(0);
                        }
                    }
                }

                // 5. 底部操作栏点击 (靠近底边 40px)
                let h = comp.current_height.get();
                if cy >= h - px(40) && cy <= h {
                    // 点击左侧 [⭐ 收藏到表达本]
                    if cx >= px(16) && cx <= px(150) {
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
                    // 点击右侧 [💡 进入练习 (Ctrl+Alt+D)]
                    if cx >= w - px(176) && cx <= w - px(16) {
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
