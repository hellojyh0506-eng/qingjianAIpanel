//! 一轮流式回答的收尾：把回答记进会话与 LLM 上下文，并通知窗口刷新侧栏。

use std::sync::{Arc, Mutex};

use qingjian_learning::chat_history::{ChatRecord, ChatRole};
use qingjian_predict::ChatMessage;

use super::store::AiStore;
use crate::dispatch::CandidateSink;

pub(super) struct Turn {
    pub(super) sink: Box<dyn CandidateSink>,

    pub(super) store: Arc<AiStore>,

    /// LLM 上下文（`Router::ai_messages`）。
    pub(super) context: Arc<Mutex<Vec<ChatMessage>>>,

    /// 这一轮属于哪个会话：用户中途切走后，回答仍记回它自己的会话。
    pub(super) session_id: String,
}

impl Turn {
    /// 记下助手的回答（可能是被打断的半截）；空回答不记。
    pub(super) fn record(&self, text: &str, reasoning: &str) {
        if text.is_empty() {
            return;
        }
        let mut record = ChatRecord::new(ChatRole::Assistant, text);
        record.reasoning = reasoning.to_owned();
        self.store.push(&self.session_id, record);
        if self.store.session_id() == self.session_id
            && let Ok(mut context) = self.context.lock()
        {
            context.push(ChatMessage {
                role: "assistant".to_string(),
                content: text.to_owned(),
            });
        }
        self.sink.send_ai_sessions(self.store.sessions());
    }
}
