//! AI 问答面板的视图状态变换：把业务层推来的会话装进 [`ChatView`]、流式追加、出错、建议问题。
//! 全是纯函数，不碰窗口，Linux 上也能测。

use qingjian_learning::chat_history::{ChatRecord, ChatRole, SessionSummary};
use qingjian_predict::SearchResult;
use qingjian_render::chat::{ChatMessage, ChatView, Phase, Role, SessionItem};

use crate::dispatch::AiPanelState;

/// 把整面状态装进视图：侧栏、主题、消息、建议问题；在途状态、滚动位置重置，输入框草稿保留。
pub(super) fn load_state(view: &mut ChatView, state: &AiPanelState) {
    view.sessions = sidebar_items(&state.sessions, &state.session.summary());
    view.sidebar_scroll = 0.0;
    view.topic = state.session.topic.clone();
    view.messages = state.session.messages.iter().map(message_of).collect();
    view.suggestions = suggestions_for(view.topic.as_deref(), state.alternative.as_deref());
    view.phase = Phase::Idle;
    view.scroll = 0.0;
    view.follow_bottom = true;
}

fn message_of(record: &ChatRecord) -> ChatMessage {
    let role = match record.role {
        ChatRole::User => Role::User,
        ChatRole::Assistant => Role::Assistant,
    };
    let mut message = ChatMessage::new(role, record.text.as_str());
    if !record.reasoning.is_empty() {
        message.push_reasoning(&record.reasoning);
    }
    message
}

/// 侧栏条目与它们对应的会话 id（下标一一对应）。当前会话还没落盘（空会话）就前插一行，让用户看到自己在哪。
pub(super) fn sidebar_items(
    sessions: &[SessionSummary],
    current: &SessionSummary,
) -> Vec<SessionItem> {
    let mut rows: Vec<SessionSummary> = Vec::with_capacity(sessions.len() + 1);
    if !sessions.iter().any(|summary| summary.id == current.id) {
        rows.push(current.clone());
    }
    rows.extend(sessions.iter().cloned());
    rows.iter()
        .map(|summary| SessionItem {
            title: summary.title.clone(),
            subtitle: subtitle_of(summary),
            selected: summary.id == current.id,
        })
        .collect()
}

/// 侧栏条目对应的会话 id，顺序与 [`sidebar_items`] 一致。
pub(super) fn sidebar_ids(sessions: &[SessionSummary], current: &SessionSummary) -> Vec<String> {
    let mut ids = Vec::with_capacity(sessions.len() + 1);
    if !sessions.iter().any(|summary| summary.id == current.id) {
        ids.push(current.id.clone());
    }
    ids.extend(sessions.iter().map(|summary| summary.id.clone()));
    ids
}

fn subtitle_of(summary: &SessionSummary) -> String {
    if summary.message_count == 0 {
        return "新对话".to_string();
    }
    format!("{}条消息 · {}", summary.message_count, summary.date_label())
}

/// 空会话欢迎卡下的建议问题。有主题（从学习助手进来）就围绕它问，否则给通用三条。
pub(super) fn suggestions_for(topic: Option<&str>, alternative: Option<&str>) -> Vec<String> {
    let Some(topic) = topic.map(str::trim).filter(|topic| !topic.is_empty()) else {
        return vec![
            "怎么说得更地道?".to_string(),
            "请给我 IELTS 口语题".to_string(),
            "美国年轻人会怎么说?".to_string(),
        ];
    };
    let versus = match alternative.map(str::trim).filter(|alt| !alt.is_empty()) {
        Some(alt) => format!("{topic} 和 {alt} 有什么区别?"),
        None => format!("{topic} 还有哪些更地道的说法?"),
    };
    vec![
        format!("什么时候用 {topic}?"),
        "请给我 IELTS 口语题".to_string(),
        versus,
        "美国年轻人会怎么说?".to_string(),
    ]
}

/// 联网来源的显示行：「N. 标题」，标题为空用网址。
pub(super) fn source_lines(sources: &[SearchResult]) -> Vec<String> {
    sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let name = if source.title.trim().is_empty() {
                source.url.as_str()
            } else {
                source.title.trim()
            };
            format!("{}. {name}", index + 1)
        })
        .collect()
}

/// 这一轮是否还在进行（只有进行中才接收流式内容，换会话 / 停止之后迟到的块一律丢掉）。
pub(super) fn is_busy(view: &ChatView) -> bool {
    view.phase.is_busy()
}

/// 本地先放上用户气泡，进入等待回答。
pub(super) fn push_user(view: &mut ChatView, text: &str) {
    view.messages.push(ChatMessage::new(Role::User, text));
    view.phase = Phase::Waiting;
    view.follow_bottom = true;
}

/// 取出（必要时新建）正在流式的那条助手消息；挂上暂存的联网来源。
fn streaming_message<'a>(
    view: &'a mut ChatView,
    pending_sources: &mut Vec<String>,
) -> &'a mut ChatMessage {
    if view.last_assistant_mut().is_none() {
        let mut message = ChatMessage::new(Role::Assistant, "");
        if !pending_sources.is_empty() {
            message.set_sources(std::mem::take(pending_sources));
        }
        view.messages.push(message);
    }
    view.phase = Phase::Streaming;
    match view.messages.last_mut() {
        Some(message) => message,
        None => unreachable!("刚保证过最后一条是助手消息"),
    }
}

/// 回答文字增量。不在进行中就丢弃，返回是否被接收。
pub(super) fn append_chunk(
    view: &mut ChatView,
    pending_sources: &mut Vec<String>,
    chunk: &str,
) -> bool {
    if !is_busy(view) {
        return false;
    }
    streaming_message(view, pending_sources).push_text(chunk);
    true
}

/// 思考过程增量；思考过程默认展开，让用户看到「正在想」。
pub(super) fn append_reasoning(
    view: &mut ChatView,
    pending_sources: &mut Vec<String>,
    chunk: &str,
) -> bool {
    if !is_busy(view) {
        return false;
    }
    let message = streaming_message(view, pending_sources);
    if message.reasoning().is_empty() && message.text().is_empty() {
        message.set_reasoning_open(true);
    }
    message.push_reasoning(chunk);
    true
}

/// 联网来源到了：已有助手消息就挂上去，没有就暂存到助手消息出现。
pub(super) fn attach_sources(
    view: &mut ChatView,
    pending_sources: &mut Vec<String>,
    lines: Vec<String>,
) {
    if !is_busy(view) {
        return;
    }
    match view.last_assistant_mut() {
        Some(message) => message.set_sources(lines),
        None => *pending_sources = lines,
    }
}

/// 回答结束：思考过程收起（只留一行「已深度思考」），回到空闲。
pub(super) fn finish(view: &mut ChatView, pending_sources: &mut Vec<String>) {
    pending_sources.clear();
    if let Some(message) = view.last_assistant_mut() {
        message.set_reasoning_open(false);
    }
    view.phase = Phase::Idle;
}

/// 出错：追加红色错误气泡，回到空闲。不在进行中就忽略（多半是被打断的残留）。
pub(super) fn fail(view: &mut ChatView, pending_sources: &mut Vec<String>, error: &str) -> bool {
    if !is_busy(view) {
        return false;
    }
    pending_sources.clear();
    view.messages.push(ChatMessage::new(
        Role::Error,
        format!("请求失败:{}", error.trim()),
    ));
    view.phase = Phase::Idle;
    view.follow_bottom = true;
    true
}

/// 清空当前对话的消息（保留主题与输入草稿）。
pub(super) fn clear_messages(view: &mut ChatView, pending_sources: &mut Vec<String>) {
    pending_sources.clear();
    view.messages.clear();
    view.phase = Phase::Idle;
    view.scroll = 0.0;
    view.follow_bottom = true;
}

#[cfg(test)]
mod tests {
    use qingjian_learning::chat_history::ChatSession;

    use super::*;

    fn state_with(topic: Option<&str>, texts: &[(&str, &str)]) -> AiPanelState {
        let mut session = ChatSession::new("cur", topic.map(str::to_owned), 0);
        for (user, answer) in texts {
            session.push(ChatRecord::new(ChatRole::User, *user), 1);
            session.push(ChatRecord::new(ChatRole::Assistant, *answer), 2);
        }
        AiPanelState {
            sessions: Vec::new(),
            session,
            alternative: None,
        }
    }

    #[test]
    fn load_state_maps_records_topic_and_resets_the_turn() {
        let mut state = state_with(Some("Do your best"), &[("q", "a")]);
        state.session.messages[1].reasoning = "想过".into();
        let mut view = ChatView {
            phase: Phase::Streaming,
            scroll: 99.0,
            input: "草稿".into(),
            ..ChatView::default()
        };
        load_state(&mut view, &state);
        assert_eq!(view.topic.as_deref(), Some("Do your best"));
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[0].role, Role::User);
        assert_eq!(view.messages[1].role, Role::Assistant);
        assert_eq!(view.messages[1].reasoning(), "想过");
        assert!(!view.messages[1].reasoning_open);
        assert_eq!(view.phase, Phase::Idle);
        assert_eq!(view.scroll, 0.0);
        assert!(view.follow_bottom);
        assert_eq!(view.input, "草稿");
    }

    #[test]
    fn sidebar_prepends_the_unsaved_current_session_and_marks_it() {
        let state = AiPanelState {
            sessions: vec![SessionSummary {
                id: "old".into(),
                title: "旧会话".into(),
                message_count: 4,
                updated_at: 0,
            }],
            ..state_with(None, &[])
        };
        let items = sidebar_items(&state.sessions, &state.session.summary());
        assert_eq!(items.len(), 2);
        assert!(items[0].selected && !items[1].selected);
        assert_eq!(items[0].subtitle, "新对话");
        assert!(items[1].subtitle.starts_with("4条消息 · "));
        assert_eq!(
            sidebar_ids(&state.sessions, &state.session.summary()),
            ["cur", "old"]
        );
    }

    #[test]
    fn sidebar_does_not_duplicate_a_saved_current_session() {
        let state = state_with(None, &[("hello", "hi")]);
        let current = state.session.summary();
        let sessions = vec![current.clone()];
        let items = sidebar_items(&sessions, &current);
        assert_eq!(items.len(), 1);
        assert!(items[0].selected);
        assert_eq!(items[0].title, "hello");
        assert_eq!(sidebar_ids(&sessions, &current), ["cur"]);
    }

    #[test]
    fn suggestions_follow_topic_and_alternative() {
        let general = suggestions_for(None, None);
        assert_eq!(general.len(), 3);
        let plain = suggestions_for(Some("Do your best"), None);
        assert_eq!(plain[0], "什么时候用 Do your best?");
        assert_eq!(plain[2], "Do your best 还有哪些更地道的说法?");
        let versus = suggestions_for(Some("Do your best"), Some("Give it your all"));
        assert_eq!(versus[2], "Do your best 和 Give it your all 有什么区别?");
        assert_eq!(suggestions_for(Some("  "), None), general);
    }

    #[test]
    fn stream_creates_one_assistant_message_and_collects_pending_sources() {
        let mut view = ChatView::default();
        let mut pending = Vec::new();
        assert!(!append_chunk(&mut view, &mut pending, "晚到"), "空闲时丢弃");
        push_user(&mut view, "hi");
        attach_sources(&mut view, &mut pending, vec!["1. A".into()]);
        assert_eq!(pending, ["1. A"]);
        assert!(append_reasoning(&mut view, &mut pending, "想"));
        assert!(append_chunk(&mut view, &mut pending, "你"));
        assert!(append_chunk(&mut view, &mut pending, "好"));
        assert!(pending.is_empty());
        assert_eq!(view.messages.len(), 2);
        let answer = &view.messages[1];
        assert_eq!(answer.text(), "你好");
        assert_eq!(answer.reasoning(), "想");
        assert_eq!(answer.sources, ["1. A"]);
        assert!(answer.reasoning_open, "思考中保持展开");
        assert_eq!(view.phase, Phase::Streaming);
        finish(&mut view, &mut pending);
        assert_eq!(view.phase, Phase::Idle);
        assert!(!view.messages[1].reasoning_open, "结束后收起");
    }

    #[test]
    fn fail_adds_an_error_bubble_only_while_busy() {
        let mut view = ChatView::default();
        let mut pending = Vec::new();
        assert!(!fail(&mut view, &mut pending, "超时"));
        push_user(&mut view, "hi");
        assert!(fail(&mut view, &mut pending, " 超时 "));
        let last = view.messages.last().unwrap();
        assert_eq!(last.role, Role::Error);
        assert_eq!(last.text(), "请求失败:超时");
        assert_eq!(view.phase, Phase::Idle);
    }

    #[test]
    fn clear_messages_keeps_topic_and_draft() {
        let mut view = ChatView {
            topic: Some("t".into()),
            input: "草稿".into(),
            ..ChatView::default()
        };
        let mut pending = vec!["x".to_string()];
        push_user(&mut view, "hi");
        clear_messages(&mut view, &mut pending);
        assert!(view.messages.is_empty() && pending.is_empty());
        assert_eq!(view.topic.as_deref(), Some("t"));
        assert_eq!(view.input, "草稿");
        assert_eq!(view.phase, Phase::Idle);
    }

    #[test]
    fn source_lines_number_titles_and_fall_back_to_urls() {
        let lines = source_lines(&[
            SearchResult {
                title: " 剑桥词典 ".into(),
                url: "https://a".into(),
                description: String::new(),
            },
            SearchResult {
                title: String::new(),
                url: "https://b".into(),
                description: String::new(),
            },
        ]);
        assert_eq!(lines, ["1. 剑桥词典", "2. https://b"]);
    }
}
