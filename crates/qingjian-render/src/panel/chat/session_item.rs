//! 侧栏里的一行会话。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    pub title: String,

    /// 副标题，如「3条消息 · 10/24」。
    pub subtitle: String,

    pub selected: bool,
}
