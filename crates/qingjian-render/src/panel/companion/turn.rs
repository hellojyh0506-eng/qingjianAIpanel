//! 对话里的一句话。

use crate::panel::Avatar;

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    /// 说话人：`Boy` 是 A（左），`Girl` 是 B（右）。
    pub speaker: Avatar,

    pub en: String,

    pub zh: String,

    /// 英文里要加粗的目标表达。
    pub hl: String,
}
