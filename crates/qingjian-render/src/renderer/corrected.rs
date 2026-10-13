//! 「纠」标记：候选是按纠正后的拼音转出来的，词后面画一枚小胶囊告诉用户这条是输入法替他改的。

use super::{Metrics, Renderer};
use crate::canvas::Canvas;
use crate::color::Color;
use crate::frame::Row;

/// 胶囊里的字。
const LABEL: &str = "纠";

/// 胶囊底色（`#FEF3C7`）。
const BACKGROUND: Color = Color::rgb(0xFE, 0xF3, 0xC7);

/// 胶囊字色（`#B45309`）。
const FOREGROUND: Color = Color::rgb(0xB4, 0x53, 0x09);

/// 胶囊与前面候选词（或辅码）的间距（点）。
const GAP: f32 = 4.0;

/// 字左右各留多少（点）。
const PADDING_X: f32 = 4.0;

/// 字上下各留多少（点）。
const PADDING_Y: f32 = 1.0;

/// 胶囊圆角（点）。
const RADIUS: f32 = 4.0;

impl Renderer {
    /// 「纠」标记占的宽度（含前面的间距）；这一行不是纠正出来的就是 0。
    pub(super) fn corrected_width(&mut self, row: &Row, m: &Metrics) -> f32 {
        if !row.corrected {
            return 0.0;
        }
        let style = m.annotation_style(FOREGROUND);
        m.px(GAP + PADDING_X * 2.0) + self.measure(LABEL, &style).width
    }

    /// 在 `x` 处画「纠」胶囊，竖直方向在候选词那一行（`top`、`text_height`）里居中。
    pub(super) fn draw_corrected(
        &mut self,
        canvas: &mut Canvas,
        m: &Metrics,
        x: f32,
        top: f32,
        text_height: f32,
    ) {
        let style = m.annotation_style(FOREGROUND);
        let label = self.measure(LABEL, &style);
        let width = label.width + m.px(PADDING_X * 2.0);
        let height = label.height + m.px(PADDING_Y * 2.0);
        let capsule_x = x + m.px(GAP);
        let capsule_y = top + (text_height - height) / 2.0;
        canvas.fill_round_rect(
            capsule_x,
            capsule_y,
            width,
            height,
            m.px(RADIUS),
            BACKGROUND,
        );
        self.draw_text(
            canvas,
            LABEL,
            &style,
            capsule_x + m.px(PADDING_X),
            capsule_y + m.px(PADDING_Y),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::fonts::FontLibrary;
    use crate::frame::{Frame, Row};
    use crate::layout::Layout;
    use crate::renderer::Renderer;
    use crate::theme::Theme;

    fn frame(corrected: bool) -> Frame {
        let mut row = Row::plain(0, "今天我们一起去楼下的小饭馆吃饭好不好");
        row.corrected = corrected;
        Frame {
            rows: vec![row, Row::plain(1, "今天我们")],
            ..Default::default()
        }
    }

    #[test]
    fn corrected_rows_grow_wider_and_paint_the_capsule_color() {
        // 没有系统字体的环境（CI 容器）跳过
        let Ok(library) = FontLibrary::system("zh-CN") else {
            return;
        };
        let mut renderer = Renderer::new(library);
        let theme = Theme::light();
        for layout in [Layout::Horizontal, Layout::Vertical] {
            let plain = renderer
                .render(&frame(false), layout, &theme, 2.0, None)
                .unwrap();
            let marked = renderer
                .render(&frame(true), layout, &theme, 2.0, None)
                .unwrap();
            assert!(marked.content_width > plain.content_width, "{layout:?}");
            let capsule =
                |rendered: &crate::renderer::Rendered| {
                    rendered.pixmap.pixels().iter().any(|pixel| {
                        (pixel.red(), pixel.green(), pixel.blue()) == (0xFE, 0xF3, 0xC7)
                    })
                };
            assert!(capsule(&marked), "{layout:?}");
            assert!(!capsule(&plain), "{layout:?}");
        }
    }
}
