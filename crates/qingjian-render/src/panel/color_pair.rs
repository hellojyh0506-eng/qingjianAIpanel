//! 底色 + 字色的一对：标签、胶囊这类「浅底深字」控件用。

use crate::color::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPair {
    /// 背景。
    pub bg: Color,

    /// 文字。
    pub fg: Color,
}

impl ColorPair {
    pub const fn new(bg: Color, fg: Color) -> Self {
        Self { bg, fg }
    }
}
