//! 面板里反复出现的小部件：胶囊、圆形图标按钮、高亮短语拆分、骨架条。

use super::hit::CompanionHit;
use super::style::{CAPTION, SKELETON};
use crate::color::Color;
use crate::panel::{Area, ColorPair, Icon, PanelSurface, PanelText, tokens};
use crate::text::RichSpan;

pub(super) type Surface<'a> = PanelSurface<'a, CompanionHit>;

/// 胶囊文字样式。
const CHIP_TEXT: PanelText = PanelText::new(11.0, 15.0, tokens::TEXT);

/// 胶囊宽度：文字宽 + 两侧内边距。
pub(super) fn chip_width(s: &mut Surface<'_>, text: &str) -> f32 {
    s.measure(text, &CHIP_TEXT) + 18.0
}

/// 在 `(x, y)` 画一个高 20 的胶囊，返回宽度。
pub(super) fn chip(s: &mut Surface<'_>, x: f32, y: f32, text: &str, colors: ColorPair) -> f32 {
    let width = chip_width(s, text);
    s.fill(Area::new(x, y, width, 20.0), tokens::RADIUS_CHIP, colors.bg);
    s.text(x + 9.0, y + 2.5, text, &CHIP_TEXT.colored(colors.fg));
    width
}

/// 右边缘对齐到 `right` 的胶囊，返回宽度。
pub(super) fn chip_right(
    s: &mut Surface<'_>,
    right: f32,
    y: f32,
    text: &str,
    colors: ColorPair,
) -> f32 {
    let width = chip_width(s, text);
    chip(s, right - width, y, text, colors);
    width
}

/// 无底的图标按钮：图标居中，点击区是整块 `area`。
pub(super) fn icon_button(
    s: &mut Surface<'_>,
    area: Area,
    icon: Icon,
    size: f32,
    color: Color,
    hit: CompanionHit,
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

/// 白底描边的圆形图标按钮（复制、朗读）。
pub(super) fn round_button(
    s: &mut Surface<'_>,
    cx: f32,
    cy: f32,
    diameter: f32,
    icon: Icon,
    hit: CompanionHit,
) {
    let area = Area::new(cx - diameter / 2.0, cy - diameter / 2.0, diameter, diameter);
    s.fill_bordered(area, diameter / 2.0, tokens::WINDOW_BG, tokens::BORDER);
    let size = (diameter * 0.55).round();
    s.icon(icon, cx - size / 2.0, cy - size / 2.0, size, tokens::TEXT_2);
    s.hit(area, hit);
}

/// 把 `text` 按目标短语 `hl` 拆成「前 / 粗体 / 后」；`hl` 为空或不在句中时整句不加粗。
pub(super) fn highlight_spans<'a>(text: &'a str, hl: &str) -> Vec<RichSpan<'a>> {
    let found = (!hl.is_empty())
        .then(|| text.find(hl).map(|at| (at, hl.len())))
        .flatten()
        .or_else(|| find_ignore_case(text, hl));
    match found {
        Some((at, len)) => {
            let mut spans = Vec::with_capacity(3);
            if at > 0 {
                spans.push(RichSpan::plain(&text[..at]));
            }
            spans.push(RichSpan::bold(&text[at..at + len]));
            if at + len < text.len() {
                spans.push(RichSpan::plain(&text[at + len..]));
            }
            spans
        }
        None => vec![RichSpan::plain(text)],
    }
}

/// 英文大小写不敏感查找（只在 ASCII 范围内，保证字节下标合法）。
fn find_ignore_case(text: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() || !text.is_ascii() || !needle.is_ascii() {
        return None;
    }
    let at = text
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())?;
    Some((at, needle.len()))
}

/// 骨架条：加载中的占位。
pub(super) fn skeleton_bar(s: &mut Surface<'_>, x: f32, y: f32, width: f32) {
    s.fill(Area::new(x, y, width, 11.0), 5.5, SKELETON);
}

/// 说明小字（灰色 11 pt）。
pub(super) fn caption(s: &mut Surface<'_>, x: f32, y: f32, width: f32, text: &str) -> f32 {
    s.paragraph(x, y, width, text, &CAPTION).height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_splits_around_the_phrase() {
        let spans = highlight_spans("Just do your best. That's all.", "do your best");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "Just ");
        assert_eq!(spans[1].text, "do your best");
        assert_eq!(spans[1].bold, Some(true));
        assert_eq!(spans[2].text, ". That's all.");
    }

    #[test]
    fn highlight_falls_back_to_plain_and_ignores_case() {
        assert_eq!(highlight_spans("Hello", "").len(), 1);
        assert_eq!(highlight_spans("Hello", "bye").len(), 1);
        let spans = highlight_spans("Do your best.", "do your best");
        assert_eq!((spans.len(), spans[0].text), (2, "Do your best"));
        assert_eq!(spans[0].bold, Some(true));
        // 非 ASCII 不做大小写折叠，也不会在字符中间切开
        assert_eq!(highlight_spans("尽量 Do", "尽量").len(), 2);
    }
}
