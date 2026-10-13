//! 「更自然地说」标签：3 个更地道的替换表达，每个带语气、区别和一条例句。

use super::alternative::Alternative;
use super::hit::CompanionHit;
use super::states;
use super::status::TabStatus;
use super::style::{BODY, CAPTION, card_area};
use super::view::CompanionView;
use super::widgets::{Surface, chip_right, chip_width, icon_button};
use crate::color::Color;
use crate::panel::{Area, ColorPair, Icon, PanelText, tokens};
use crate::text::RichSpan;

const BADGES: [Color; 3] = [tokens::BADGE_1, tokens::BADGE_2, tokens::BADGE_3];
const EXPRESSION: PanelText = PanelText::new(13.0, 20.0, tokens::TEXT).bolded();
const EXAMPLE_EN: PanelText = PanelText::new(11.0, 14.0, tokens::TEXT);
const EXAMPLE_ZH: PanelText = PanelText::new(10.5, 14.0, tokens::TEXT_2);
const BULLET: PanelText = PanelText::new(11.0, 13.5, super::style::TEXT_MID);
const HEADING: PanelText = PanelText::new(13.0, 18.0, tokens::TEXT).bolded();

/// 卡片之间的间距。
const GAP: f32 = 5.0;

/// 表达左侧留给序号徽标的宽度。
const INDENT: f32 = 47.0;

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView, mut y: f32) -> f32 {
    match &view.alternatives_status {
        TabStatus::Idle => return states::empty(s, y),
        TabStatus::Loading => return states::loading(s, y, "更自然的表达", view.spin),
        TabStatus::Failed(message) if view.alternatives.is_empty() => {
            return states::error_card(s, y, message);
        }
        _ => {}
    }
    if matches!(view.alternatives_status, TabStatus::Failed(_)) {
        y = states::error_strip(s, y) + GAP;
    }
    let head = card_area(y, 0.0);
    s.text(head.x + 2.0, y - 0.5, "3 个更地道的替换表达", &HEADING);
    let note = "不同语气，适用不同场景";
    let width = s.measure(note, &CAPTION);
    s.text(head.right() - width - 2.0, y + 1.5, note, &CAPTION);
    let mut bottom = y + 23.0;
    for (index, alt) in view.alternatives.iter().enumerate() {
        bottom = card(s, bottom, index, alt) + GAP;
    }
    if view.alternatives_status == TabStatus::Streaming {
        bottom = states::tail(s, bottom, view.spin) + GAP;
    }
    bottom + 4.0
}

/// 标签文字 → 配色：先看语义，认不出就按序号轮换。
fn tag_colors(tag: &str, index: usize) -> ColorPair {
    let has = |words: &[&str]| words.iter().any(|w| tag.contains(w));
    if has(&["口语", "日常", "轻松", "随意"]) {
        tokens::TAG_GREEN
    } else if has(&["温和", "安慰", "支持", "鼓励", "委婉"]) {
        tokens::TAG_BLUE
    } else if has(&["力度", "强", "正式", "有力", "地道"]) {
        tokens::TAG_ORANGE
    } else {
        [tokens::TAG_GREEN, tokens::TAG_BLUE, tokens::TAG_ORANGE][index % 3]
    }
}

/// 一张替换表达卡，返回下沿。
fn card(s: &mut Surface<'_>, y: f32, index: usize, alt: &Alternative) -> f32 {
    let probe = card_area(y, 0.0);
    let (left, width) = (probe.x, probe.width);
    let text_x = left + INDENT;
    let tag_w = if alt.tag.is_empty() {
        0.0
    } else {
        chip_width(s, &alt.tag)
    };
    let expr_w = width - INDENT - tag_w - 60.0;
    let expr = s.measure_rich(&[RichSpan::plain(&alt.expression)], &EXPRESSION, expr_w);
    let body_w = width - INDENT - 10.0;

    // 先量出各段高度，再定卡片总高
    let zh_h = s
        .measure_rich(&[RichSpan::plain(&alt.zh)], &BODY, body_w)
        .height;
    let tone = format!("语气：{}", alt.tone);
    let diff = format!("区别：{}", alt.diff);
    let tone_h = s
        .measure_rich(&[RichSpan::plain(&tone)], &BULLET, body_w - 10.0)
        .height;
    let diff_h = s
        .measure_rich(&[RichSpan::plain(&diff)], &BULLET, body_w - 10.0)
        .height;
    let box_w = width - INDENT - 10.0;
    let ex_w = box_w - 9.0 - 28.0;
    let ex = &alt.example;
    let en_h = s
        .measure_rich(&[RichSpan::plain(&ex.en)], &EXAMPLE_EN, ex_w)
        .height;
    let ex_zh_h = s
        .measure_rich(&[RichSpan::plain(&ex.zh)], &EXAMPLE_ZH, ex_w)
        .height;
    let box_h = 4.0 + en_h + ex_zh_h + 4.0;
    let height = 3.0 + expr.height.max(20.0) + zh_h + 3.0 + tone_h + diff_h + 7.5 + box_h + 9.5;

    let card = Area::new(left, y, width, height);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::WINDOW_BG, tokens::BORDER);
    let badge = Area::new(left + 10.0, y + 6.0, 22.0, 22.0);
    s.fill(badge, 6.0, BADGES[index % 3]);
    let number = (index + 1).to_string();
    let number_style = PanelText::new(12.0, 18.0, tokens::WINDOW_BG).bolded();
    let nw = s.measure(&number, &number_style);
    s.text(
        badge.x + (badge.width - nw) / 2.0,
        badge.y + 2.0,
        &number,
        &number_style,
    );

    let mut cy = y + 3.0;
    s.paragraph(text_x, cy, expr_w, &alt.expression, &EXPRESSION);
    let icons_x = text_x + expr.width + 6.0;
    icon_button(
        s,
        Area::new(icons_x, cy - 1.0, 22.0, 22.0),
        Icon::Volume2,
        14.0,
        tokens::TEXT_2,
        CompanionHit::Speak(alt.expression.clone()),
    );
    icon_button(
        s,
        Area::new(icons_x + 22.0, cy - 1.0, 22.0, 22.0),
        Icon::Copy,
        13.0,
        tokens::TEXT_2,
        CompanionHit::Copy(alt.expression.clone()),
    );
    if !alt.tag.is_empty() {
        chip_right(
            s,
            card.right() - 8.0,
            y + 6.0,
            &alt.tag,
            tag_colors(&alt.tag, index),
        );
    }
    cy += expr.height.max(20.0);
    s.paragraph(text_x, cy, body_w, &alt.zh, &BODY);
    cy += zh_h + 3.0;
    for (text, text_h) in [(&tone, tone_h), (&diff, diff_h)] {
        s.circle(text_x + 2.0, cy + 7.0, 1.5, tokens::TEXT_2);
        s.paragraph(text_x + 10.0, cy, body_w - 10.0, text, &BULLET);
        cy += text_h;
    }
    cy += 7.5;
    let example = Area::new(text_x, cy, box_w, box_h);
    s.fill(example, tokens::RADIUS_BUTTON, tokens::CARD_BLUE);
    s.paragraph(example.x + 9.0, cy + 4.0, ex_w, &ex.en, &EXAMPLE_EN);
    s.paragraph(example.x + 9.0, cy + 4.0 + en_h, ex_w, &ex.zh, &EXAMPLE_ZH);
    icon_button(
        s,
        Area::new(
            example.right() - 28.0,
            cy + (box_h - 22.0) / 2.0,
            22.0,
            22.0,
        ),
        Icon::Copy,
        13.0,
        tokens::TEXT_2,
        CompanionHit::Copy(ex.en.clone()),
    );
    card.bottom()
}
