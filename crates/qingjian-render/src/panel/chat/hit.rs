//! AI 问答面板上能点的东西。窗口过程查 [`HitMap`](crate::panel::HitMap) 得到它，再交给业务层处理。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatHit {
    /// 顶栏空白处：按住拖动窗口。
    Drag,

    Close,

    /// 置顶 / 取消置顶。
    Pin,

    /// 最大化 / 还原。
    Maximize,

    /// 新建对话。
    NewChat,

    /// 切换到侧栏第 n 个会话。
    Session(usize),

    /// 清空当前会话。
    ClearChat,

    /// 打开设置。
    Settings,

    /// 点建议问题 n：直接发出。
    Suggestion(usize),

    /// 输入框：聚焦。
    Input,

    /// 联网搜索开关。
    ToggleWeb,

    /// 深度思考开关。
    ToggleDeep,

    Send,

    /// 流式期间的停止键。
    Stop,

    /// 复制第 n 条消息。
    CopyMessage(usize),

    /// 展开 / 折叠第 n 条消息的思考过程。
    ToggleReasoning(usize),
}
