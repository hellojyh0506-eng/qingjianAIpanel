//! 文字测绘：cosmic-text 整形 + swash 栅格，单行、像素坐标；普通字形走覆盖率遮罩，彩色 emoji 走 RGBA 位图。

mod size;
mod span;
mod style;

use std::collections::HashMap;

use cosmic_text::fontdb::ID;
use cosmic_text::{
    Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight, Wrap,
};

use crate::canvas::Canvas;
use crate::color::Color;
use crate::fonts::{FontLibrary, Trak, UI_FAMILY};

pub use span::RichSpan;
pub use size::TextSize;
pub(crate) use style::TextStyle;

pub(crate) struct TextPainter {
    /// 字体库与回退链。
    font_system: FontSystem,

    /// 字形位图缓存（按字体、字号、亚像素位移）。
    cache: SwashCache,

    /// 复用的单行缓冲。
    buffer: Buffer,

    /// 每张字体的字距表（没有的记 `None`），按字形所用字体各查各的，与 CoreText 一致。
    tracking: HashMap<ID, Option<Trak>>,

    /// 覆盖率 gamma 查找表，按 gamma 值缓存。
    gamma_tables: HashMap<u32, Box<[u8; 256]>>,
}

impl TextPainter {
    pub(crate) fn new(library: FontLibrary) -> Self {
        let mut font_system = library.into_font_system();
        let buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 19.0));
        Self {
            font_system,
            cache: SwashCache::new(),
            buffer,
            tracking: HashMap::new(),
            gamma_tables: HashMap::new(),
        }
    }

    /// 光学字号（点）：SF 这类带 `opsz` 轴的字体在小字号用文本视觉尺寸，CoreText 对系统字体自动做，这里要显式给。
    /// 现在是整个画笔一个值（cosmic-text 的字体实例缓存没按它分键），候选窗几种字号都在 20 pt 以下，落到同一档。
    pub(crate) fn set_optical_size(&mut self, points: Option<f32>) {
        self.font_system.set_optical_size(points);
    }

    /// 一段文字的宽高（像素）。高度就是行高。
    pub(crate) fn measure(&mut self, text: &str, style: &TextStyle) -> TextSize {
        self.shape(text, style);
        let size = self.measure_laid_out(style);
        TextSize {
            width: size.width,
            height: style.line_height,
        }
    }

    /// 把文字画到 `(x, y)`，`y` 是行框顶边。返回宽度。
    pub(crate) fn draw(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        style: &TextStyle,
        x: f32,
        y: f32,
    ) -> f32 {
        self.shape(text, style);
        self.paint(canvas, style, x, y).width
    }

    /// 富文本在 `max_width`（像素）内自动换行后的宽高；加粗、变色按 span 分段。
    pub(crate) fn measure_wrapped(
        &mut self,
        spans: &[RichSpan],
        style: &TextStyle,
        max_width: f32,
    ) -> TextSize {
        self.layout(spans, style, Some(max_width));
        self.measure_laid_out(style)
    }

    /// 富文本自动换行后画到 `(x, y)`（`y` 是第一行的行框顶边），返回占用的宽高。
    pub(crate) fn draw_wrapped(
        &mut self,
        canvas: &mut Canvas,
        spans: &[RichSpan],
        style: &TextStyle,
        x: f32,
        y: f32,
        max_width: f32,
    ) -> TextSize {
        self.layout(spans, style, Some(max_width));
        self.paint(canvas, style, x, y)
    }

    /// 每个字形用的字族名（相邻相同的合并），拿来核对中日字形与 emoji 回退到了哪家字体。
    pub(crate) fn trace_families(&mut self, text: &str, style: &TextStyle) -> Vec<String> {
        self.shape(text, style);
        let mut names: Vec<String> = Vec::new();
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                let name = self
                    .font_system
                    .db()
                    .face(glyph.font_id)
                    .and_then(|face| face.families.first().map(|(n, _)| n.clone()))
                    .unwrap_or_else(|| "?".to_owned());
                if names.last() != Some(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    fn shape(&mut self, text: &str, style: &TextStyle) {
        self.layout(&[RichSpan::plain(text)], style, None);
    }

    /// 把 span 排进缓冲；`max_width` 为 `None` 时单行不换行。
    fn layout(&mut self, spans: &[RichSpan], style: &TextStyle, max_width: Option<f32>) {
        let base = Attrs::new()
            .family(UI_FAMILY)
            .weight(weight(style.bold))
            .color(style.color.to_cosmic());
        self.buffer
            .set_metrics(Metrics::new(style.size, style.line_height));
        self.buffer.set_wrap(if max_width.is_some() {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        });
        self.buffer.set_size(max_width, None);
        self.buffer.set_rich_text(
            spans.iter().map(|span| {
                let mut attrs = base.clone();
                if let Some(bold) = span.bold {
                    attrs = attrs.weight(weight(bold));
                }
                if let Some(color) = span.color {
                    attrs = attrs.color(color.to_cosmic());
                }
                (span.text, attrs)
            }),
            &base,
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// 量缓冲里已排好的内容：最宽一行（含字距）与有字的最后一行的底边；没有字形（空文本）高度为 0。
    fn measure_laid_out(&mut self, style: &TextStyle) -> TextSize {
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for run in self.buffer.layout_runs() {
            if run.glyphs.is_empty() {
                continue;
            }
            let tracked: f32 = run
                .glyphs
                .iter()
                .map(|glyph| {
                    tracking_px(&self.font_system, &mut self.tracking, glyph.font_id, style)
                })
                .sum();
            width = width.max(run.line_w + tracked);
            height = height.max(run.line_top + run.line_height);
        }
        TextSize { width, height }
    }

    /// 把缓冲里已排好的内容画到 `(x, y)`，返回占用的宽高。
    fn paint(&mut self, canvas: &mut Canvas, style: &TextStyle, x: f32, y: f32) -> TextSize {
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for run in self.buffer.layout_runs() {
            if !run.glyphs.is_empty() {
                height = height.max(run.line_top + run.line_height);
            }
            let baseline = y + run.line_y;
            // 每个字形画完把它那份字距累加到后面所有字形的 x 上
            let mut tracked = 0.0_f32;
            for glyph in run.glyphs {
                let physical = glyph.physical((x + tracked, y), 1.0);
                tracked += tracking_px(&self.font_system, &mut self.tracking, glyph.font_id, style);
                let Some(image) = self
                    .cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                let gx = physical.x + image.placement.left;
                let gy = run.line_y.round() as i32 + physical.y - image.placement.top;
                let (w, h) = (image.placement.width, image.placement.height);
                match image.content {
                    SwashContent::Mask => {
                        let table = gamma_table(&mut self.gamma_tables, style.gamma);
                        let data: Vec<u8> = image.data.iter().map(|&c| table[c as usize]).collect();
                        let color = glyph.color_opt.map_or(style.color, Color::from_cosmic);
                        canvas.blend_mask(gx, gy, w, h, &data, color);
                    }
                    SwashContent::Color => canvas.blend_rgba(gx, gy, w, h, &image.data),
                    // 没有申请亚像素格式，不会出现
                    SwashContent::SubpixelMask => {}
                }
            }
            let line_w = run.line_w + tracked;
            width = width.max(line_w);
            let thickness = (style.size / 14.0).max(1.0);
            if style.strike {
                // 删除线穿过小写字母中部
                let line_y = baseline - style.size * 0.3;
                canvas.fill_rect(x, line_y, line_w, thickness, style.color);
            }
            if style.underline {
                // 下划线压在基线下面一点，与删除线同一套粗细
                let line_y = baseline + style.size * 0.14;
                canvas.fill_rect(x, line_y, line_w, thickness, style.color);
            }
        }
        TextSize { width, height }
    }
}

fn weight(bold: bool) -> Weight {
    if bold { Weight::BOLD } else { Weight::NORMAL }
}

/// 某张字体在这个字号下每个字形要加的间距（像素）；第一次用到时解析它的 `trak` 表。
fn tracking_px(
    font_system: &FontSystem,
    cache: &mut HashMap<ID, Option<Trak>>,
    font_id: ID,
    style: &TextStyle,
) -> f32 {
    let trak = cache.entry(font_id).or_insert_with(|| {
        font_system
            .db()
            .with_face_data(font_id, Trak::parse)
            .flatten()
    });
    trak.as_ref()
        .map_or(0.0, |trak| trak.tracking_em(style.points) * style.size)
}

/// 覆盖率 → gamma 校正后的覆盖率。
fn gamma_table(cache: &mut HashMap<u32, Box<[u8; 256]>>, gamma: f32) -> &[u8; 256] {
    cache.entry(gamma.to_bits()).or_insert_with(|| {
        let mut table = [0u8; 256];
        for (i, out) in table.iter_mut().enumerate() {
            *out = ((i as f32 / 255.0).powf(gamma) * 255.0).round() as u8;
        }
        Box::new(table)
    })
}
