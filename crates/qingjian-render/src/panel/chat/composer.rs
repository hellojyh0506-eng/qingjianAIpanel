//! 底部输入区：多行输入框（带联网 / 深度开关）与发送键，并算出输入光标的位置。

use std::collections::HashMap;

use super::hit::ChatHit;
use super::input_wrap::{Line, caret_line, wrap};
use super::phase::Phase;
use super::style::{
    COMPOSER_BOTTOM, COMPOSER_SIDE, INPUT_LINE_HEIGHT, INPUT_MAX_LINES, INPUT_MIN_HEIGHT,
    INPUT_TEXT, SEND_SIZE, SIDEBAR_WIDTH, SOFT_BORDER,
};
use super::view::ChatView;
use super::widgets::Surface;
use crate::color::Color;
use crate::panel::{Area, Icon, tokens};

const PLACEHOLDER: &str = "输入你的问题……（Enter 发送，Shift+Enter 换行）";
const PLACEHOLDER_COLOR: Color = Color::hex(0x9CA3AF);

/// 输入框与发送键的间距、文字左内边距、右侧开关按钮的边长。
const SEND_GAP: f32 = 5.0;
const TEXT_PAD: f32 = 14.0;
const TOGGLE: f32 = 26.0;

/// 自绘光标的宽度（点）。
const CARET_WIDTH: f32 = 1.5;

/// 输入框上下内边距：单行时刚好凑出最小高度。
const PAD_Y: f32 = (INPUT_MIN_HEIGHT - INPUT_LINE_HEIGHT) / 2.0;

pub(super) struct Composer {
    pub input: Area,

    pub send: Area,

    /// 显示用的全文：已确定文字，光标处插入输入法组字。
    text: String,

    /// 光标在 `text` 里的字节位置（含组字）。
    cursor: usize,

    lines: Vec<Line>,

    /// 显示哪几行（半开区间）：行数超过上限时跟着光标。
    shown: (usize, usize),
}

/// 量输入框：折行、决定高度与位置。
pub(super) fn layout(s: &mut Surface<'_>, view: &ChatView) -> Composer {
    let mut text = String::with_capacity(view.input.len() + view.composition.len());
    let cursor = view.cursor.min(view.input.len());
    text.push_str(&view.input[..cursor]);
    text.push_str(&view.composition);
    text.push_str(&view.input[cursor..]);
    let cursor = cursor + view.composition.len();

    let send = Area::new(
        view.width - COMPOSER_SIDE - SEND_SIZE,
        view.height - COMPOSER_BOTTOM - SEND_SIZE,
        SEND_SIZE,
        SEND_SIZE,
    );
    let left = SIDEBAR_WIDTH + COMPOSER_SIDE;
    let width = send.x - SEND_GAP - left;
    let text_width = width - TEXT_PAD - 2.0 * TOGGLE - 8.0;

    let mut widths: HashMap<char, f32> = HashMap::new();
    let lines = wrap(&text, text_width, |ch| {
        *widths
            .entry(ch)
            .or_insert_with(|| s.measure(ch.encode_utf8(&mut [0; 4]), &INPUT_TEXT))
    });
    let visible = lines.len().min(INPUT_MAX_LINES);
    let first = (caret_line(&lines, cursor) + 1).saturating_sub(visible);
    let height = (2.0 * PAD_Y + visible as f32 * INPUT_LINE_HEIGHT).max(INPUT_MIN_HEIGHT);
    Composer {
        input: Area::new(left, view.height - COMPOSER_BOTTOM - height, width, height),
        send,
        text,
        cursor,
        lines,
        shown: (first, first + visible),
    }
}

/// 画输入区，返回输入光标 `(x, y, 高)`（窗口坐标，点）。
pub(super) fn draw(s: &mut Surface<'_>, view: &ChatView, composer: &Composer) -> (f32, f32, f32) {
    let input = composer.input;
    s.fill_bordered(input, 12.0, tokens::WINDOW_BG, SOFT_BORDER);
    s.hit(input, ChatHit::Input);

    let text_x = input.x + TEXT_PAD;
    let top = input.y + PAD_Y;
    if composer.text.is_empty() {
        s.text(
            text_x,
            top,
            PLACEHOLDER,
            &INPUT_TEXT.colored(PLACEHOLDER_COLOR),
        );
    }
    let (first, end) = composer.shown;
    let mut caret = (text_x, top, INPUT_LINE_HEIGHT - 2.0);
    let caret_row = caret_line(&composer.lines, composer.cursor);
    for (row, line) in composer.lines[first..end].iter().enumerate() {
        let y = top + row as f32 * INPUT_LINE_HEIGHT;
        let content = &composer.text[line.start..line.end];
        s.text(text_x, y, content, &INPUT_TEXT);
        if first + row == caret_row {
            let prefix = &composer.text[line.start..composer.cursor.clamp(line.start, line.end)];
            let x = text_x + s.measure(prefix, &INPUT_TEXT);
            caret = (x, y + 1.0, INPUT_LINE_HEIGHT - 2.0);
        }
    }

    if view.show_caret {
        let (x, y, height) = caret;
        s.fill(Area::new(x, y, CARET_WIDTH, height), 0.75, tokens::TEXT);
    }

    let toggle_y = input.bottom() - (INPUT_MIN_HEIGHT - TOGGLE) / 2.0 - TOGGLE;
    let deep = Area::new(input.right() - 4.0 - TOGGLE, toggle_y, TOGGLE, TOGGLE);
    let web = Area::new(deep.x - TOGGLE, toggle_y, TOGGLE, TOGGLE);
    toggle(s, web, Icon::Globe, view.web, ChatHit::ToggleWeb);
    toggle(s, deep, Icon::Brain, view.deep, ChatHit::ToggleDeep);

    send_button(s, view, composer.send);
    caret
}

fn toggle(s: &mut Surface<'_>, area: Area, icon: Icon, on: bool, hit: ChatHit) {
    if on {
        s.fill(area.inset(1.0), tokens::RADIUS_BUTTON, tokens::SELECTED_BG);
    }
    let color = if on { tokens::PRIMARY } else { tokens::TEXT_2 };
    s.icon(icon, area.x + 5.0, area.y + 5.0, 16.0, color);
    s.hit(area, hit);
}

fn send_button(s: &mut Surface<'_>, view: &ChatView, area: Area) {
    let busy = view.phase != Phase::Idle;
    let ready = !view.input.trim().is_empty() || !view.composition.is_empty();
    s.fill(area, 10.0, tokens::PRIMARY);
    let (icon, size) = if busy {
        (Icon::Square, 14.0)
    } else {
        (Icon::Send, 17.0)
    };
    s.icon(
        icon,
        area.x + (area.width - size) / 2.0,
        area.y + (area.height - size) / 2.0,
        size,
        tokens::WINDOW_BG,
    );
    if busy {
        s.hit(area, ChatHit::Stop);
    } else if ready {
        s.hit(area, ChatHit::Send);
    }
}
