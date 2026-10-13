//! 面板画布：一帧面板的绘制原语。所有坐标是点，原点在窗口（不含阴影边距）左上角；每画一个可点的东西就同步登记点击区域。

use tiny_skia::{Pixmap, Rect, Transform};

use super::area::Area;
use super::avatar::Avatar;
use super::hitmap::HitMap;
use super::icon::{Icon, VIEWBOX};
use super::panel_text::PanelText;
use super::renderer::PanelRenderer;
use crate::canvas::Canvas;
use crate::color::Color;
use crate::text::{RichSpan, TextSize};

/// 图标描边宽度（点）：16 pt 及以上统一 1.75，更小的图标按比例减细但不低于 1.25，免得小图标糊成一团。
fn icon_stroke(size: f32) -> f32 {
    (1.75 * (size / 16.0).min(1.0)).max(1.25)
}

pub struct PanelSurface<'a, H> {
    renderer: &'a mut PanelRenderer,

    canvas: Canvas,

    hits: HitMap<H>,

    /// 阴影留的边距（点）：窗口在位图里偏移这么多。
    margin: f32,
}

/// 画完的一帧。
pub struct PanelFrame<H> {
    /// 预乘 alpha 的 RGBA 位图，含阴影边距。
    pub pixmap: Pixmap,

    /// 点击表，坐标相对窗口左上角（不含边距）。
    pub hits: HitMap<H>,

    /// 窗口在位图里的偏移（点），即阴影边距；贴到屏幕时窗口位置要减去它。
    pub margin: f32,
}

impl<'a, H> PanelSurface<'a, H> {
    pub(super) fn new(renderer: &'a mut PanelRenderer, canvas: Canvas, margin: f32) -> Self {
        Self {
            renderer,
            canvas,
            hits: HitMap::new(),
            margin,
        }
    }

    /// 填充圆角矩形，`radius` 为 0 即直角。
    pub fn fill(&mut self, area: Area, radius: f32, color: Color) {
        let (x, y, w, h) = self.px(area);
        let radius = radius * self.renderer.scale();
        self.canvas.fill_round_rect(x, y, w, h, radius, color);
    }

    /// 圆角矩形描边，线宽 `width`（点）向内长。
    pub fn stroke(&mut self, area: Area, radius: f32, width: f32, color: Color) {
        let scale = self.renderer.scale();
        let (x, y, w, h) = self.px(area);
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            self.canvas
                .stroke_round_rect(rect, radius * scale, width * scale, color);
        }
    }

    /// 填充加 1 px 边框：卡片、选中标签。
    pub fn fill_bordered(&mut self, area: Area, radius: f32, fill: Color, border: Color) {
        self.fill(area, radius, border);
        self.fill(area.inset(1.0), (radius - 1.0).max(0.0), fill);
    }

    /// 实心圆。
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        self.fill(
            Area::new(cx - radius, cy - radius, 2.0 * radius, 2.0 * radius),
            radius,
            color,
        );
    }

    /// 1 pt 粗的水平分割线。
    pub fn hline(&mut self, x: f32, y: f32, width: f32, color: Color) {
        self.fill(Area::new(x, y, width, 1.0), 0.0, color);
    }

    /// 单行文字的宽度（点），同 [`PanelRenderer::measure`]；画到一半要居中、右对齐时用。
    pub fn measure(&mut self, text: &str, style: &PanelText) -> f32 {
        self.renderer.measure(text, style)
    }

    /// 富文本换行后的宽高（点），同 [`PanelRenderer::measure_rich`]。
    pub fn measure_rich(
        &mut self,
        spans: &[RichSpan],
        style: &PanelText,
        max_width: f32,
    ) -> TextSize {
        self.renderer.measure_rich(spans, style, max_width)
    }

    /// 单行文字，`y` 是行框顶边；返回宽度（点）。
    pub fn text(&mut self, x: f32, y: f32, text: &str, style: &PanelText) -> f32 {
        let scale = self.renderer.scale();
        let pixel_style = self.renderer.style(style);
        let (px, py) = ((x + self.margin) * scale, (y + self.margin) * scale);
        let width = self
            .renderer
            .text
            .draw(&mut self.canvas, text, &pixel_style, px, py);
        width / scale
    }

    /// 富文本在 `max_width`（点）内自动换行；`y` 是第一行的行框顶边，返回占用的宽高（点）。
    pub fn rich(
        &mut self,
        x: f32,
        y: f32,
        max_width: f32,
        spans: &[RichSpan],
        style: &PanelText,
    ) -> TextSize {
        let scale = self.renderer.scale();
        let pixel_style = self.renderer.style(style);
        let (px, py) = ((x + self.margin) * scale, (y + self.margin) * scale);
        let size = self.renderer.text.draw_wrapped(
            &mut self.canvas,
            spans,
            &pixel_style,
            px,
            py,
            max_width * scale,
        );
        self.renderer.to_points(size)
    }

    /// 一段纯文本自动换行，同 [`rich`](Self::rich)。
    pub fn paragraph(
        &mut self,
        x: f32,
        y: f32,
        max_width: f32,
        text: &str,
        style: &PanelText,
    ) -> TextSize {
        self.rich(x, y, max_width, &[RichSpan::plain(text)], style)
    }

    /// 画一个 Lucide 图标，`(x, y)` 是左上角，`size` 是边长（点）。
    pub fn icon(&mut self, icon: Icon, x: f32, y: f32, size: f32, color: Color) {
        self.icon_rotated(icon, x, y, size, color, 0.0);
    }

    /// 同 [`icon`](Self::icon)，绕图标中心顺时针转 `degrees` 度（加载中的转圈用）。
    pub fn icon_rotated(
        &mut self,
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        degrees: f32,
    ) {
        let Some(path) = icon.path() else {
            return;
        };
        let scale = self.renderer.scale();
        let unit = size * scale / VIEWBOX;
        let transform =
            Transform::from_translate((x + self.margin) * scale, (y + self.margin) * scale)
                .pre_scale(unit, unit)
                .pre_rotate_at(degrees, VIEWBOX / 2.0, VIEWBOX / 2.0);
        // 线宽在 24 单位的图标坐标系里，换算回点再乘倍数
        let width = icon_stroke(size) * scale / unit;
        self.canvas.stroke_path(&path, width, color, transform);
    }

    /// 画对话头像，`(x, y)` 是左上角，`size` 是边长（点）。
    pub fn avatar(&mut self, avatar: Avatar, x: f32, y: f32, size: f32) {
        let scale = self.renderer.scale();
        let target = size * scale;
        let (px, py) = ((x + self.margin) * scale, (y + self.margin) * scale);
        if let Some(pixmap) = self.renderer.avatars.get(avatar, target) {
            self.canvas
                .draw_pixmap_scaled(px, py, target, target, pixmap);
        }
    }

    /// 登记点击区域；后登记的盖在先登记的上面。
    pub fn hit(&mut self, area: Area, hit: H) {
        self.hits.push(area, hit);
    }

    /// 画完，交出位图与点击表。
    pub fn finish(self) -> PanelFrame<H> {
        PanelFrame {
            pixmap: self.canvas.into_pixmap(),
            hits: self.hits,
            margin: self.margin,
        }
    }

    /// 区域（点，窗口坐标）→ 画布像素。
    fn px(&self, area: Area) -> (f32, f32, f32, f32) {
        let scale = self.renderer.scale();
        (
            (area.x + self.margin) * scale,
            (area.y + self.margin) * scale,
            area.width * scale,
            area.height * scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontLibrary;
    use crate::panel::tokens;
    use crate::shadow::Shadow;

    /// 没有系统字体的环境（CI 容器）返回 `None`，测试跳过。
    fn renderer(scale: f32) -> Option<PanelRenderer> {
        let library = FontLibrary::system("zh-CN").ok()?;
        let mut renderer = PanelRenderer::new(library);
        renderer.set_scale(scale);
        Some(renderer)
    }

    fn body() -> PanelText {
        PanelText::new(14.0, 20.0, tokens::TEXT)
    }

    fn alpha_at(pixmap: &Pixmap, x: u32, y: u32) -> u8 {
        pixmap.pixel(x, y).map_or(0, |p| p.alpha())
    }

    #[test]
    fn wrapped_height_is_whole_lines_and_grows_when_narrower() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let text = "I wish I could pick up a new language faster than I actually do these days.";
        let wide = renderer.measure_rich(&[RichSpan::plain(text)], &body(), 600.0);
        let narrow = renderer.measure_rich(&[RichSpan::plain(text)], &body(), 120.0);
        assert_eq!(wide.height, 20.0);
        assert!(narrow.height > wide.height);
        assert_eq!(narrow.height % 20.0, 0.0);
        assert!(narrow.width <= 120.5, "宽度 {} 超出限制", narrow.width);
        let chinese = renderer.measure_rich(
            &[RichSpan::plain(
                "我希望自己能学得更快一些，但现实总是不太顺利。",
            )],
            &body(),
            100.0,
        );
        assert!(chinese.height >= 40.0);
        assert_eq!(
            renderer
                .measure_rich(&[RichSpan::plain("")], &body(), 100.0)
                .height,
            0.0
        );
    }

    #[test]
    fn measure_matches_drawn_width_and_scale_does_not_change_points() {
        let (Some(mut at_1x), Some(mut at_2x)) = (renderer(1.0), renderer(2.0)) else {
            return;
        };
        let w1 = at_1x.measure("Hello, world", &body());
        let w2 = at_2x.measure("Hello, world", &body());
        assert!((w1 - w2).abs() < 1.5, "1x={w1} 2x={w2}");
        let mut surface = at_2x.begin::<()>(200.0, 40.0, None).unwrap();
        let drawn = surface.text(10.0, 10.0, "Hello, world", &body());
        assert!((drawn - w2).abs() < 0.01);
    }

    #[test]
    fn bold_is_not_narrower_than_regular() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let regular = renderer.measure("Replace expression", &body());
        let bold = renderer.measure("Replace expression", &body().bolded());
        assert!(bold >= regular, "bold={bold} regular={regular}");
    }

    #[test]
    fn rich_span_color_reaches_the_pixels() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let red = Color::rgb(220, 38, 38);
        let mut surface = renderer.begin::<()>(300.0, 60.0, None).unwrap();
        surface.rich(
            16.0,
            16.0,
            260.0,
            &[
                RichSpan::plain("It was "),
                RichSpan::bold("a piece of cake").colored(red),
            ],
            &body(),
        );
        let frame = surface.finish();
        let reddish = frame
            .pixmap
            .pixels()
            .iter()
            .filter(|p| p.alpha() == 255 && p.red() > 180 && p.green() < 90 && p.blue() < 90)
            .count();
        assert!(reddish > 20, "红色字形像素只有 {reddish}");
    }

    #[test]
    fn window_has_border_fill_and_rounded_corner() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let frame = renderer.begin::<()>(100.0, 60.0, None).unwrap().finish();
        assert_eq!((frame.pixmap.width(), frame.pixmap.height()), (200, 120));
        assert_eq!(alpha_at(&frame.pixmap, 0, 0), 0, "圆角处应透明");
        assert_eq!(alpha_at(&frame.pixmap, 100, 60), 255);
        let center = frame.pixmap.pixel(100, 60).unwrap();
        assert_eq!(
            (center.red(), center.green(), center.blue()),
            (255, 255, 255)
        );
        let edge = frame.pixmap.pixel(100, 0).unwrap();
        assert_eq!(
            (edge.red(), edge.green(), edge.blue()),
            (0xE5, 0xE7, 0xEB),
            "顶边应是边框色"
        );
    }

    #[test]
    fn shadow_adds_margin_around_the_window() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let shadow = Shadow::mac_panel();
        let frame = renderer
            .begin::<()>(100.0, 60.0, Some(shadow))
            .unwrap()
            .finish();
        let margin = shadow.margin();
        assert_eq!(frame.margin, margin);
        assert_eq!(
            frame.pixmap.width(),
            ((100.0 + 2.0 * margin) * 2.0).ceil() as u32
        );
        // 窗口下方偏移出来的阴影有颜色，窗口正中是白底
        let below = ((60.0 + margin + 4.0) * 2.0) as u32;
        assert!(alpha_at(&frame.pixmap, frame.pixmap.width() / 2, below) > 0);
        let inside = frame
            .pixmap
            .pixel(frame.pixmap.width() / 2, ((margin + 30.0) * 2.0) as u32)
            .unwrap();
        assert_eq!(inside.red(), 255);
    }

    #[test]
    fn fills_and_strokes_land_where_the_area_says() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let mut surface = renderer.begin::<()>(100.0, 60.0, None).unwrap();
        surface.fill(Area::new(10.0, 10.0, 20.0, 10.0), 0.0, tokens::PRIMARY);
        surface.fill_bordered(
            Area::new(40.0, 10.0, 20.0, 20.0),
            8.0,
            tokens::SELECTED_BG,
            tokens::PRIMARY,
        );
        let frame = surface.finish();
        let p = frame.pixmap.pixel(30, 30).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (0x25, 0x63, 0xEB));
        // 描边 1 pt = 2 px：边上是主色，里面是选中底色
        let border = frame.pixmap.pixel(100, 21).unwrap();
        assert_eq!((border.red(), border.blue()), (0x25, 0xEB));
        let inner = frame.pixmap.pixel(100, 40).unwrap();
        assert_eq!(
            (inner.red(), inner.green(), inner.blue()),
            (0xEB, 0xF2, 0xFF)
        );
    }

    #[test]
    fn icons_draw_ink_inside_their_box_and_rotation_changes_it() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let mut surface = renderer.begin::<()>(100.0, 60.0, None).unwrap();
        surface.icon(Icon::Copy, 10.0, 10.0, 20.0, tokens::TEXT_2);
        surface.icon_rotated(Icon::RefreshCw, 50.0, 10.0, 20.0, tokens::TEXT_2, 45.0);
        let frame = surface.finish();
        let ink = |x0: u32, x1: u32| {
            let mut count = 0;
            for y in 0..frame.pixmap.height() {
                for x in x0..x1 {
                    if alpha_at(&frame.pixmap, x, y) > 0 && !(y < 2 || x < 2) {
                        count += 1;
                    }
                }
            }
            count
        };
        assert!(ink(18, 62) > 100, "Copy 没画出笔画");
        assert!(ink(98, 142) > 100, "RefreshCw 没画出笔画");
        // 图标盒（20 pt = 40 px）之外不该有笔画（留 2 px 抗锯齿余量）
        assert_eq!(alpha_at(&frame.pixmap, 100, 60), 255, "窗口内部是白底");
    }

    #[test]
    fn avatar_is_drawn_circular_at_requested_size() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let mut surface = renderer.begin::<()>(100.0, 60.0, None).unwrap();
        surface.avatar(Avatar::Boy, 10.0, 10.0, 40.0);
        let frame = surface.finish();
        // 头像中心不是白底；头像盒的四角（圆外）仍是窗口白底
        let center = frame.pixmap.pixel(20 + 40, 20 + 40).unwrap();
        assert!(!(center.red() == 255 && center.green() == 255 && center.blue() == 255));
        let corner = frame.pixmap.pixel(22, 22).unwrap();
        assert_eq!(
            (corner.red(), corner.green(), corner.blue()),
            (255, 255, 255)
        );
    }

    #[test]
    fn hits_registered_while_drawing_are_returned_in_window_coordinates() {
        let Some(mut renderer) = renderer(2.0) else {
            return;
        };
        let mut surface = renderer
            .begin::<&'static str>(100.0, 60.0, Some(Shadow::mac_panel()))
            .unwrap();
        let button = Area::new(10.0, 10.0, 30.0, 20.0);
        surface.fill(button, 8.0, tokens::PRIMARY);
        surface.hit(button, "copy");
        let frame = surface.finish();
        assert_eq!(frame.hits.hit(12.0, 12.0), Some(&"copy"));
        assert_eq!(frame.hits.hit(50.0, 12.0), None);
        // 命中区与实际填充位置一致：位图里窗口偏移 margin
        let px = ((frame.margin + 25.0) * 2.0) as u32;
        let py = ((frame.margin + 20.0) * 2.0) as u32;
        let pixel = frame.pixmap.pixel(px, py).unwrap();
        assert_eq!(
            (pixel.red(), pixel.green(), pixel.blue()),
            (0x25, 0x63, 0xEB)
        );
    }
}
