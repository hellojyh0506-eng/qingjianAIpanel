//! 富文本的一段：同一段文字共用字重与颜色，面板里用来给高亮短语加粗、给标签变色。

use crate::color::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RichSpan<'a> {
    pub text: &'a str,

    /// `Some` 覆盖整段文字的默认字重（`true` 为加粗），`None` 沿用默认。
    pub bold: Option<bool>,

    /// `Some` 覆盖这一段的颜色，`None` 沿用默认。
    pub color: Option<Color>,
}

impl<'a> RichSpan<'a> {
    /// 沿用默认样式的一段。
    pub const fn plain(text: &'a str) -> Self {
        Self {
            text,
            bold: None,
            color: None,
        }
    }

    /// 加粗的一段。
    pub const fn bold(text: &'a str) -> Self {
        Self {
            text,
            bold: Some(true),
            color: None,
        }
    }

    /// 换一种颜色。
    pub const fn colored(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_set_only_their_field() {
        let span = RichSpan::bold("hi").colored(Color::rgb(1, 2, 3));
        assert_eq!(span.bold, Some(true));
        assert_eq!(span.color, Some(Color::rgb(1, 2, 3)));
        assert_eq!(RichSpan::plain("a").bold, None);
    }
}
