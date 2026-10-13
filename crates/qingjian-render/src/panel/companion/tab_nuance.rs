//! 「语感解析」标签：意思、语气、使用场景三张卡，一条自然例句，可折叠的搭配与语法细节。

use super::example::Example;
use super::hit::CompanionHit;
use super::states;
use super::status::TabStatus;
use super::style::{BODY, BODY_BOLD, CARD_GAP, NAVY, SMALL_BOLD, TEXT_MID, card_area};
use super::view::CompanionView;
use super::widgets::{Surface, highlight_spans, icon_button, round_button};
use crate::color::Color;
use crate::panel::{Area, Icon, PanelText, tokens};
use crate::text::RichSpan;

/// 一张信息卡的样子。
struct CardStyle {
    icon: Icon,
    title: &'static str,
    title_color: Color,
    chip: Color,
    bg: Color,
}

const MEANING: CardStyle = CardStyle {
    icon: Icon::FileText,
    title: "意思",
    title_color: tokens::TITLE_BLUE,
    chip: tokens::ICON_CHIP_BLUE,
    bg: tokens::CARD_BLUE,
};
const TONE: CardStyle = CardStyle {
    icon: Icon::Clock,
    title: "语气",
    title_color: tokens::TITLE_GREEN,
    chip: tokens::ICON_CHIP_GREEN,
    bg: tokens::CARD_BG,
};
const USAGE: CardStyle = CardStyle {
    icon: Icon::Target,
    title: "使用场景",
    title_color: tokens::TITLE_PURPLE,
    chip: tokens::ICON_CHIP_PURPLE,
    bg: tokens::CARD_PURPLE,
};

/// 例句中文的字号。
const EXAMPLE_ZH: PanelText = PanelText::new(12.0, 17.0, tokens::TEXT_2);

/// 从 `y` 画起，返回内容下沿。
pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView, mut y: f32) -> f32 {
    match &view.nuance_status {
        TabStatus::Idle => return states::empty(s, y),
        TabStatus::Loading => return states::loading(s, y, "语感解析", view.spin),
        TabStatus::Failed(message) if view.nuance.is_empty() => {
            return states::error_card(s, y, message);
        }
        _ => {}
    }
    // 失败提示放最上面：内容再长也看得到重试按钮
    if matches!(view.nuance_status, TabStatus::Failed(_)) {
        y = states::error_strip(s, y) + CARD_GAP;
    }
    let nuance = &view.nuance;
    for (style, text) in [
        (&MEANING, &nuance.meaning),
        (&TONE, &nuance.tone),
        (&USAGE, &nuance.usage),
    ] {
        if !text.is_empty() {
            y = info_card(s, y, style, text) + CARD_GAP;
        }
    }
    if let Some(example) = &nuance.example {
        y = example_section(s, y, example);
    }
    if !nuance.details.is_empty() {
        y = details(s, y, view);
    }
    if view.nuance_status == TabStatus::Streaming {
        let done = [
            !nuance.meaning.is_empty(),
            !nuance.tone.is_empty() && !nuance.usage.is_empty(),
            nuance.example.is_some(),
            !nuance.details.is_empty(),
        ];
        y = states::steps(s, y, done, view.spin) + CARD_GAP;
    }
    y + 4.0
}

/// 左边图标底块 + 标题 + 正文；正文换行时卡片变高。返回卡片下沿。
fn info_card(s: &mut Surface<'_>, y: f32, style: &CardStyle, text: &str) -> f32 {
    let probe = card_area(y, 0.0);
    let text_width = probe.width - 48.0;
    let text_height = s
        .measure_rich(&[RichSpan::plain(text)], &BODY, text_width)
        .height;
    let card = card_area(y, (12.0 + 18.0 + text_height).max(50.0));
    s.fill(card, tokens::RADIUS_CARD, style.bg);
    let chip = Area::new(card.x + 5.0, y + 5.0, 26.0, 26.0);
    s.fill(chip, 7.0, style.chip);
    s.icon(
        style.icon,
        chip.x + 5.0,
        chip.y + 5.0,
        16.0,
        style.title_color,
    );
    s.text(
        card.x + 42.0,
        y + 5.0,
        style.title,
        &BODY_BOLD.colored(style.title_color),
    );
    s.paragraph(card.x + 42.0, y + 25.0, text_width, text, &BODY);
    card.bottom()
}

/// 分割线 + 「自然例句」小标题 + 例句卡，返回下沿（含与下一块的间距）。
fn example_section(s: &mut Surface<'_>, y: f32, example: &Example) -> f32 {
    let card = card_area(y + 4.0, 0.0);
    s.hline(card.x, y + 4.0, card.width, tokens::BORDER);
    s.text(card.x + 2.0, y + 14.0, "自然例句", &BODY_BOLD.colored(NAVY));
    let top = y + 38.5;
    let text_width = card.width - 16.0 - 84.0;
    let spans = highlight_spans(&example.en, &example.hl);
    let en_h = s.measure_rich(&spans, &BODY, text_width).height;
    let zh_h = s
        .measure_rich(&[RichSpan::plain(&example.zh)], &EXAMPLE_ZH, text_width)
        .height;
    let area = card_area(top, (8.0 + en_h + 2.0 + zh_h + 8.0).max(54.0));
    s.fill_bordered(area, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    s.rich(area.x + 14.0, top + 8.0, text_width, &spans, &BODY);
    s.paragraph(
        area.x + 14.0,
        top + 10.0 + en_h,
        text_width,
        &example.zh,
        &EXAMPLE_ZH,
    );
    let cy = top + area.height / 2.0;
    icon_button(
        s,
        Area::new(area.right() - 76.0, cy - 14.0, 28.0, 28.0),
        Icon::Volume2,
        14.0,
        tokens::TEXT_2,
        CompanionHit::Speak(example.en.clone()),
    );
    round_button(
        s,
        area.right() - 24.0,
        cy,
        28.0,
        Icon::Copy,
        CompanionHit::Copy(example.en.clone()),
    );
    area.bottom() + 12.0
}

/// 「搭配与语法细节（N 项）」折叠行，展开后每项一张小卡。
fn details(s: &mut Surface<'_>, y: f32, view: &CompanionView) -> f32 {
    let row = card_area(y, 38.0);
    s.fill_bordered(
        row,
        tokens::RADIUS_BUTTON,
        tokens::WINDOW_BG,
        tokens::BORDER,
    );
    let count = view.nuance.details.len();
    s.icon(Icon::ChevronDown, row.x + 12.0, y + 12.0, 14.0, NAVY);
    s.text(
        row.x + 34.0,
        y + 10.0,
        &format!("搭配与语法细节（{count} 项）"),
        &SMALL_BOLD,
    );
    let tail = if view.details_open {
        Icon::ChevronUp
    } else {
        Icon::ChevronRight
    };
    s.icon(tail, row.right() - 28.0, y + 11.0, 16.0, tokens::TEXT_2);
    s.hit(row, CompanionHit::ToggleDetails);
    let mut bottom = row.bottom() + CARD_GAP;
    if !view.details_open {
        return bottom;
    }
    let body = PanelText::new(12.0, 17.0, TEXT_MID);
    for detail in &view.nuance.details {
        let width = row.width - 28.0;
        let body_h = s
            .measure_rich(&[RichSpan::plain(&detail.body)], &body, width)
            .height;
        let card = card_area(bottom, 10.0 + 17.0 + body_h + 10.0);
        s.fill(card, tokens::RADIUS_BUTTON, tokens::CARD_BG);
        s.text(card.x + 14.0, bottom + 9.0, &detail.title, &SMALL_BOLD);
        s.paragraph(card.x + 14.0, bottom + 27.0, width, &detail.body, &body);
        bottom = card.bottom() + 6.0;
    }
    bottom + 2.0
}
