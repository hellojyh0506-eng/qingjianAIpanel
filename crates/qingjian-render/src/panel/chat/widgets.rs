//! 面板里反复用到的小部件：类型别名、按钮、胶囊、省略截断。

use super::hit::ChatHit;
use super::style::CHIP_TEXT;
use crate::color::Color;
use crate::panel::{Area, Icon, PanelSurface, PanelText, tokens};

pub(super) type Surface<'a> = PanelSurface<'a, ChatHit>;

/// 无底的图标按钮：图标居中，点击区是整块 `area`。
pub(super) fn icon_button(
    s: &mut Surface<'_>,
    area: Area,
    icon: Icon,
    size: f32,
    color: Color,
    hit: ChatHit,
) {
    s.icon(
        icon,
        area.x + (area.width - size) / 2.0,
        area.y + (area.height - size) / 2.0,
        size,
        color,
    );
    s.hit(area, hit);
}

/// 文字超过 `max_width` 时截断并补「…」。
pub(super) fn fit_text(
    s: &mut Surface<'_>,
    text: &str,
    style: &PanelText,
    max_width: f32,
) -> String {
    if s.measure(text, style) <= max_width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if s.measure(&candidate, style) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    chars[..low].iter().collect::<String>() + "…"
}

/// 带描边的小胶囊（建议问题）：返回宽度。
pub(super) fn outlined_chip(s: &mut Surface<'_>, area: Area, text: &str, hit: Option<ChatHit>) {
    s.fill_bordered(
        area,
        tokens::RADIUS_BUTTON,
        super::style::CHIP_BG,
        super::style::SOFT_BORDER,
    );
    s.text(
        area.x + 12.0,
        area.y + (area.height - CHIP_TEXT.line_height) / 2.0,
        text,
        &CHIP_TEXT,
    );
    if let Some(hit) = hit {
        s.hit(area, hit);
    }
}
