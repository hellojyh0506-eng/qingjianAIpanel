//! 一个会话：主题、消息与更新时间。

use serde::{Deserialize, Serialize};

use super::TITLE_CHARS;
use super::record::ChatRecord;
use super::role::ChatRole;
use super::summary::SessionSummary;

/// 没有主题也没有提问时的标题。
const DEFAULT_TITLE: &str = "新对话";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,

    /// 这次对话围绕的表达（从学习助手进来时有）。
    #[serde(default)]
    pub topic: Option<String>,

    /// 最后更新时间（Unix 秒）。
    pub updated_at: i64,

    pub messages: Vec<ChatRecord>,
}

impl ChatSession {
    pub fn new(id: impl Into<String>, topic: Option<String>, now: i64) -> Self {
        Self {
            id: id.into(),
            topic: topic.filter(|topic| !topic.trim().is_empty()),
            updated_at: now,
            messages: Vec::new(),
        }
    }

    /// 标题：有主题用主题，否则取首个问题的前几个字，都没有是「新对话」。
    pub fn title(&self) -> String {
        if let Some(topic) = &self.topic {
            return topic.trim().to_owned();
        }
        self.messages
            .iter()
            .find(|message| message.role == ChatRole::User)
            .map(|message| {
                message
                    .text
                    .trim()
                    .chars()
                    .take(TITLE_CHARS)
                    .collect::<String>()
            })
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| DEFAULT_TITLE.to_owned())
    }

    pub fn push(&mut self, record: ChatRecord, now: i64) {
        self.messages.push(record);
        self.updated_at = now;
    }

    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            title: self.title(),
            message_count: self.messages.len(),
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_is_the_title() {
        let session = ChatSession::new("a", Some(" Do your best ".into()), 0);
        assert_eq!(session.title(), "Do your best");
    }

    #[test]
    fn without_topic_the_first_question_gives_twelve_chars() {
        let mut session = ChatSession::new("a", None, 0);
        assert_eq!(session.title(), "新对话");
        session.push(
            ChatRecord::new(ChatRole::User, "请问雅思口语第二部分怎么准备才能拿到高分呢"),
            5,
        );
        assert_eq!(session.title(), "请问雅思口语第二部分怎么");
        assert_eq!(session.updated_at, 5);
    }

    #[test]
    fn blank_topic_counts_as_no_topic() {
        let session = ChatSession::new("a", Some("  ".into()), 0);
        assert_eq!(session.topic, None);
    }
}
