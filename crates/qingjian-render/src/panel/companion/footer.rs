//! 底部固定按钮：问 AI；上面一条分割线。

use super::hit::CompanionHit;
use super::style::{BUTTON_HEIGHT, BUTTON_Y, FOOTER_Y, MARGIN_BAR, WIDTH};
use super::widgets::Surface;
use crate::color::Color;
use crate::panel::{Area, Icon, PanelText, tokens};

pub(super) fn draw(s: &mut Surface<'_>) {
    s.hline(1.0, FOOTER_Y, WIDTH - 2.0, tokens::BORDER);
    let ask = Area::new(
        MARGIN_BAR,
        BUTTON_Y,
        WIDTH - 2.0 * MARGIN_BAR,
        BUTTON_HEIGHT,
    );
    s.fill(ask, tokens::RADIUS_BUTTON, tokens::PRIMARY);
    centered(
        s,
        ask,
        Icon::MessageSquare,
        "问 AI",
        tokens::WINDOW_BG,
        true,
    );
    s.hit(ask, CompanionHit::AskAi);
}

/// 图标 + 文字整体在按钮里居中。
fn centered(s: &mut Surface<'_>, area: Area, icon: Icon, label: &str, color: Color, bold: bool) {
    let mut style = PanelText::new(13.0, 18.0, color);
    style.bold = bold;
    let text_width = s.measure(label, &style);
    let left = area.x + (area.width - text_width - 22.0) / 2.0;
    s.icon(icon, left, area.y + (area.height - 16.0) / 2.0, 16.0, color);
    s.text(
        left + 22.0,
        area.y + (area.height - 18.0) / 2.0,
        label,
        &style,
    );
}
