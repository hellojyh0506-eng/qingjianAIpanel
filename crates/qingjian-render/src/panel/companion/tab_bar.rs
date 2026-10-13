//! 三个标签页的分段条：选中的有底色与描边，「更自然地说」选中时用青绿。

use super::hit::CompanionHit;
use super::style::{MARGIN_BAR, TAB_BAR_HEIGHT, TAB_BAR_Y, TEAL, TEAL_BG, TEXT_MID, WIDTH};
use super::view::CompanionView;
use super::widgets::Surface;
use crate::color::Color;
use crate::panel::{Area, Icon, PanelText, tokens};

const TABS: [(&str, Icon); 3] = [
    ("语感解析", Icon::FileText),
    ("更自然地说", Icon::Sparkles),
    ("真实对话", Icon::MessageSquare),
];

/// 选中态的（描边与文字、底色）。
fn accent(tab: usize) -> (Color, Color) {
    if tab == 1 {
        (TEAL, TEAL_BG)
    } else {
        (tokens::PRIMARY, tokens::SELECTED_BG)
    }
}

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView) {
    let bar = Area::new(
        MARGIN_BAR,
        TAB_BAR_Y,
        WIDTH - 2.0 * MARGIN_BAR,
        TAB_BAR_HEIGHT,
    );
    s.fill(bar, tokens::RADIUS_BUTTON + 1.0, tokens::TABBAR_BG);
    let tab_width = bar.width / 3.0;
    for (index, (label, icon)) in TABS.iter().enumerate() {
        let area = Area::new(
            bar.x + index as f32 * tab_width,
            bar.y,
            tab_width,
            bar.height,
        );
        let selected = view.tab == index;
        let mut color = TEXT_MID;
        let mut style = PanelText::new(12.0, 18.0, color);
        if selected {
            let (fg, bg) = accent(index);
            s.fill_bordered(area.inset(1.0), tokens::RADIUS_BUTTON, bg, fg);
            color = fg;
            style = PanelText::new(12.0, 18.0, color).bolded();
        } else if index > 0 && view.tab != index - 1 {
            s.fill(
                Area::new(area.x, area.y + 8.0, 1.0, area.height - 16.0),
                0.0,
                tokens::BORDER,
            );
        }
        let text_width = s.measure(label, &style);
        let left = area.x + (area.width - text_width - 20.0) / 2.0;
        s.icon(
            *icon,
            left,
            area.y + (area.height - 14.0) / 2.0,
            14.0,
            color,
        );
        s.text(
            left + 20.0,
            area.y + (area.height - 18.0) / 2.0,
            label,
            &style,
        );
        s.hit(area, CompanionHit::Tab(index));
    }
}
