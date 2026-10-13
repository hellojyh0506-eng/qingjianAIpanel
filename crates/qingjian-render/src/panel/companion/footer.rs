//! 底部固定按钮：收藏、练一句、问 AI；上面一条分割线。

use super::hit::CompanionHit;
use super::style::{BUTTON_HEIGHT, BUTTON_Y, FOOTER_Y, MARGIN_BAR, WIDTH};
use super::view::CompanionView;
use super::widgets::Surface;
use crate::color::Color;
use crate::panel::{Area, Icon, PanelText, tokens};

const GAP: f32 = 6.0;

/// 收藏后的星星颜色。
const STAR: Color = Color::hex(0xF59E0B);

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView) {
    s.hline(1.0, FOOTER_Y, WIDTH - 2.0, tokens::BORDER);
    let total = WIDTH - 2.0 * MARGIN_BAR - 2.0 * GAP;
    let save_w = 105.0;
    let practice_w = 121.0;
    let ask_w = total - save_w - practice_w;
    let save = Area::new(MARGIN_BAR, BUTTON_Y, save_w, BUTTON_HEIGHT);
    let practice = Area::new(save.right() + GAP, BUTTON_Y, practice_w, BUTTON_HEIGHT);
    let ask = Area::new(practice.right() + GAP, BUTTON_Y, ask_w, BUTTON_HEIGHT);

    let (save_label, save_color) = if view.saved {
        ("已收藏", STAR)
    } else {
        ("收藏", tokens::TEXT)
    };
    outlined(
        s,
        save,
        Icon::Star,
        save_label,
        save_color,
        CompanionHit::Save,
    );
    outlined(
        s,
        practice,
        Icon::Mic,
        "练一句",
        tokens::TEXT,
        CompanionHit::Practice,
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

fn outlined(
    s: &mut Surface<'_>,
    area: Area,
    icon: Icon,
    label: &str,
    color: Color,
    hit: CompanionHit,
) {
    s.fill_bordered(
        area,
        tokens::RADIUS_BUTTON,
        tokens::WINDOW_BG,
        tokens::BORDER,
    );
    centered(s, area, icon, label, color, false);
    s.hit(area, hit);
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
