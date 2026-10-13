//! 会话历史的读写：`<用户目录>/ai-chat/<会话 id>.json`。

use std::fs;
use std::path::{Path, PathBuf};

use super::session::ChatSession;
use super::summary::SessionSummary;
use super::{KEEP_LIMIT, SIDEBAR_LIMIT};
use crate::error::LearningError;

const DIR_NAME: &str = "ai-chat";

#[derive(Debug, Clone)]
pub struct ChatHistory {
    dir: PathBuf,
}

impl ChatHistory {
    pub fn open(user_dir: &Path) -> Self {
        Self {
            dir: user_dir.join(DIR_NAME),
        }
    }

    /// 最近的会话摘要，最新的在前，最多 [`SIDEBAR_LIMIT`] 个。
    pub fn recent(&self) -> Vec<SessionSummary> {
        let mut all = self.all();
        all.truncate(SIDEBAR_LIMIT);
        all.iter().map(ChatSession::summary).collect()
    }

    pub fn load(&self, id: &str) -> Option<ChatSession> {
        let path = self.path_of(id)?;
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// 落盘；没有消息的会话不存（已存在的旧文件也删掉）。顺手清掉超出 [`KEEP_LIMIT`] 的旧会话。
    pub fn save(&self, session: &ChatSession) -> Result<(), LearningError> {
        let Some(path) = self.path_of(&session.id) else {
            return Ok(());
        };
        if session.messages.is_empty() {
            let _ = fs::remove_file(path);
            return Ok(());
        }
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(session).map_err(std::io::Error::other)?;
        // 先写临时文件再改名，中途断电不会留下半个 JSON。
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(&temp, &path)?;
        self.prune();
        Ok(())
    }

    pub fn delete(&self, id: &str) {
        if let Some(path) = self.path_of(id) {
            let _ = fs::remove_file(path);
        }
    }

    /// 读出全部会话，按更新时间从新到旧；坏文件跳过。
    fn all(&self) -> Vec<ChatSession> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut sessions: Vec<ChatSession> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| serde_json::from_str(&fs::read_to_string(entry.path()).ok()?).ok())
            .collect();
        sessions.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions
    }

    fn prune(&self) {
        for old in self.all().iter().skip(KEEP_LIMIT) {
            self.delete(&old.id);
        }
    }

    /// id 只允许字母数字、`-`、`_`，免得拼出目录之外的路径。
    fn path_of(&self, id: &str) -> Option<PathBuf> {
        let valid = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        valid.then(|| self.dir.join(format!("{id}.json")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_history::{ChatRecord, ChatRole};

    fn temp_history(name: &str) -> ChatHistory {
        let dir = std::env::temp_dir().join(format!("qingjian-chat-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        ChatHistory::open(&dir)
    }

    fn session(id: &str, now: i64, text: &str) -> ChatSession {
        let mut session = ChatSession::new(id, None, now);
        session.push(ChatRecord::new(ChatRole::User, text), now);
        session
    }

    #[test]
    fn saved_session_comes_back_after_reopening() {
        let history = temp_history("roundtrip");
        let mut saved = session("s1", 10, "你好");
        saved.messages[0].reasoning = "先想想".into();
        saved.push(ChatRecord::new(ChatRole::Assistant, "Hi!"), 11);
        history.save(&saved).expect("写盘成功");
        let loaded = ChatHistory {
            dir: history.dir.clone(),
        }
        .load("s1")
        .expect("读得回来");
        assert_eq!(loaded, saved);
    }

    #[test]
    fn recent_lists_newest_first_and_stops_at_the_sidebar_limit() {
        let history = temp_history("recent");
        for i in 0..25 {
            history
                .save(&session(&format!("s{i}"), i, "问题"))
                .expect("写盘成功");
        }
        let recent = history.recent();
        assert_eq!(recent.len(), SIDEBAR_LIMIT);
        assert_eq!(recent[0].id, "s24");
        assert_eq!(recent[0].message_count, 1);
    }

    #[test]
    fn empty_sessions_are_not_kept() {
        let history = temp_history("empty");
        history.save(&session("s1", 1, "问题")).expect("写盘成功");
        history
            .save(&ChatSession::new("s1", None, 2))
            .expect("写盘成功");
        assert!(history.recent().is_empty());
    }

    #[test]
    fn bad_ids_and_bad_files_are_ignored() {
        let history = temp_history("bad");
        history
            .save(&session("../evil", 1, "问题"))
            .expect("不报错");
        assert!(history.load("../evil").is_none());
        fs::create_dir_all(&history.dir).expect("建目录");
        fs::write(history.dir.join("broken.json"), "{not json").expect("写坏文件");
        history.save(&session("ok", 1, "问题")).expect("写盘成功");
        assert_eq!(history.recent().len(), 1);
    }

    #[test]
    fn old_sessions_beyond_the_keep_limit_are_pruned() {
        let history = temp_history("prune");
        for i in 0..(KEEP_LIMIT as i64 + 5) {
            history
                .save(&session(&format!("s{i}"), i, "问题"))
                .expect("写盘成功");
        }
        assert_eq!(history.all().len(), KEEP_LIMIT);
        assert!(history.load("s0").is_none());
    }
}
