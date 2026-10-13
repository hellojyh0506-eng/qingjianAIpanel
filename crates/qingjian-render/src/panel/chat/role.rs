//! 一条消息是谁说的。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,

    Assistant,

    /// 出错提示：红色气泡，不进入对话历史。
    Error,
}
