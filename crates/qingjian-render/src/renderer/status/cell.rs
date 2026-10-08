//! 状态条的一格。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusCell {
    /// 一段文字；`emphasized` 用品牌色（当前模式、生效中的全角标点），否则用译文的灰。
    Text { text: String, emphasized: bool },

    /// 指定颜色的文字（如训练模式下的高亮橙色）。
    ColoredText { text: String, color: crate::color::Color },

    /// 圆角高亮徽标（如 Learn 模式的蓝底白字、Drill 模式的橙底白字）。
    Badge {
        text: String,
        bg_color: crate::color::Color,
        text_color: crate::color::Color,
    },

    /// 打开设置的齿轮。
    Gear,
}

impl StatusCell {
    pub fn text(text: impl Into<String>, emphasized: bool) -> Self {
        Self::Text {
            text: text.into(),
            emphasized,
        }
    }

    pub fn colored(text: impl Into<String>, color: crate::color::Color) -> Self {
        Self::ColoredText {
            text: text.into(),
            color,
        }
    }

    pub fn badge(
        text: impl Into<String>,
        bg_color: crate::color::Color,
        text_color: crate::color::Color,
    ) -> Self {
        Self::Badge {
            text: text.into(),
            bg_color,
            text_color,
        }
    }
}

