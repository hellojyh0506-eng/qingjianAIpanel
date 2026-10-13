//! 气泡里一个 Markdown 块的量与画。

use super::super::markdown::{Block, Inline};
use super::super::style::{BODY, CODE, CODE_BG};
use super::super::widgets::Surface;
use super::colors::BubbleColors;
use crate::color::Color;
use crate::panel::{Area, PanelText};
use crate::text::RichSpan;

/// 换行宽度留的余量：量出来的宽度再拿去画时不会因浮点误差多折一行。
const WRAP_SLACK: f32 = 0.5;

const CODE_PAD_X: f32 = 8.0;
const CODE_PAD_Y: f32 = 6.0;
const BULLET_INDENT: f32 = 14.0;
const NUMBER_INDENT: f32 = 22.0;
const QUOTE_INDENT: f32 = 11.0;

/// 行内片段 → 富文本：粗体加粗，行内代码换色。
fn spans(inlines: &[Inline], code: Color) -> Vec<RichSpan<'_>> {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Plain(text) => RichSpan::plain(text),
            Inline::Bold(text) => RichSpan::bold(text),
            Inline::Code(text) => RichSpan::plain(text).colored(code),
        })
        .collect()
}

/// 块的文字样式与左缩进。
fn geometry(block: &Block, colors: &BubbleColors) -> (PanelText, f32) {
    let body = BODY.colored(colors.text);
    match block {
        Block::Heading { level: 1, .. } => (PanelText::new(15.0, 22.0, colors.text).bolded(), 0.0),
        Block::Heading { level: 2, .. } => (PanelText::new(14.0, 21.0, colors.text).bolded(), 0.0),
        Block::Heading { .. } => (body.bolded(), 0.0),
        Block::Bullet(_) => (body, BULLET_INDENT),
        Block::Numbered { .. } => (body, NUMBER_INDENT),
        Block::Quote(_) => (body.colored(colors.muted), QUOTE_INDENT),
        Block::Code { .. } => (CODE.colored(colors.text), CODE_PAD_X),
        Block::Paragraph(_) => (body, 0.0),
    }
}

fn block_spans<'a>(block: &'a Block, colors: &BubbleColors) -> Vec<RichSpan<'a>> {
    match block {
        Block::Heading { spans: inlines, .. }
        | Block::Numbered { spans: inlines, .. }
        | Block::Bullet(inlines)
        | Block::Quote(inlines)
        | Block::Paragraph(inlines) => spans(inlines, colors.code),
        Block::Code { text } => vec![RichSpan::plain(text)],
    }
}

/// 块在 `inner_width`（点）内排版后的宽高。
pub(in crate::panel::chat) fn measure(
    s: &mut Surface<'_>,
    block: &Block,
    inner_width: f32,
    colors: &BubbleColors,
) -> (f32, f32) {
    let (style, indent) = geometry(block, colors);
    let is_code = matches!(block, Block::Code { .. });
    let wrap = inner_width - indent * if is_code { 2.0 } else { 1.0 };
    let size = s.measure_rich(&block_spans(block, colors), &style, wrap);
    let height = size.height.max(style.line_height);
    if is_code {
        (size.width + 2.0 * CODE_PAD_X, height + 2.0 * CODE_PAD_Y)
    } else {
        (size.width + indent, height)
    }
}

/// 把块画在 `(x, y)`，`inner_width` 是气泡内宽，`height` 是 [`measure`] 量出的高度。
pub(in crate::panel::chat) fn draw(
    s: &mut Surface<'_>,
    block: &Block,
    (x, y): (f32, f32),
    inner_width: f32,
    height: f32,
    colors: &BubbleColors,
) {
    let (style, indent) = geometry(block, colors);
    let items = block_spans(block, colors);
    match block {
        Block::Bullet(_) => {
            s.circle(x + 4.0, y + style.line_height / 2.0, 2.0, colors.muted);
        }
        Block::Numbered { number, .. } => {
            s.text(x, y, &format!("{number}."), &style.colored(colors.muted));
        }
        Block::Quote(_) => {
            s.fill(Area::new(x, y + 1.0, 3.0, height - 2.0), 1.5, colors.bar);
        }
        Block::Code { .. } => {
            s.fill(Area::new(x, y, inner_width, height), 6.0, CODE_BG);
            s.rich(
                x + CODE_PAD_X,
                y + CODE_PAD_Y,
                inner_width - 2.0 * CODE_PAD_X + WRAP_SLACK,
                &items,
                &style,
            );
            return;
        }
        Block::Heading { .. } | Block::Paragraph(_) => {}
    }
    s.rich(
        x + indent,
        y,
        inner_width - indent + WRAP_SLACK,
        &items,
        &style,
    );
}
