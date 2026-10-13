//! 标题栏下面的英雄区：中文原意、英文大字加朗读、特点标签与等级。

use super::hit::CompanionHit;
use super::style::{CHIP_ROW_Y, HERO_EN_Y, HERO_ZH_Y, MARGIN_CARD, WIDTH};
use super::view::CompanionView;
use super::widgets::{Surface, chip, chip_right, icon_button};
use crate::panel::{Area, Icon, PanelText, tokens};

const ZH: PanelText = PanelText::new(14.0, 20.0, tokens::TEXT).bolded();
const EN: PanelText = PanelText::new(22.0, 28.0, tokens::PRIMARY).bolded();

/// 特点标签的配色：第一个橙，其余绿。
fn tag_colors(index: usize) -> crate::panel::ColorPair {
    if index == 0 {
        tokens::TAG_ORANGE
    } else {
        tokens::TAG_GREEN
    }
}

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView) {
    let x = MARGIN_CARD + 2.0;
    s.text(x, HERO_ZH_Y, &view.zh, &ZH);
    let width = s.text(x, HERO_EN_Y, &view.en, &EN);
    let speaker = Area::new(x + width + 6.0, HERO_EN_Y + 1.0, 26.0, 26.0);
    icon_button(
        s,
        speaker,
        Icon::Volume2,
        18.0,
        tokens::PRIMARY,
        CompanionHit::Speak(view.en.clone()),
    );
    let mut cx = x;
    for (index, tag) in view.tags.iter().take(3).enumerate() {
        cx += chip(s, cx, CHIP_ROW_Y, tag, tag_colors(index)) + 8.0;
    }
    if !view.level.is_empty() {
        chip_right(
            s,
            WIDTH - MARGIN_CARD,
            CHIP_ROW_Y,
            &view.level,
            tokens::TAG_BLUE,
        );
    }
}
