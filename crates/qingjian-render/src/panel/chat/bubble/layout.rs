//! 气泡排版：量出宽高与各部分高度，结果缓存在消息上，文字或宽度不变就不重量。

use super::super::message::ChatMessage;
use super::super::role::Role;
use super::super::style::{BLOCK_GAP, BUBBLE_PAD_X, BUBBLE_PAD_Y, CAPTION};
use super::super::widgets::Surface;
use super::blocks;
use super::colors::BubbleColors;

/// 「思考过程」折叠行的高度与展开后文字框的内边距。
pub(super) const REASONING_ROW: f32 = 20.0;
pub(super) const REASONING_PAD: f32 = 8.0;

/// 助手气泡底部（复制键）一行的高度。
pub(super) const FOOTER_HEIGHT: f32 = 20.0;

/// 「思考过程 ›」一行至少要这么宽。
const REASONING_MIN_WIDTH: f32 = 96.0;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::panel::chat) struct BubbleLayout {
    /// 缓存键：最大宽度、缩放、是否正在流式（影响页脚）。
    key: (u32, u32, bool),

    pub width: f32,

    pub height: f32,

    /// 每个 Markdown 块的高度。
    pub blocks: Vec<f32>,

    /// 展开的思考过程文字高度；折叠时为 0。
    pub reasoning_text: f32,

    /// 来源说明文字高度；没有来源为 0。
    pub sources: f32,

    pub footer: bool,
}

/// 保证 `message.layout` 是 `max_width`（点）下的最新排版。
pub(in crate::panel::chat) fn ensure(
    s: &mut Surface<'_>,
    message: &mut ChatMessage,
    max_width: f32,
    scale: f32,
    live: bool,
) {
    let key = (max_width.to_bits(), scale.to_bits(), live);
    if message
        .layout
        .as_ref()
        .is_some_and(|layout| layout.key == key)
    {
        return;
    }
    message.layout = Some(measure(s, message, max_width, key, live));
}

fn measure(
    s: &mut Surface<'_>,
    message: &ChatMessage,
    max_width: f32,
    key: (u32, u32, bool),
    live: bool,
) -> BubbleLayout {
    let colors = BubbleColors::of(message.role);
    let inner_max = max_width - 2.0 * BUBBLE_PAD_X;
    let mut inner = 0.0f32;
    let mut height = 2.0 * BUBBLE_PAD_Y;
    let mut parts = 0;
    let mut add = |part: f32, height: &mut f32| {
        if parts > 0 {
            *height += BLOCK_GAP;
        }
        *height += part;
        parts += 1;
    };

    let mut reasoning_text = 0.0;
    if !message.reasoning().is_empty() {
        let mut row = REASONING_ROW;
        inner = inner.max(REASONING_MIN_WIDTH);
        if message.reasoning_open {
            let size = s.measure_rich(
                &[crate::text::RichSpan::plain(message.reasoning())],
                &CAPTION,
                inner_max - 2.0 * REASONING_PAD,
            );
            reasoning_text = size.height;
            row += REASONING_PAD * 2.0 + size.height;
            inner = inner.max(size.width + 2.0 * REASONING_PAD);
        }
        add(row, &mut height);
    }

    let mut block_heights = Vec::with_capacity(message.blocks().len());
    for block in message.blocks() {
        let (width, block_height) = blocks::measure(s, block, inner_max, &colors);
        inner = inner.max(width);
        block_heights.push(block_height);
        add(block_height, &mut height);
    }

    let mut sources = 0.0;
    if !message.sources.is_empty() {
        let text = format!("来源：\n{}", message.sources.join("\n"));
        let size = s.measure_rich(&[crate::text::RichSpan::plain(&text)], &CAPTION, inner_max);
        sources = size.height;
        inner = inner.max(size.width);
        add(sources, &mut height);
    }

    let footer = message.role == Role::Assistant && !live && !message.text().is_empty();
    if footer {
        inner = inner.max(40.0);
        add(FOOTER_HEIGHT, &mut height);
    }

    BubbleLayout {
        key,
        width: inner.min(inner_max) + 2.0 * BUBBLE_PAD_X,
        height,
        blocks: block_heights,
        reasoning_text,
        sources,
        footer,
    }
}
