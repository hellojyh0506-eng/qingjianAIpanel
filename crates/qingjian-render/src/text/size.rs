//! 一段文字量出来的尺寸；单位随调用方（候选窗内部是像素，面板的公开接口是点）。

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSize {
    pub width: f32,

    /// 单行等于所用样式的行高，换行后是行数 × 行高。
    pub height: f32,
}
