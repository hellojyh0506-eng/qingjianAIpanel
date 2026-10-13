//! English Drill Session (独立练习与对比窗口)。
//! 专属于主动输出、主动回忆与 AI 对比评测的独立窗口，带独立编辑框、字数统计、Enter对比、AI反馈卡片。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DT_NOPREFIX, DT_WORDBREAK, DeleteObject, DrawTextW, GetTextExtentPoint32W, HDC, HFONT, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW, FW_BOLD, FW_NORMAL,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW,
    SW_HIDE, SW_SHOW, SendMessageW, ShowWindow, WM_CHAR, WM_LBUTTONDOWN,
    WM_NCHITTEST, WM_NCLBUTTONDOWN, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use super::layered::{self, Layered};
use super::monitor;
use super::window_class::WindowClass;
use crate::dispatch::CandidateAction;

const CLASS_NAME: PCWSTR = w!("QingjianDrillWindow");
static CLASS: WindowClass = WindowClass::new();

const DRILL_WIDTH: i32 = 460;
const DRILL_HEIGHT: i32 = 340;

/// 对比批改详细条目
#[derive(Debug, Clone)]
pub struct DrillFeedbackItem {
    pub label: String,
    pub explanation: String,
}

/// 对比批改结果数据
#[derive(Debug, Clone)]
pub struct DrillResultData {
    pub verdict: String,
    pub main_issue: String,
    pub better_expression: String,
    pub feedback_items: Vec<DrillFeedbackItem>,
    pub overall_rating: String,
    pub ielts_suggestion: Option<String>,
}

/// 练习会话窗口数据
#[derive(Debug, Clone)]
pub struct DrillSessionData {
    pub target_chinese: String,
    pub reference_expression: String,
    /// 挖空练习：英文句里被挖掉的短语换成 `____`；有它就让用户补空，而不是整句翻译。
    pub cloze_sentence: Option<String>,
    pub user_input: String,
    pub is_comparing: bool,
    pub show_answer: bool,
    pub result: Option<DrillResultData>,
    pub saved_notice: Option<String>,
    pub step: usize,
    pub total_steps: usize,
}

impl Default for DrillSessionData {
    fn default() -> Self {
        Self {
            target_chinese: "一点都没区别".to_string(),
            reference_expression: "There's no difference at all.".to_string(),
            cloze_sentence: None,
            user_input: String::new(),
            is_comparing: false,
            show_answer: false,
            result: None,
            saved_notice: None,
            step: 1,
            total_steps: 5,
        }
    }
}

pub struct DrillWindow {
    hwnd: HWND,
    data: RefCell<DrillSessionData>,
    last_anchor: Cell<Option<RECT>>,
    dark: Cell<bool>,
    action_cb: Box<dyn Fn(CandidateAction)>,
}

impl DrillWindow {
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
                w!("Qingjian English Drill"),
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
            data: RefCell::new(DrillSessionData::default()),
            last_anchor: Cell::new(None),
            dark: Cell::new(false),
            action_cb,
        });
        DRILL_WINDOWS.with(|d| d.borrow_mut().insert(hwnd.0 as isize, Rc::clone(&instance)));
        Ok(instance)
    }

    pub fn start_session(&self, target_chinese: &str, reference: &str, cloze: Option<String>, anchor: RECT) {
        let mut d = self.data.borrow_mut();
        d.target_chinese = target_chinese.to_string();
        d.reference_expression = reference.to_string();
        d.cloze_sentence = cloze;
        d.user_input.clear();
        d.is_comparing = false;
        d.show_answer = false;
        d.result = None;
        d.saved_notice = None;
        drop(d);
        let _ = self.show_at(anchor);
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
    }

    pub fn set_result(&self, result: DrillResultData) {
        let mut d = self.data.borrow_mut();
        d.is_comparing = false;
        d.result = Some(result);
        drop(d);
        if let Some(anchor) = self.last_anchor.get() {
            let _ = self.show_at(anchor);
        }
    }

    pub fn set_comparing(&self, comparing: bool) {
        self.data.borrow_mut().is_comparing = comparing;
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
        let content = (DRILL_WIDTH, DRILL_HEIGHT);

        // 居中或稍靠在候选窗右上方
        let (pos_x, pos_y) = calculate_drill_position(anchor, content, margin);

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
                    paint_drill(hdc, &data, client, self.dark.get());
                },
            },
        )?;
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOW) };
        Ok(())
    }
}

thread_local! {
    static DRILL_WINDOWS: RefCell<std::collections::HashMap<isize, Rc<DrillWindow>>> = RefCell::new(std::collections::HashMap::new());
}

fn calculate_drill_position(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(windows::Win32::Foundation::POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let right_x = anchor.right + 12;
    let y = anchor.top.clamp(work.top + margin, work.bottom - content.1 - margin);
    if right_x + content.0 + margin <= work.right {
        return (right_x, y);
    }
    // 居中显示
    let center_x = (work.left + work.right - content.0) / 2;
    let center_y = (work.top + work.bottom - content.1) / 2;
    (center_x, center_y)
}

fn paint_drill(hdc: HDC, data: &DrillSessionData, client: RECT, dark: bool) {
    unsafe { SetBkMode(hdc, TRANSPARENT) };

    let text_color = if dark { COLORREF(0x00E0E0E0) } else { COLORREF(0x001F1F1F) };
    let sub_color = if dark { COLORREF(0x00909090) } else { COLORREF(0x006B7280) };
    let brand_color = COLORREF(0x001673F9);
    let badge_bg = if dark { COLORREF(0x00333333) } else { COLORREF(0x00F3F4F6) };
    let green_color = COLORREF(0x000F8A5F);
    let red_color = COLORREF(0x002B37E6);

    let font_title = create_font(12, FW_NORMAL, false);
    let font_prompt = create_font(15, FW_BOLD, false);
    let font_body = create_font(13, FW_NORMAL, false);
    let font_tag = create_font(11, FW_NORMAL, false);
    let font_large = create_font(14, FW_BOLD, false);

    // 1. 顶部 Header
    draw_text_gdi(hdc, font_title, sub_color, 16, 12, "🇺🇸 English Drill · 练习模式");
    let step_str = format!("{}/{}", data.step, data.total_steps);
    draw_text_gdi(hdc, font_title, brand_color, client.right - 60, 12, &step_str);
    draw_text_gdi(hdc, font_title, sub_color, client.right - 24, 12, "×");

    let sep = RECT { left: 16, top: 32, right: client.right - 16, bottom: 33 };
    super::candidates::view::fill_rect(hdc, sep, if dark { COLORREF(0x00333333) } else { COLORREF(0x00E5E7EB) });

    if let Some(res) = &data.result {
        // ========== 对比结果反馈模式 ==========
        draw_text_gdi(hdc, font_tag, sub_color, 16, 42, "批改结果 (AI Feedback)：");

        // 你的表达
        let user_str = if data.user_input.is_empty() { "(未输入)" } else { &data.user_input };
        let is_good = res.verdict == "correct";
        let mark = if is_good { "✅ 你的表达：" } else { "❌ 你的表达：" };
        let mark_color = if is_good { green_color } else { red_color };
        draw_text_gdi(hdc, font_body, mark_color, 16, 62, mark);
        draw_text_gdi(hdc, font_body, text_color, 110, 62, user_str);

        // 更自然的表达
        draw_text_gdi(hdc, font_body, green_color, 16, 88, "✔ 更地道表达：");
        let nw = draw_text_gdi(hdc, font_large, green_color, 110, 88, &res.better_expression);
        draw_text_gdi(hdc, font_body, sub_color, 110 + nw + 10, 88, "🔊");

        // AI 详细反馈小卡片
        let fb_top = 118;
        let fb_rect = RECT { left: 16, top: fb_top, right: client.right - 16, bottom: fb_top + 100 };
        super::candidates::view::fill_round_rect(hdc, fb_rect, badge_bg, 8);

        draw_text_gdi(hdc, font_tag, brand_color, 24, fb_top + 8, "AI 详细反馈：");
        let mut fy = fb_top + 28;
        for item in res.feedback_items.iter().take(3) {
            let label_rect = RECT { left: 24, top: fy, right: 24 + 48, bottom: fy + 16 };
            super::candidates::view::fill_round_rect(hdc, label_rect, if dark { COLORREF(0x003A404D) } else { COLORREF(0x00E0E7FF) }, 3);
            draw_text_gdi(hdc, font_tag, brand_color, 32, fy + 1, &item.label);
            draw_text_gdi(hdc, font_tag, text_color, 80, fy + 1, &item.explanation);
            fy += 22;
        }

        if let Some(ielts) = &res.ielts_suggestion {
            draw_text_gdi(hdc, font_tag, green_color, 24, fy, &format!("雅思建议：{}", ielts));
        }

        // 总体评价胶囊
        let rate_rect = RECT { left: client.right - 160, top: 40, right: client.right - 16, bottom: 62 };
        super::candidates::view::fill_round_rect(hdc, rate_rect, if is_good { COLORREF(0x00D1FAE5) } else { COLORREF(0x00FEE2E2) }, 4);
        draw_text_gdi(hdc, font_tag, if is_good { green_color } else { red_color }, client.right - 150, 44, &res.overall_rating);

        // 底部按键：[🔄 再试一次] [⭐ 保存到表达本] [完成 (Enter)]
        let btn_y = client.bottom - 45;
        let r1 = RECT { left: 16, top: btn_y, right: 120, bottom: btn_y + 28 };
        super::candidates::view::fill_round_rect(hdc, r1, badge_bg, 4);
        draw_text_gdi(hdc, font_body, text_color, 26, btn_y + 5, "🔄 再试一次");

        let r2 = RECT { left: 130, top: btn_y, right: 260, bottom: btn_y + 28 };
        super::candidates::view::fill_round_rect(hdc, r2, badge_bg, 4);
        let save_str = if data.saved_notice.is_some() { "⭐ 已保存" } else { "⭐ 保存到表达本" };
        draw_text_gdi(hdc, font_body, text_color, 140, btn_y + 5, save_str);

        let r3 = RECT { left: client.right - 116, top: btn_y, right: client.right - 16, bottom: btn_y + 28 };
        super::candidates::view::fill_round_rect(hdc, r3, brand_color, 4);
        draw_text_gdi(hdc, font_body, COLORREF(0x00FFFFFF), client.right - 104, btn_y + 5, "完成 (Enter)");
    } else {
        // ========== 题目主动输入模式 ==========
        let box_top = if data.cloze_sentence.is_some() { 146 } else { 100 };
        if let Some(cloze) = &data.cloze_sentence {
            draw_text_gdi(hdc, font_body, sub_color, 16, 42, "补全句子里的空缺：");
            draw_text_gdi(hdc, font_prompt, text_color, 16, 64, &data.target_chinese);
            let rect = RECT { left: 16, top: 92, right: client.right - 16, bottom: box_top - 6 };
            draw_wrapped_gdi(hdc, font_large, brand_color, rect, cloze);
        } else {
            draw_text_gdi(hdc, font_body, sub_color, 16, 42, "请用英文表达：");
            draw_text_gdi(hdc, font_prompt, text_color, 16, 64, &data.target_chinese);
        }

        // 用户输入框
        let box_rect = RECT { left: 16, top: box_top, right: client.right - 16, bottom: 200 };
        super::candidates::view::fill_round_rect(hdc, box_rect, badge_bg, 8);

        let text_top = box_top + 12;
        if data.user_input.is_empty() {
            let hint = if data.cloze_sentence.is_some() { "输入空缺处的英文……" } else { "输入你的英文……（直接打字）" };
            draw_text_gdi(hdc, font_body, sub_color, 26, text_top, hint);
        } else {
            let input_with_cursor = format!("{}|", data.user_input);
            draw_text_gdi(hdc, font_body, text_color, 26, text_top, &input_with_cursor);
        }

        // 字数统计
        let count_str = format!("{}/200", data.user_input.len());
        draw_text_gdi(hdc, font_tag, sub_color, client.right - 70, 180, &count_str);

        // 如果开启了“显示答案”
        if data.show_answer {
            let ans_rect = RECT { left: 16, top: 208, right: client.right - 16, bottom: 232 };
            super::candidates::view::fill_round_rect(hdc, ans_rect, if dark { COLORREF(0x003A404D) } else { COLORREF(0x00E0E7FF) }, 4);
            draw_text_gdi(hdc, font_tag, brand_color, 24, 212, &format!("参考答案：{}", data.reference_expression));
        } else {
            // 提示行
            draw_text_gdi(hdc, font_tag, sub_color, 16, 212, "💡 提示：尝试使用自然的口语表达，不需要追求复杂词汇。");
        }

        // 底部按键：[对比 (Enter)] [显示答案] [跳过]
        let btn_y = client.bottom - 45;
        let r_compare = RECT { left: 16, top: btn_y, right: 120, bottom: btn_y + 28 };
        let comp_bg = if data.is_comparing { COLORREF(0x00909090) } else { brand_color };
        super::candidates::view::fill_round_rect(hdc, r_compare, comp_bg, 4);
        let comp_str = if data.is_comparing { "对比中..." } else { "对比 (Enter)" };
        draw_text_gdi(hdc, font_body, COLORREF(0x00FFFFFF), 26, btn_y + 5, comp_str);

        let r_show = RECT { left: 130, top: btn_y, right: 220, bottom: btn_y + 28 };
        super::candidates::view::fill_round_rect(hdc, r_show, badge_bg, 4);
        draw_text_gdi(hdc, font_body, text_color, 142, btn_y + 5, "显示答案");

        let r_skip = RECT { left: 230, top: btn_y, right: 290, bottom: btn_y + 28 };
        super::candidates::view::fill_round_rect(hdc, r_skip, badge_bg, 4);
        draw_text_gdi(hdc, font_body, sub_color, 246, btn_y + 5, "跳过");

        draw_text_gdi(hdc, font_tag, sub_color, client.right - 70, btn_y + 8, "Esc 退出");
    }

    unsafe {
        let _ = DeleteObject(font_title.into());
        let _ = DeleteObject(font_prompt.into());
        let _ = DeleteObject(font_body.into());
        let _ = DeleteObject(font_tag.into());
        let _ = DeleteObject(font_large.into());
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

/// 在 `rect` 里自动换行画文字，放不下的部分被裁掉。
fn draw_wrapped_gdi(hdc: HDC, font: HFONT, color: COLORREF, rect: RECT, text: &str) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rect = rect;
    unsafe {
        let old = SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        DrawTextW(hdc, &mut wide, &mut rect, DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(hdc, old);
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
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_CHAR => {
            let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
            let window = DRILL_WINDOWS.with(|d| d.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = window {
                let mut d = win.data.borrow_mut();
                if d.result.is_none() {
                    if ch == '\x08' {
                        // Backspace
                        d.user_input.pop();
                    } else if ch == '\r' || ch == '\n' {
                        // Enter -> 对比
                        let input = d.user_input.trim().to_string();
                        if !input.is_empty() {
                            d.is_comparing = true;
                            let prompt = d.target_chinese.clone();
                            let reference = d.reference_expression.clone();
                            drop(d);
                            (win.action_cb)(CandidateAction::DrillCompare(prompt, input, reference));
                        }
                        return LRESULT(0);
                    } else if ch == '\t' {
                        // Tab -> 显示答案
                        d.show_answer = true;
                    } else if ch == '\x1b' {
                        // Esc -> 退出
                        drop(d);
                        win.hide();
                        return LRESULT(0);
                    } else if !ch.is_control() && d.user_input.len() < 200 {
                        d.user_input.push(ch);
                    }
                    drop(d);
                    if let Some(anchor) = win.last_anchor.get() {
                        let _ = win.show_at(anchor);
                    }
                } else {
                    // 结果模式下
                    if ch == '\r' || ch == '\x1b' {
                        drop(d);
                        win.hide();
                        return LRESULT(0);
                    } else if ch == 'r' || ch == 'R' {
                        // 再试一次
                        d.result = None;
                        d.user_input.clear();
                        drop(d);
                        if let Some(anchor) = win.last_anchor.get() {
                            let _ = win.show_at(anchor);
                        }
                    } else if ch == 's' || ch == 'S' {
                        // 收藏
                        let target = d.target_chinese.clone();
                        let expr = d.result.as_ref().map(|r| r.better_expression.clone()).unwrap_or_default();
                        if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                            let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                            vault.save_expression(&target, &expr, Some("Natural"), None);
                            let _ = vault.flush();
                        }
                        d.saved_notice = Some("⭐ 已保存到表达本".to_string());
                        drop(d);
                        if let Some(anchor) = win.last_anchor.get() {
                            let _ = win.show_at(anchor);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let window = DRILL_WINDOWS.with(|d| d.borrow().get(&(hwnd.0 as isize)).cloned());
            if let Some(win) = window {
                let margin = layered::shadow_margin(unsafe { GetDpiForWindow(hwnd) }.max(96));
                let cx = x - margin;
                let cy = y - margin;

                // 关闭按钮
                if cx >= DRILL_WIDTH - 36 && cx <= DRILL_WIDTH && cy <= 30 {
                    win.hide();
                    return LRESULT(0);
                }

                // 标题栏拖动
                if cy <= 30 {
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

                let mut d = win.data.borrow_mut();
                if d.result.is_none() {
                    let btn_y = DRILL_HEIGHT - 45;
                    if cy >= btn_y && cy <= btn_y + 28 {
                        // 对比
                        if cx >= 16 && cx <= 120 {
                            let input = d.user_input.trim().to_string();
                            if !input.is_empty() {
                                d.is_comparing = true;
                                let prompt = d.target_chinese.clone();
                                let reference = d.reference_expression.clone();
                                drop(d);
                                (win.action_cb)(CandidateAction::DrillCompare(prompt, input, reference));
                            }
                            return LRESULT(0);
                        }
                        // 显示答案
                        if cx >= 130 && cx <= 220 {
                            d.show_answer = true;
                            drop(d);
                            if let Some(anchor) = win.last_anchor.get() {
                                let _ = win.show_at(anchor);
                            }
                            return LRESULT(0);
                        }
                        // 跳过
                        if cx >= 230 && cx <= 290 {
                            drop(d);
                            win.hide();
                            return LRESULT(0);
                        }
                    }
                } else {
                    let btn_y = DRILL_HEIGHT - 45;
                    if cy >= btn_y && cy <= btn_y + 28 {
                        // 再试一次
                        if cx >= 16 && cx <= 120 {
                            d.result = None;
                            d.user_input.clear();
                            drop(d);
                            if let Some(anchor) = win.last_anchor.get() {
                                let _ = win.show_at(anchor);
                            }
                            return LRESULT(0);
                        }
                        // 收藏
                        if cx >= 130 && cx <= 260 {
                            let target = d.target_chinese.clone();
                            let expr = d.result.as_ref().map(|r| r.better_expression.clone()).unwrap_or_default();
                            if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                                let mut vault = qingjian_learning::LearningVault::open(&user_dir);
                                vault.save_expression(&target, &expr, Some("Natural"), None);
                                let _ = vault.flush();
                            }
                            d.saved_notice = Some("⭐ 已保存到表达本".to_string());
                            drop(d);
                            if let Some(anchor) = win.last_anchor.get() {
                                let _ = win.show_at(anchor);
                            }
                            return LRESULT(0);
                        }
                        // 完成
                        if cx >= DRILL_WIDTH - 116 && cx <= DRILL_WIDTH - 16 {
                            drop(d);
                            win.hide();
                            return LRESULT(0);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
