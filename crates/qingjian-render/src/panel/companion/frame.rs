//! 渲染出来的一帧学习助手面板，连同滚动信息。

use super::hit::CompanionHit;
use crate::panel::PanelFrame;

pub struct CompanionFrame {
    pub frame: PanelFrame<CompanionHit>,

    /// 内容区最多能滚多远（点）；内容没超出视口时为 0。
    pub max_scroll: f32,

    /// 这一帧实际使用的滚动量（已夹到 `0..=max_scroll`）。
    pub scroll: f32,

    /// 窗口高度（点）：收起时只剩标题栏。
    pub height: f32,
}
