//! AI 问答面板（青简 AI 聊天）：视图模型与整帧渲染。
//!
//! 窗口层（Windows / macOS）持有 [`ChatView`]，状态变化后调 [`render`] 得到位图与点击表，再自己贴到屏幕。
//! 布局常量与参考图的对应关系见 `style.rs`，设计与验收见 `docs/plan/ai-panel-rebuild.md` 的 P9。

mod bubble;
mod composer;
mod frame;
mod hit;
mod input_wrap;
mod markdown;
mod message;
mod messages;
mod phase;
mod render;
mod role;
mod session_item;
mod sidebar;
mod style;
#[cfg(test)]
mod tests;
mod topic_bar;
mod view;
mod welcome;
mod widgets;

pub use frame::ChatFrame;
pub use hit::ChatHit;
pub use markdown::{Block, Inline, parse_blocks};
pub use message::ChatMessage;
pub use phase::Phase;
pub use render::render;
pub use role::Role;
pub use session_item::SessionItem;
pub use style::SIDEBAR_WIDTH;
pub use view::ChatView;
