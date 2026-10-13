//! AI 问答面板的视图模型：窗口持有它，每次变化改字段后重画一帧。

use super::message::ChatMessage;
use super::phase::Phase;
use super::role::Role;
use super::session_item::SessionItem;

#[derive(Debug, Clone)]
pub struct ChatView {
    /// 窗口尺寸（点），最大化时由窗口层改。
    pub width: f32,

    pub height: f32,

    /// 侧栏会话，最新在前。
    pub sessions: Vec<SessionItem>,

    /// 侧栏列表向上滚过的距离（点）。
    pub sidebar_scroll: f32,

    /// 当前主题（来自学习助手的表达）；自由提问为 `None`。
    pub topic: Option<String>,

    pub messages: Vec<ChatMessage>,

    /// 空会话时欢迎卡下面的建议问题。
    pub suggestions: Vec<String>,

    /// 输入框已确定的文字。
    pub input: String,

    /// 光标在 `input` 里的字节位置（必须落在字符边界上）。
    pub cursor: usize,

    /// 输入法正在组字的文字，显示在光标处。
    pub composition: String,

    pub phase: Phase,

    /// 联网搜索开关。
    pub web: bool,

    /// 深度思考开关。
    pub deep: bool,

    /// 消息区向上滚过的距离（点）。
    pub scroll: f32,

    /// 贴着底部：新内容到来时自动滚到底；用户往上滚就关掉，滚回底部再开。
    pub follow_bottom: bool,

    pub pinned: bool,

    pub maximized: bool,

    /// 底部短暂提示（「已复制」）。
    pub notice: Option<String>,

    /// 转圈图标的角度（度），由窗口的定时器推进。
    pub spin: f32,
}

impl Default for ChatView {
    fn default() -> Self {
        Self {
            width: 720.0,
            height: 520.0,
            sessions: Vec::new(),
            sidebar_scroll: 0.0,
            topic: None,
            messages: Vec::new(),
            suggestions: Vec::new(),
            input: String::new(),
            cursor: 0,
            composition: String::new(),
            phase: Phase::Idle,
            web: false,
            deep: false,
            scroll: 0.0,
            follow_bottom: true,
            pinned: false,
            maximized: false,
            notice: None,
            spin: 0.0,
        }
    }
}

impl ChatView {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }

    /// 最后一条助手消息；流式追加文字、思考过程都落在它上面。
    pub fn last_assistant_mut(&mut self) -> Option<&mut ChatMessage> {
        self.messages
            .last_mut()
            .filter(|message| message.role == Role::Assistant)
    }
}
