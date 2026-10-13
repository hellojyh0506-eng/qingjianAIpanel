//! 画一个气泡：底色、思考过程折叠行、Markdown 块、来源、复制键。

use super::super::hit::ChatHit;
use super::super::message::ChatMessage;
use super::super::style::{BLOCK_GAP, BUBBLE_PAD_X, BUBBLE_PAD_Y, BUBBLE_RADIUS, CAPTION};
use super::super::widgets::Surface;
use super::blocks;
use super::colors::BubbleColors;
use super::layout::{FOOTER_HEIGHT, REASONING_PAD, REASONING_ROW};
use crate::panel::{Area, Icon, tokens};
use crate::text::RichSpan;

/// `message` 的排版必须已经由 [`ensure`](super::layout::ensure) 算好；`(x, y)` 是气泡左上角。
pub(in crate::panel::chat) fn draw(
    s: &mut Surface<'_>,
    message: &ChatMessage,
    index: usize,
    (x, y): (f32, f32),
    live: bool,
) {
    let Some(layout) = &message.layout else {
        return;
    };
    let colors = BubbleColors::of(message.role);
    let area = Area::new(x, y, layout.width, layout.height);
    match colors.border {
        Some(border) => s.fill_bordered(area, BUBBLE_RADIUS, colors.fill, border),
        None => s.fill(area, BUBBLE_RADIUS, colors.fill),
    }
    let inner_x = x + BUBBLE_PAD_X;
    let inner_width = layout.width - 2.0 * BUBBLE_PAD_X;
    let mut cursor = y + BUBBLE_PAD_Y;
    let mut first = true;
    let mut next = |height: f32, cursor: &mut f32| -> f32 {
        if !first {
            *cursor += BLOCK_GAP;
        }
        first = false;
        let top = *cursor;
        *cursor += height;
        top
    };

    if !message.reasoning().is_empty() {
        let mut row = REASONING_ROW;
        if message.reasoning_open {
            row += 2.0 * REASONING_PAD + layout.reasoning_text;
        }
        let top = next(row, &mut cursor);
        let label = if live && message.text().is_empty() {
            "思考中…"
        } else {
            "思考过程"
        };
        let style = CAPTION.colored(colors.muted);
        let width = s.text(inner_x, top + 2.5, label, &style);
        let chevron = if message.reasoning_open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        s.icon(
            chevron,
            inner_x + width + 2.0,
            top + 3.0,
            14.0,
            colors.muted,
        );
        s.hit(
            Area::new(inner_x - 4.0, top, width + 26.0, REASONING_ROW),
            ChatHit::ToggleReasoning(index),
        );
        if message.reasoning_open {
            let box_area = Area::new(
                inner_x,
                top + REASONING_ROW,
                inner_width,
                layout.reasoning_text + 2.0 * REASONING_PAD,
            );
            s.fill(box_area, 6.0, tokens::TABBAR_BG);
            s.rich(
                box_area.x + REASONING_PAD,
                box_area.y + REASONING_PAD,
                inner_width - 2.0 * REASONING_PAD + 0.5,
                &[RichSpan::plain(message.reasoning())],
                &CAPTION,
            );
        }
    }

    for (block, &height) in message.blocks().iter().zip(&layout.blocks) {
        let top = next(height, &mut cursor);
        blocks::draw(s, block, (inner_x, top), inner_width, height, &colors);
    }

    if layout.sources > 0.0 {
        let top = next(layout.sources, &mut cursor);
        let text = format!("来源：\n{}", message.sources.join("\n"));
        s.rich(
            inner_x,
            top,
            inner_width + 0.5,
            &[RichSpan::plain(&text)],
            &CAPTION,
        );
    }

    if layout.footer {
        let top = next(FOOTER_HEIGHT, &mut cursor);
        let button = Area::new(
            x + layout.width - BUBBLE_PAD_X - 24.0,
            top - 2.0,
            24.0,
            24.0,
        );
        s.icon(
            Icon::Copy,
            button.x + 5.0,
            button.y + 5.0,
            14.0,
            tokens::TEXT_2,
        );
        s.hit(button, ChatHit::CopyMessage(index));
    }
}
