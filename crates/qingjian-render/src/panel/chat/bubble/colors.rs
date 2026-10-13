//! 气泡配色：用户、助手、错误三种。

use super::super::role::Role;
use super::super::style::{CODE_INLINE, QUOTE_BAR, TEXT_MID};
use crate::color::Color;
use crate::panel::tokens;

#[derive(Debug, Clone, Copy)]
pub(in crate::panel::chat) struct BubbleColors {
    pub fill: Color,

    /// 有值就画 1 px 描边。
    pub border: Option<Color>,

    pub text: Color,

    /// 次要文字：引用、编号、思考过程。
    pub muted: Color,

    pub code: Color,

    pub bar: Color,
}

impl BubbleColors {
    pub fn of(role: Role) -> Self {
        match role {
            Role::User => Self {
                fill: tokens::PRIMARY,
                border: None,
                text: tokens::WINDOW_BG,
                muted: Color::hex(0xDBE7FF),
                code: tokens::WINDOW_BG,
                bar: Color::hex(0x93B4F5),
            },
            Role::Assistant => Self {
                fill: tokens::CARD_BLUE,
                border: None,
                text: tokens::TEXT,
                muted: TEXT_MID,
                code: CODE_INLINE,
                bar: QUOTE_BAR,
            },
            Role::Error => Self {
                fill: tokens::ERROR_BG,
                border: Some(tokens::ERROR_BORDER),
                text: tokens::ERROR_FG,
                muted: tokens::ERROR_FG,
                code: tokens::ERROR_FG,
                bar: tokens::ERROR_BORDER,
            },
        }
    }
}
