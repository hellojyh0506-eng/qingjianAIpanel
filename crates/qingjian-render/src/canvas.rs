//! 位图画布：tiny-skia `Pixmap` 之上的几个填充原语，加字形位图的逐像素 source-over 混合。坐标一律是像素、左上角原点。

use tiny_skia::{
    BlendMode, FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, PremultipliedColorU8, Rect, Stroke, Transform,
};

use crate::color::{Color, mul_u8, premultiply};
use crate::error::RenderError;

pub(crate) struct Canvas {
    /// 预乘 RGBA。
    pixmap: Pixmap,

    /// 裁剪区域：之后的所有绘制只落在它里面；`None` 为不裁剪。
    clip: Option<Clip>,
}

/// 矩形裁剪：像素级混合直接比边界，路径与位图走 tiny-skia 的遮罩。
struct Clip {
    mask: Mask,

    /// 左、上、右、下（像素，右下不含）。
    bounds: (i32, i32, i32, i32),
}

impl Canvas {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        Pixmap::new(width, height)
            .map(|pixmap| Self { pixmap, clip: None })
            .ok_or(RenderError::InvalidSize { width, height })
    }

    pub(crate) fn from_pixmap(pixmap: Pixmap) -> Self {
        Self { pixmap, clip: None }
    }

    pub(crate) fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub(crate) fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    /// 设置（或取消）矩形裁剪，坐标是像素；取整到像素边界。
    pub(crate) fn set_clip(&mut self, rect: Option<(f32, f32, f32, f32)>) {
        self.clip = rect.and_then(|(x, y, w, h)| {
            let left = (x.floor() as i32).max(0);
            let top = (y.floor() as i32).max(0);
            let right = ((x + w).ceil() as i32).min(self.pixmap.width() as i32);
            let bottom = ((y + h).ceil() as i32).min(self.pixmap.height() as i32);
            let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
            let area = Rect::from_ltrb(left as f32, top as f32, right as f32, bottom as f32)?;
            mask.fill_path(
                &PathBuilder::from_rect(area),
                FillRule::Winding,
                false,
                Transform::identity(),
            );
            Some(Clip {
                mask,
                bounds: (left, top, right, bottom),
            })
        });
    }

    pub(crate) fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        let Some(rect) = Rect::from_xywh(x, y, width, height) else {
            return;
        };
        self.pixmap.fill_rect(
            rect,
            &paint(color, BlendMode::SourceOver),
            Transform::identity(),
            self.clip.as_ref().map(|clip| &clip.mask),
        );
    }

    pub(crate) fn fill_round_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: Color,
    ) {
        if let Some(path) = round_rect(x, y, width, height, radius) {
            self.fill_path(&path, color, BlendMode::SourceOver);
        }
    }

    pub(crate) fn fill_path(&mut self, path: &Path, color: Color, blend: BlendMode) {
        self.pixmap.fill_path(
            path,
            &paint(color, blend),
            FillRule::Winding,
            Transform::identity(),
            self.clip.as_ref().map(|clip| &clip.mask),
        );
    }

    /// 描边路径：圆头圆角，`width` 是路径坐标系里的线宽（经 `transform` 缩放后才是像素）。
    pub(crate) fn stroke_path(
        &mut self,
        path: &Path,
        width: f32,
        color: Color,
        transform: Transform,
    ) {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            path,
            &paint(color, BlendMode::SourceOver),
            &stroke,
            transform,
            self.clip.as_ref().map(|clip| &clip.mask),
        );
    }

    /// 圆角矩形描边，线宽向矩形内侧长（外轮廓与同参数的 `fill_round_rect` 重合）。
    pub(crate) fn stroke_round_rect(
        &mut self,
        rect: Rect,
        radius: f32,
        line_width: f32,
        color: Color,
    ) {
        let half = line_width / 2.0;
        if let Some(path) = round_rect(
            rect.x() + half,
            rect.y() + half,
            rect.width() - line_width,
            rect.height() - line_width,
            (radius - half).max(0.0),
        ) {
            self.stroke_path(&path, line_width, color, Transform::identity());
        }
    }

    /// 把位图缩放到 `width × height` 贴到 `(x, y)`，双三次插值；用于头像这种按 DPI 选图后的微调缩放。
    pub(crate) fn draw_pixmap_scaled(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        source: &Pixmap,
    ) {
        if source.width() == 0 || source.height() == 0 || width <= 0.0 || height <= 0.0 {
            return;
        }
        let transform = Transform::from_translate(x, y).pre_scale(
            width / source.width() as f32,
            height / source.height() as f32,
        );
        let paint = PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &paint,
            transform,
            self.clip.as_ref().map(|clip| &clip.mask),
        );
    }

    /// 把另一张位图整张叠上来（左上角对齐到 `(x, y)`）。
    pub(crate) fn blend_pixmap(&mut self, x: i32, y: i32, other: &Pixmap) {
        let width = other.width();
        for (i, pixel) in other.pixels().iter().enumerate() {
            if pixel.alpha() == 0 {
                continue;
            }
            let px = x + (i as u32 % width) as i32;
            let py = y + (i as u32 / width) as i32;
            self.blend_pixel(px, py, *pixel);
        }
    }

    /// 8 位覆盖率遮罩（普通字形）按颜色叠上来。
    pub(crate) fn blend_mask(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        data: &[u8],
        color: Color,
    ) {
        for row in 0..height {
            for col in 0..width {
                let Some(&coverage) = data.get((row * width + col) as usize) else {
                    return;
                };
                if coverage == 0 {
                    continue;
                }
                self.blend_pixel(
                    x + col as i32,
                    y + row as i32,
                    color.premultiplied(coverage),
                );
            }
        }
    }

    /// 直通 RGBA 位图（彩色 emoji）叠上来。
    pub(crate) fn blend_rgba(&mut self, x: i32, y: i32, width: u32, height: u32, data: &[u8]) {
        for (i, px) in data.chunks_exact(4).enumerate() {
            if px[3] == 0 {
                continue;
            }
            let col = (i as u32 % width) as i32;
            let row = (i as u32 / width) as i32;
            if row >= height as i32 {
                return;
            }
            self.blend_pixel(x + col, y + row, premultiply(px[0], px[1], px[2], px[3]));
        }
    }

    /// source-over：`dst = src + dst × (1 − src.a)`。
    fn blend_pixel(&mut self, x: i32, y: i32, src: PremultipliedColorU8) {
        let (left, top, right, bottom) = self.clip.as_ref().map_or(
            (
                0,
                0,
                self.pixmap.width() as i32,
                self.pixmap.height() as i32,
            ),
            |clip| clip.bounds,
        );
        if x < left || y < top || x >= right || y >= bottom {
            return;
        }
        let index = y as usize * self.pixmap.width() as usize + x as usize;
        let dst = &mut self.pixmap.pixels_mut()[index];
        let inverse = 255 - src.alpha();
        let a = src.alpha().saturating_add(mul_u8(dst.alpha(), inverse));
        let channel = |s: u8, d: u8| s.saturating_add(mul_u8(d, inverse)).min(a);
        let r = channel(src.red(), dst.red());
        let g = channel(src.green(), dst.green());
        let b = channel(src.blue(), dst.blue());
        if let Some(out) = PremultipliedColorU8::from_rgba(r, g, b, a) {
            *dst = out;
        }
    }
}

fn paint(color: Color, blend: BlendMode) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color.to_skia());
    paint.anti_alias = true;
    paint.blend_mode = blend;
    paint
}

/// 圆角矩形路径；圆角用三次贝塞尔近似四分之一圆。
pub(crate) fn round_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    let r = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    if r <= 0.0 {
        return Rect::from_xywh(x, y, width, height).map(PathBuilder::from_rect);
    }
    // 四分之一圆的贝塞尔控制点系数
    const KAPPA: f32 = 0.552_284_8;
    let k = r * KAPPA;
    let (right, bottom) = (x + width, y + height);
    let mut path = PathBuilder::new();
    path.move_to(x + r, y);
    path.line_to(right - r, y);
    path.cubic_to(right - r + k, y, right, y + r - k, right, y + r);
    path.line_to(right, bottom - r);
    path.cubic_to(
        right,
        bottom - r + k,
        right - r + k,
        bottom,
        right - r,
        bottom,
    );
    path.line_to(x + r, bottom);
    path.cubic_to(x + r - k, bottom, x, bottom - r + k, x, bottom - r);
    path.line_to(x, y + r);
    path.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    path.close();
    path.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_source_over_and_clips_to_bounds() {
        let mut canvas = Canvas::new(2, 1).unwrap();
        canvas.fill_rect(0.0, 0.0, 2.0, 1.0, Color::rgb(0, 0, 255));
        canvas.blend_mask(0, 0, 1, 1, &[255], Color::rgb(255, 0, 0));
        canvas.blend_mask(5, 5, 1, 1, &[255], Color::rgb(255, 0, 0));
        let pixmap = canvas.into_pixmap();
        let red = pixmap.pixel(0, 0).unwrap();
        assert_eq!((red.red(), red.blue(), red.alpha()), (255, 0, 255));
        let blue = pixmap.pixel(1, 0).unwrap();
        assert_eq!((blue.red(), blue.blue()), (0, 255));
    }

    #[test]
    fn half_alpha_over_white_lightens() {
        let mut canvas = Canvas::new(1, 1).unwrap();
        canvas.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgb(255, 255, 255));
        canvas.blend_mask(0, 0, 1, 1, &[255], Color::gray(0, 128));
        let px = canvas.into_pixmap().pixel(0, 0).unwrap();
        assert_eq!(px.alpha(), 255);
        assert!((px.red() as i32 - 127).abs() <= 1, "got {}", px.red());
    }
}
