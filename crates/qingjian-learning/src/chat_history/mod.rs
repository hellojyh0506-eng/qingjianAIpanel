//! AI 问答面板的会话历史：每个会话一个 JSON，放在用户目录的 `ai-chat/` 下。
//!
//! 侧栏只列最近的几个（[`SIDEBAR_LIMIT`]），更老的在落盘时顺手清掉（[`KEEP_LIMIT`]）。没有任何消息的会话不落盘。

mod record;
mod role;
mod session;
mod store;
mod summary;

pub use record::ChatRecord;
pub use role::ChatRole;
pub use session::ChatSession;
pub use store::ChatHistory;
pub use summary::SessionSummary;

/// 侧栏最多列几个会话。
pub const SIDEBAR_LIMIT: usize = 20;

/// 磁盘上最多留几个会话，超出的按更新时间从旧到新删。
pub const KEEP_LIMIT: usize = 60;

/// 没有主题时，用首个问题的前几个字当标题。
pub const TITLE_CHARS: usize = 12;
