//! 面板渲染器：持有字体与头像缓存，量文字、开一张画布。一个面板窗口一个，跨帧复用。
//!
//! 坐标与尺寸对外一律是点（逻辑像素），内部乘 [`PanelRenderer::set_scale`] 的倍数换成像素；量文字也按这个倍数排版再除回来，
//! 所以布局阶段量出的宽高与绘制时一致。

use tiny_skia::Rect;

use super::avatar_cache::AvatarCache;
use super::panel_text::PanelText;
use super::surface::PanelSurface;
use super::tokens;
use crate::canvas::Canvas;
use crate::error::RenderError;
use crate::fonts::FontLibrary;
use crate::shadow::Shadow;
use crate::text::{RichSpan, TextPainter, TextSize, TextStyle};

/// 文字覆盖率 gamma：与浅色候选窗一致。
const TEXT_GAMMA: f32 = 0.85;

pub struct PanelRenderer {
    pub(super) text: TextPainter,

    pub(super) avatars: AvatarCache,

    /// 点 → 像素倍数。
    scale: f32,
}

impl PanelRenderer {
    pub fn new(library: FontLibrary) -> Self {
        Self {
            text: TextPainter::new(library),
            avatars: AvatarCache::default(),
            scale: 1.0,
        }
    }

    /// 设置点 → 像素倍数（100% 缩放为 1，150% 为 1.5，Retina 为 2）。
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(0.5, 8.0);
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// 单行文字的宽度（点）。
    pub fn measure(&mut self, text: &str, style: &PanelText) -> f32 {
        let size = self.text.measure(text, &self.style(style));
        size.width / self.scale
    }

    /// 富文本在 `max_width`（点）内自动换行后的宽高（点），高度是行数 × 行高；空文本高度为 0。
    pub fn measure_rich(
        &mut self,
        spans: &[RichSpan],
        style: &PanelText,
        max_width: f32,
    ) -> TextSize {
        let size = self
            .text
            .measure_wrapped(spans, &self.style(style), max_width * self.scale);
        self.to_points(size)
    }

    /// 开一张 `width × height`（点）的面板画布：白底、1 px 边框、圆角；`shadow` 给了就在四周留出阴影边距。
    /// 之后的绘制坐标原点是窗口（不含阴影边距）的左上角。
    pub fn begin<H>(
        &mut self,
        width: f32,
        height: f32,
        shadow: Option<Shadow>,
    ) -> Result<PanelSurface<'_, H>, RenderError> {
        let scale = self.scale;
        let margin = shadow.map_or(0.0, |s| s.margin());
        let pixel_width = ((width + 2.0 * margin) * scale).ceil() as u32;
        let pixel_height = ((height + 2.0 * margin) * scale).ceil() as u32;
        let mut canvas = Canvas::new(pixel_width, pixel_height)?;
        let (x, y) = (margin * scale, margin * scale);
        let (w, h) = (width * scale, height * scale);
        let radius = tokens::WINDOW_RADIUS * scale;
        let content = Rect::from_xywh(x, y, w, h);
        if let (Some(shadow), Some(content)) = (shadow, content) {
            shadow.paint(&mut canvas, content, radius, scale);
        }
        canvas.fill_round_rect(x, y, w, h, radius, tokens::WINDOW_BG);
        if let Some(content) = content {
            canvas.stroke_round_rect(content, radius, scale, tokens::BORDER);
        }
        Ok(PanelSurface::new(self, canvas, margin))
    }

    /// 面板文字样式 → 渲染器内部的像素样式。
    pub(super) fn style(&self, style: &PanelText) -> TextStyle {
        let mut out = TextStyle::new(
            crate::theme::FontSpec::new(style.size * self.scale, style.line_height * self.scale),
            style.size,
            style.color,
            TEXT_GAMMA,
        );
        out.bold = style.bold;
        out
    }

    pub(super) fn to_points(&self, size: TextSize) -> TextSize {
        TextSize {
            width: size.width / self.scale,
            height: size.height / self.scale,
        }
    }
}
