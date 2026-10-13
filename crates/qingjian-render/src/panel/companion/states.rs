//! 空状态、加载中、流式进度、失败：标签页内容区里除正常内容外的几种样子。每个函数从 `y` 画起，返回下沿 y。

use super::hit::CompanionHit;
use super::style::{BODY, BODY_BOLD, MARGIN_CARD, SKELETON, SMALL, WIDTH, card_area};
use super::widgets::{Surface, caption, skeleton_bar};
use crate::color::Color;
use crate::panel::{Area, Icon, PanelText, tokens};
use crate::text::RichSpan;

/// 流式进度的四步，标签是固定文案，哪些做完了看内容到了没有。
pub(super) const STEP_LABELS: [&str; 4] = [
    "已获取基础释义",
    "正在分析语气和场景...",
    "正在生成例句...",
    "正在整理搭配信息...",
];

/// 骨架条宽度占卡片内宽的比例。
const BAR_RATIOS: [f32; 6] = [1.0, 0.64, 1.0, 0.64, 0.82, 0.64];

const DONE_GREEN: Color = Color::hex(0x16A34A);

/// 居中画单行文字，返回行高。
fn centered(s: &mut Surface<'_>, y: f32, text: &str, style: &PanelText) -> f32 {
    let width = s.measure(text, style);
    s.text((WIDTH - width) / 2.0, y, text, style);
    style.line_height
}

/// 居中画一段会换行的文字，返回占用高度。
fn centered_paragraph(
    s: &mut Surface<'_>,
    y: f32,
    width: f32,
    text: &str,
    style: &PanelText,
) -> f32 {
    let size = s.measure_rich(&[RichSpan::plain(text)], style, width);
    s.paragraph((WIDTH - size.width) / 2.0, y, width, text, style);
    size.height
}

/// 空状态：首次进入、还没请求任何内容。
pub(super) fn empty(s: &mut Surface<'_>, y: f32) -> f32 {
    let card = card_area(y, 168.0);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    let cx = WIDTH / 2.0;
    s.circle(cx, y + 42.0, 28.0, tokens::SELECTED_BG);
    s.icon(Icon::ScanText, cx - 15.0, y + 27.0, 30.0, tokens::PRIMARY);
    centered(s, y + 84.0, "点击上方选项卡查看内容", &BODY_BOLD);
    let text = "我们会为你提供简洁、地道、实用的英语表达解析。";
    centered_paragraph(s, y + 110.0, card.width - 70.0, text, &SMALL);
    card.bottom()
}

/// 加载中：一个区块都还没到。
pub(super) fn loading(s: &mut Surface<'_>, y: f32, what: &str, spin: f32) -> f32 {
    let card = card_area(y, 218.0);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    s.icon_rotated(
        Icon::LoaderCircle,
        card.x + 14.0,
        y + 14.0,
        18.0,
        tokens::PRIMARY,
        spin,
    );
    s.text(
        card.x + 40.0,
        y + 15.0,
        &format!("正在生成{what}…"),
        &BODY_BOLD,
    );
    let inner = card.width - 28.0;
    let mut by = y + 46.0;
    for ratio in BAR_RATIOS {
        skeleton_bar(s, card.x + 14.0, by, inner * ratio);
        by += 19.0;
    }
    s.hline(card.x + 14.0, by + 6.0, inner, tokens::BORDER);
    caption(
        s,
        card.x + 14.0,
        by + 14.0,
        inner,
        "正在分析表达的含义、语气和使用场景，预计 3-5 秒…",
    );
    card.bottom()
}

/// 流式进度：`done[i]` 为这一步已经有内容；第一个没做完的转圈，后面的灰点。
pub(super) fn steps(s: &mut Surface<'_>, y: f32, done: [bool; 4], spin: f32) -> f32 {
    let card = card_area(y, 148.0);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    s.text(card.x + 14.0, y + 12.0, "正在生成...", &BODY_BOLD);
    let mut row = y + 40.0;
    let mut active_seen = false;
    for (label, done) in STEP_LABELS.iter().zip(done) {
        let (cx, cy) = (card.x + 22.0, row + 9.0);
        if done {
            s.icon(Icon::CircleCheck, cx - 8.0, cy - 8.0, 16.0, DONE_GREEN);
        } else if !active_seen {
            active_seen = true;
            s.icon_rotated(
                Icon::LoaderCircle,
                cx - 8.0,
                cy - 8.0,
                16.0,
                tokens::PRIMARY,
                spin,
            );
        } else {
            s.circle(cx, cy, 6.0, SKELETON);
        }
        s.text(card.x + 40.0, row, label, &BODY);
        row += 24.0;
    }
    card.bottom()
}

/// 通用的「还在生成」小尾巴：内容已经出来一部分，下面补一张带转圈的占位卡。
pub(super) fn tail(s: &mut Surface<'_>, y: f32, spin: f32) -> f32 {
    let card = card_area(y, 76.0);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    s.icon_rotated(
        Icon::LoaderCircle,
        card.x + 14.0,
        y + 12.0,
        16.0,
        tokens::PRIMARY,
        spin,
    );
    s.text(card.x + 38.0, y + 12.0, "正在生成...", &BODY_BOLD);
    skeleton_bar(s, card.x + 14.0, y + 42.0, card.width - 28.0);
    skeleton_bar(s, card.x + 14.0, y + 58.0, (card.width - 28.0) * 0.6);
    card.bottom()
}

/// 失败且一点内容都没有：居中的大提示与重试按钮。
pub(super) fn error_card(s: &mut Surface<'_>, y: f32, message: &str) -> f32 {
    let inner = WIDTH - 2.0 * MARGIN_CARD - 48.0;
    let text_h = s
        .measure_rich(&[RichSpan::plain(message)], &SMALL, inner)
        .height;
    let height = 24.0 + 36.0 + 8.0 + 18.0 + 6.0 + text_h + 16.0 + 34.0 + 24.0;
    let card = card_area(y, height);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::WINDOW_BG, tokens::BORDER);
    let cx = WIDTH / 2.0;
    s.icon(
        Icon::CircleAlert,
        cx - 18.0,
        y + 24.0,
        36.0,
        tokens::ERROR_FG,
    );
    let mut cy = y + 24.0 + 36.0 + 8.0;
    cy += centered(s, cy, "生成失败", &BODY_BOLD.colored(tokens::ERROR_FG)) + 6.0;
    centered_paragraph(s, cy, inner, message, &SMALL);
    cy += text_h + 16.0;
    let button = Area::new(cx - 60.0, cy, 120.0, 34.0);
    s.fill(button, tokens::RADIUS_BUTTON, tokens::PRIMARY);
    let white = BODY_BOLD.colored(tokens::WINDOW_BG);
    let label_w = s.measure("重新生成", &white);
    let left = button.x + (button.width - label_w - 22.0) / 2.0;
    s.icon(Icon::RefreshCw, left, cy + 9.0, 16.0, tokens::WINDOW_BG);
    s.text(left + 22.0, cy + 8.0, "重新生成", &white);
    s.hit(button, CompanionHit::Retry);
    card.bottom()
}

/// 失败但已有部分内容：一条红色提示条，右边重试。
pub(super) fn error_strip(s: &mut Surface<'_>, y: f32) -> f32 {
    let card = card_area(y, 46.0);
    s.fill_bordered(
        card,
        tokens::RADIUS_CARD,
        tokens::ERROR_BG,
        tokens::ERROR_BORDER,
    );
    s.icon(
        Icon::CircleAlert,
        card.x + 12.0,
        y + 14.0,
        18.0,
        tokens::ERROR_FG,
    );
    s.text(
        card.x + 38.0,
        y + 14.0,
        "生成失败，请重试",
        &BODY.colored(tokens::ERROR_FG),
    );
    let button = Area::new(card.right() - 92.0, y + 9.0, 80.0, 28.0);
    s.fill_bordered(
        button,
        tokens::RADIUS_BUTTON,
        tokens::WINDOW_BG,
        tokens::ERROR_BORDER,
    );
    let style = SMALL.colored(tokens::ERROR_FG);
    let width = s.measure("重新生成", &style);
    s.text(
        button.x + (button.width - width) / 2.0,
        y + 15.0,
        "重新生成",
        &style,
    );
    s.hit(button, CompanionHit::Retry);
    card.bottom()
}
