//! 渲染出来的一帧 AI 问答面板，连同滚动与输入光标信息。

use super::hit::ChatHit;
use crate::panel::{Area, PanelFrame};

pub struct ChatFrame {
    pub frame: PanelFrame<ChatHit>,

    /// 消息区最多能滚多远（点）。
    pub max_scroll: f32,

    /// 这一帧实际使用的消息区滚动量（已夹到 `0..=max_scroll`）。
    pub scroll: f32,

    /// 侧栏会话列表最多能滚多远（点）。
    pub sidebar_max_scroll: f32,

    /// 输入光标：窗口坐标（点）下的 `(x, y, 高)`，输入法候选窗要贴着它。
    pub caret: (f32, f32, f32),

    /// 输入框区域（点，窗口坐标）。
    pub input: Area,
}
