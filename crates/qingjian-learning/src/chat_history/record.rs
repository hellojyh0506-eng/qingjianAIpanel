//! 会话里的一条消息。

use serde::{Deserialize, Serialize};

use super::role::ChatRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRecord {
    pub role: ChatRole,

    pub text: String,

    /// 模型的思考过程（深度模式才有），界面折叠显示。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning: String,
}

impl ChatRecord {
    pub fn new(role: ChatRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            reasoning: String::new(),
        }
    }
}
