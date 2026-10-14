//! AI 问答面板的业务：打开时定主题、会话的新建 / 切换 / 清空 / 落盘、流式提问。
//! 提问与流式回答在 [`send`]，会话仓库在 [`store`]，一轮回答的收尾在 [`turn`]；面板窗口在 `ui::ai_assistant`，
//! 两边只通过 [`CandidateSink`](super::CandidateSink) 与 [`CandidateAction`](super::CandidateAction) 说话。

mod boost;
mod panel_state;
mod send;
mod store;
mod topic;
mod turn;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qingjian_learning::chat_history::{ChatHistory, ChatRole, ChatSession};
use qingjian_predict::ChatMessage;

pub use self::panel_state::AiPanelState;
pub(crate) use self::store::AiStore;
use super::Router;

impl Router {
    /// 开启 AI 问答的会话落盘：启动时调一次，之后每条消息都存进 `user_dir`。不调就只在内存里（测试）。
    pub fn enable_ai_history(&mut self, user_dir: &Path) {
        self.ai_store = Arc::new(AiStore::new(Some(ChatHistory::open(user_dir))));
    }

    /// 打开 / 聚焦独立 AI Assistant 对话窗口
    pub(crate) fn open_ai_assistant(&mut self) {
        self.ai_assistant_open = true;
        self.notice = Some("💬 已开启 AI 对话助手 (Ctrl+Alt+D)".to_string());
        let anchor = self
            .effective_display_rect()
            .map(|r| windows::Win32::Foundation::RECT {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            });
        let topic = self
            .passive_learning_card
            .as_ref()
            .map(|c| c.target_text.clone())
            .or_else(|| self.passive_translation_target.clone())
            .or_else(|| self.layout_candidate(self.highlight).map(|c| c.text))
            .map(|topic| topic.trim().to_owned())
            .filter(|topic| !topic.is_empty());
        if let Some(topic) = topic {
            self.focus_ai_topic(topic);
        }
        let state = self.ai_panel_state();
        self.candidates.show_ai_assistant(anchor, state);
    }

    /// 关闭独立 AI Assistant 对话窗口
    pub(crate) fn close_ai_assistant(&mut self) {
        self.ai_assistant_open = false;
        self.ai_stop_signal.store(true, Ordering::Relaxed);
        self.candidates.hide_ai_assistant();
        self.notice = Some("已关闭 AI 对话助手".to_string());
    }

    /// 带着主题进来：当前会话还是空的就给它定主题；已经聊过别的就另开一个会话，同主题则接着聊。
    fn focus_ai_topic(&mut self, topic: String) {
        if self.ai_store.is_empty() {
            self.ai_store.set_topic(Some(topic));
            return;
        }
        if self.ai_store.topic().as_deref() == Some(topic.as_str()) {
            return;
        }
        let session = self.ai_store.start_new(Some(topic));
        self.reset_ai_context(&session);
    }

    /// 新建会话（侧栏「新对话」）：打断在途回答，回推整面状态。
    pub(crate) fn ai_chat_new(&mut self) {
        let session = self.ai_store.start_new(None);
        self.reset_ai_context(&session);
        self.push_ai_state();
    }

    /// 切到侧栏里的另一个会话。
    pub(crate) fn ai_chat_switch(&mut self, id: &str) {
        if self.ai_store.session_id() == id {
            return;
        }
        match self.ai_store.switch(id) {
            Some(session) => {
                self.reset_ai_context(&session);
                self.push_ai_state();
            }
            None => {
                tracing::warn!(id, "要切换的 AI 会话不存在，刷新侧栏");
                self.candidates.send_ai_sessions(self.ai_store.sessions());
            }
        }
    }

    /// 清空当前会话（顶栏「清空会话」）：保留主题，删掉磁盘文件。
    pub(crate) fn ai_chat_clear(&mut self) {
        let session = self.ai_store.clear();
        self.reset_ai_context(&session);
        self.push_ai_state();
    }

    /// 换会话后：打断在途回答，LLM 上下文按新会话的消息重建。
    fn reset_ai_context(&mut self, session: &ChatSession) {
        self.ai_stop_signal.store(true, Ordering::Relaxed);
        self.ai_stop_signal = Arc::new(AtomicBool::new(false));
        if let Ok(mut context) = self.ai_messages.lock() {
            *context = session
                .messages
                .iter()
                .map(|record| ChatMessage {
                    role: match record.role {
                        ChatRole::User => "user",
                        ChatRole::Assistant => "assistant",
                    }
                    .to_string(),
                    content: record.text.clone(),
                })
                .collect();
        }
    }

    fn ai_panel_state(&self) -> AiPanelState {
        AiPanelState {
            sessions: self.ai_store.sessions(),
            session: self.ai_store.snapshot(),
            alternative: self
                .companion
                .as_ref()
                .and_then(|session| session.alternative()),
        }
    }

    fn push_ai_state(&self) {
        self.candidates.send_ai_session(self.ai_panel_state());
    }
}
