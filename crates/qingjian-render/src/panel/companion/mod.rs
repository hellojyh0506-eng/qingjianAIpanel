//! 学习助手面板（青简 · 英语学习助手）：视图模型与整帧渲染。
//!
//! 窗口层（Windows / macOS）持有 [`CompanionView`]，状态变化后调 [`render`] 得到位图与点击表，再自己贴到屏幕。
//! 布局常量与参考图的对应关系见 `style.rs`，设计与验收见 `docs/plan/ai-panel-rebuild.md` 的 P8。

mod alternative;
mod detail;
mod dialogue;
mod example;
mod footer;
mod frame;
mod header;
mod hero;
mod hit;
mod nuance;
mod render;
mod states;
mod status;
mod style;
mod tab_alternatives;
mod tab_bar;
mod tab_dialogue;
mod tab_nuance;
mod turn;
mod view;
mod widgets;

pub use alternative::Alternative;
pub use detail::Detail;
pub use dialogue::DialogueContent;
pub use example::Example;
pub use frame::CompanionFrame;
pub use hit::CompanionHit;
pub use nuance::NuanceContent;
pub use render::render;
pub use status::TabStatus;
pub use turn::Turn;
pub use view::CompanionView;
