//! AI 问答的会话仓库：当前会话 + 磁盘上的历史。流式任务在别的线程里往里追加消息，所以内部上锁。

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use qingjian_learning::chat_history::{ChatHistory, ChatRecord, ChatSession, SessionSummary};

pub(crate) struct AiStore {
    current: Mutex<ChatSession>,

    /// 落盘位置；没有（测试、没有用户目录）就只在内存里。
    history: Option<ChatHistory>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

fn fresh(topic: Option<String>) -> ChatSession {
    ChatSession::new(uuid::Uuid::new_v4().simple().to_string(), topic, now())
}

impl AiStore {
    pub(crate) fn new(history: Option<ChatHistory>) -> Self {
        Self {
            current: Mutex::new(fresh(None)),
            history,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ChatSession> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn session_id(&self) -> String {
        self.lock().id.clone()
    }

    pub(crate) fn snapshot(&self) -> ChatSession {
        self.lock().clone()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock().messages.is_empty()
    }

    /// 当前会话的主题。
    pub(crate) fn topic(&self) -> Option<String> {
        self.lock().topic.clone()
    }

    /// 给还没有消息的当前会话定主题。
    pub(crate) fn set_topic(&self, topic: Option<String>) {
        let mut session = self.lock();
        if session.messages.is_empty() {
            *session = ChatSession::new(session.id.clone(), topic, now());
        }
    }

    /// 换一个全新的空会话；旧会话已经随每条消息落过盘。
    pub(crate) fn start_new(&self, topic: Option<String>) -> ChatSession {
        let mut session = self.lock();
        *session = fresh(topic);
        session.clone()
    }

    /// 切到磁盘上的某个会话；找不到返回 `None`，当前会话不变。
    pub(crate) fn switch(&self, id: &str) -> Option<ChatSession> {
        let loaded = self.history.as_ref()?.load(id)?;
        let mut session = self.lock();
        *session = loaded;
        Some(session.clone())
    }

    /// 清空当前会话的消息（保留主题）并删掉磁盘文件。
    pub(crate) fn clear(&self) -> ChatSession {
        let mut session = self.lock();
        session.messages.clear();
        session.updated_at = now();
        if let Some(history) = &self.history {
            history.delete(&session.id);
        }
        session.clone()
    }

    /// 往 `session_id` 这个会话追加一条消息并落盘。会话已经切走了就改磁盘上的那份，不会串进当前会话。
    pub(crate) fn push(&self, session_id: &str, record: ChatRecord) {
        let mut current = self.lock();
        if current.id == session_id {
            current.push(record, now());
            self.save(&current);
            return;
        }
        drop(current);
        let Some(history) = &self.history else {
            return;
        };
        if let Some(mut other) = history.load(session_id) {
            other.push(record, now());
            self.save(&other);
        }
    }

    /// 侧栏列表：磁盘上最近的会话；没有磁盘时退化成当前会话（有消息的话）。
    pub(crate) fn sessions(&self) -> Vec<SessionSummary> {
        match &self.history {
            Some(history) => history.recent(),
            None => {
                let session = self.lock();
                if session.messages.is_empty() {
                    Vec::new()
                } else {
                    vec![session.summary()]
                }
            }
        }
    }

    fn save(&self, session: &ChatSession) {
        let Some(history) = &self.history else {
            return;
        };
        if let Err(error) = history.save(session) {
            tracing::warn!(%error, "AI 问答会话保存失败");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_learning::chat_history::ChatRole;

    fn user(text: &str) -> ChatRecord {
        ChatRecord::new(ChatRole::User, text)
    }

    fn with_disk() -> (AiStore, tempfile_dir::Dir) {
        let dir = tempfile_dir::Dir::new();
        (AiStore::new(Some(ChatHistory::open(dir.path()))), dir)
    }

    /// 测试用的临时目录，不引入额外依赖。
    mod tempfile_dir {
        use std::path::{Path, PathBuf};

        pub(super) struct Dir(PathBuf);

        impl Dir {
            pub(super) fn new() -> Self {
                let path =
                    std::env::temp_dir().join(format!("qj-ai-store-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }

            pub(super) fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn messages_from_every_session_are_saved_and_listed() {
        let (store, _dir) = with_disk();
        let first = store.session_id();
        store.push(&first, user("第一个问题"));
        store.start_new(None);
        let second = store.session_id();
        store.push(&second, user("第二个问题"));
        // 排序按秒级时间戳，同一秒内的先后不确定，次序由 chat_history 自己的测试保证。
        let mut ids: Vec<_> = store.sessions().into_iter().map(|s| s.id).collect();
        ids.sort();
        let mut expected = vec![second, first];
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn switching_loads_the_saved_session_and_missing_ids_change_nothing() {
        let (store, _dir) = with_disk();
        let first = store.session_id();
        store.push(&first, user("你好"));
        store.start_new(Some("Do your best".into()));
        assert!(store.is_empty());
        let loaded = store.switch(&first).expect("应该能切回去");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(store.session_id(), first);
        assert!(store.switch("no-such").is_none());
        assert_eq!(store.session_id(), first);
    }

    #[test]
    fn late_pushes_go_to_their_own_session_not_the_current_one() {
        let (store, _dir) = with_disk();
        let old = store.session_id();
        store.push(&old, user("问"));
        store.start_new(None);
        // 旧会话里还在跑的流式任务写回答
        store.push(&old, ChatRecord::new(ChatRole::Assistant, "迟到的回答"));
        assert!(store.is_empty(), "当前（新）会话不能被串进旧会话的消息");
        let back = store.switch(&old).unwrap();
        assert_eq!(back.messages.len(), 2);
    }

    #[test]
    fn clear_keeps_the_topic_and_removes_the_file() {
        let (store, _dir) = with_disk();
        store.set_topic(Some("Do your best".into()));
        let id = store.session_id();
        store.push(&id, user("问"));
        assert_eq!(store.sessions().len(), 1);
        let cleared = store.clear();
        assert!(cleared.messages.is_empty());
        assert_eq!(cleared.topic.as_deref(), Some("Do your best"));
        assert!(store.sessions().is_empty());
    }

    #[test]
    fn topic_is_only_set_while_the_session_is_empty() {
        let (store, _dir) = with_disk();
        let id = store.session_id();
        store.set_topic(Some("A".into()));
        assert_eq!(store.topic().as_deref(), Some("A"));
        assert_eq!(store.session_id(), id);
        store.push(&id, user("问"));
        store.set_topic(Some("B".into()));
        assert_eq!(store.topic().as_deref(), Some("A"));
    }

    #[test]
    fn without_a_disk_the_store_still_works_in_memory() {
        let store = AiStore::new(None);
        assert!(store.sessions().is_empty());
        let id = store.session_id();
        store.push(&id, user("问"));
        assert_eq!(store.sessions().len(), 1);
        assert!(store.switch(&id).is_none());
        assert_eq!(store.snapshot().messages.len(), 1);
    }
}
