//! 面板里一段文字的样式：字号、行高（点）、颜色、字重。

use crate::color::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelText {
    /// 字号（点）。
    pub size: f32,

    /// 行高（点）。
    pub line_height: f32,

    pub color: Color,

    /// 默认字重加粗。
    pub bold: bool,
}

impl PanelText {
    pub const fn new(size: f32, line_height: f32, color: Color) -> Self {
        Self {
            size,
            line_height,
            color,
            bold: false,
        }
    }

    /// 同样式加粗。
    pub const fn bolded(mut self) -> Self {
        self.bold = true;
        self
    }

    /// 同样式换颜色。
    pub const fn colored(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}
