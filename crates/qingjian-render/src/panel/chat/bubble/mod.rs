//! 对话气泡：排版（带缓存）与绘制。

mod blocks;
mod colors;
mod draw;
mod layout;

pub(super) use draw::draw;
pub(super) use layout::{BubbleLayout, ensure};
