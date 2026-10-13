//! 块级 Markdown 元素。

use super::inline::Inline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `#` / `##` / `###`，`level` 为 1 到 3。
    Heading {
        level: u8,
        spans: Vec<Inline>,
    },

    /// `- ` 或 `* ` 开头的列表项。
    Bullet(Vec<Inline>),

    /// `1. ` 开头的列表项，保留原编号。
    Numbered {
        number: usize,
        spans: Vec<Inline>,
    },

    /// `> ` 开头的引用。
    Quote(Vec<Inline>),

    /// 围栏代码块：原样保留换行与缩进。
    Code {
        text: String,
    },

    Paragraph(Vec<Inline>),
}
