//! 会话开头的欢迎卡与建议问题。

use super::hit::ChatHit;
use super::style::{CHIP_TEXT, TEXT_MID};
use super::widgets::{Surface, outlined_chip};
use crate::panel::{Area, PanelText, tokens};
use crate::text::RichSpan;

const CARD_PAD_X: f32 = 14.0;
const CARD_PAD_TOP: f32 = 8.0;
const CARD_PAD_BOTTOM: f32 = 6.0;
const CARD_RADIUS: f32 = 12.0;
const TITLE: PanelText = PanelText::new(13.0, 19.0, tokens::TEXT).bolded();
const POINT: PanelText = PanelText::new(12.0, 19.0, TEXT_MID);

/// 欢迎卡里的要点，与《严格参考图》一致。
const POINTS: [&str; 4] = [
    "解释更地道的用法和语气区别",
    "给出更多真实的例句与场景",
    "为你设计 IELTS 口语练习题",
    "帮你纠正你自己说的句子",
];

/// 要点与标题之间的间距、要点的缩进。
const TITLE_GAP: f32 = 3.0;
const POINT_INDENT: f32 = 18.0;

/// 建议胶囊：高度、间距、内边距、离欢迎卡的距离。
const CHIP_HEIGHT: f32 = 25.0;
const CHIP_GAP: f32 = 8.0;
const CHIP_ROW_GAP: f32 = 5.0;
const CHIP_PAD: f32 = 12.0;
const CHIP_INDENT: f32 = 8.0;
const CHIP_TOP: f32 = 8.0;

fn title_text(topic: Option<&str>) -> String {
    match topic {
        Some(topic) => format!("👋 你好！我可以围绕“{topic}”帮你："),
        None => "👋 你好！有英语问题随时问我，我可以帮你：".to_owned(),
    }
}

/// 欢迎卡的尺寸（点）。
pub(super) fn measure(s: &mut Surface<'_>, topic: Option<&str>, max_width: f32) -> (f32, f32) {
    let title = title_text(topic);
    let inner = max_width - 2.0 * CARD_PAD_X;
    let title_size = s.measure_rich(&[RichSpan::plain(&title)], &TITLE, inner);
    let mut width = title_size.width;
    for point in POINTS {
        width = width.max(POINT_INDENT + s.measure(point, &POINT));
    }
    let height = CARD_PAD_TOP
        + title_size.height
        + TITLE_GAP
        + POINTS.len() as f32 * POINT.line_height
        + CARD_PAD_BOTTOM;
    (width + 2.0 * CARD_PAD_X, height)
}

/// 画欢迎卡，`(x, y)` 是左上角；`width` / `height` 来自 [`measure`]。
pub(super) fn draw(
    s: &mut Surface<'_>,
    topic: Option<&str>,
    (x, y): (f32, f32),
    (width, height): (f32, f32),
) {
    s.fill(
        Area::new(x, y, width, height),
        CARD_RADIUS,
        tokens::CARD_BLUE,
    );
    let title = title_text(topic);
    let inner = width - 2.0 * CARD_PAD_X;
    let size = s.rich(
        x + CARD_PAD_X,
        y + CARD_PAD_TOP,
        inner + 0.5,
        &[RichSpan::plain(&title)],
        &TITLE,
    );
    let mut line_y = y + CARD_PAD_TOP + size.height + TITLE_GAP;
    for point in POINTS {
        s.circle(
            x + CARD_PAD_X + 5.0,
            line_y + POINT.line_height / 2.0,
            2.0,
            tokens::TEXT,
        );
        s.text(x + CARD_PAD_X + POINT_INDENT, line_y, point, &POINT);
        line_y += POINT.line_height;
    }
}

/// 建议胶囊的位置（相对 `(0, 0)`，已换行）与总高度。
pub(super) fn measure_suggestions(
    s: &mut Surface<'_>,
    suggestions: &[String],
    max_width: f32,
) -> (Vec<Area>, f32) {
    let mut areas = Vec::with_capacity(suggestions.len());
    let (mut x, mut y) = (CHIP_INDENT, CHIP_TOP);
    for text in suggestions {
        let width = (s.measure(text, &CHIP_TEXT) + 2.0 * CHIP_PAD).min(max_width - CHIP_INDENT);
        if x > CHIP_INDENT && x + width > max_width {
            x = CHIP_INDENT;
            y += CHIP_HEIGHT + CHIP_ROW_GAP;
        }
        areas.push(Area::new(x, y, width, CHIP_HEIGHT));
        x += width + CHIP_GAP;
    }
    let height = areas.last().map_or(0.0, |last| last.bottom());
    (areas, height)
}

pub(super) fn draw_suggestions(
    s: &mut Surface<'_>,
    suggestions: &[String],
    areas: &[Area],
    (x, y): (f32, f32),
) {
    for (index, (text, area)) in suggestions.iter().zip(areas).enumerate() {
        let area = Area::new(x + area.x, y + area.y, area.width, area.height);
        outlined_chip(s, area, text, Some(ChatHit::Suggestion(index)));
    }
}
