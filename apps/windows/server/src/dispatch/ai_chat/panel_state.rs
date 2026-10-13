//! 下发给 AI 问答窗口的整面状态：侧栏会话列表、当前会话、主题的替换表达。

use qingjian_learning::chat_history::{ChatSession, SessionSummary};

#[derive(Debug, Clone)]
pub struct AiPanelState {
    /// 侧栏会话（最新在前，已落盘的）。
    pub sessions: Vec<SessionSummary>,

    /// 当前会话，含全部消息。
    pub session: ChatSession,

    /// 学习助手「更自然地说」里的第一个替换表达，建议问题里用；没生成过就是 `None`。
    pub alternative: Option<String>,
}
