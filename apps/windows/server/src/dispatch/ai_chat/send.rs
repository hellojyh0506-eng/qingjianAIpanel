//! 提问：把用户的话记进会话与上下文，联网搜索（可选），流式请求模型，回答边到边转发给窗口。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qingjian_learning::chat_history::{ChatRecord, ChatRole};
use qingjian_predict::{
    ChatMessage, SearchResult, StreamChatClient, StreamEvent, execute_web_search,
};

use super::boost;
use super::topic;
use super::turn::Turn;
use crate::dispatch::Router;

const SYSTEM_PROMPT: &str = "你是青简输入法内置的 AI 智能助手。请自然、准确、清晰地回答用户的问题。根据问题类型灵活作答：日常闲聊或普通问答直接简洁回应；技术或专业问题直击本质；涉及语言学习时提供地道自然的解答。善用 Markdown 格式组织内容，不讲无用套话，不做无谓的长篇大论，绝不擅自强加中英互译。";

/// 一次回答最多生成的 token 数，与送给模型的历史条数。
const MAX_TOKENS: u32 = 2048;
const HISTORY_LIMIT: usize = 10;

const DEFAULT_BASE_URL: &str = "https://api.siliconflow.cn/v1";
const DEFAULT_MODEL: &str = "Qwen/Qwen3-14B";

impl Router {
    /// 停掉在途的回答（用户点了停止，或要换会话 / 关窗口）。
    pub(crate) fn ai_chat_stop(&mut self) {
        self.ai_stop_signal.store(true, Ordering::Relaxed);
    }

    /// 发起一轮提问。`deep` 打开才允许模型思考，`web_search` 打开先联网检索。
    pub(crate) fn ai_chat_send(&mut self, prompt: String, deep: bool, web_search: bool) {
        self.ai_stop_signal.store(true, Ordering::Relaxed);
        let stop_signal = Arc::new(AtomicBool::new(false));
        self.ai_stop_signal = stop_signal.clone();

        if let Ok(mut msgs) = self.ai_messages.lock() {
            msgs.push(ChatMessage {
                role: "user".to_string(),
                content: prompt.clone(),
            });
        }
        let session_id = self.ai_store.session_id();
        self.ai_store
            .push(&session_id, ChatRecord::new(ChatRole::User, prompt.clone()));
        self.candidates.send_ai_sessions(self.ai_store.sessions());

        let (base_url, model, api_key) = self.ai_endpoint();
        let turn = Turn {
            sink: self.candidates.clone_sink(),
            store: Arc::clone(&self.ai_store),
            context: Arc::clone(&self.ai_messages),
            session_id,
        };

        // 优先拦截 /boost 诊断指令，执行全链路状态自检与性能加速报告
        if prompt.trim().starts_with("/boost") {
            let client = StreamChatClient::new(base_url.clone(), api_key, model.clone());
            boost::spawn(
                &self.ai_runtime,
                client,
                (base_url, model),
                stop_signal,
                turn,
            );
            return;
        }

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: SYSTEM_PROMPT.to_string(),
        }];
        if let Some(topic) = self.ai_store.topic() {
            let english = self
                .companion
                .as_ref()
                .filter(|session| session.zh().trim() == topic.trim())
                .map(|session| session.en().to_owned());
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: topic::context_note(&topic, english.as_deref()),
            });
        }
        if let Ok(msgs) = self.ai_messages.lock() {
            let start = msgs.len().saturating_sub(HISTORY_LIMIT);
            messages.extend(msgs[start..].iter().cloned());
        }
        let client = StreamChatClient::new(base_url, api_key, model).with_thinking(deep);
        self.ai_runtime.spawn(async move {
            if web_search {
                let sources = search_sources(&prompt).await;
                if !sources.is_empty() {
                    turn.sink.send_ai_web_sources(sources.clone());
                    add_web_context(&sources, &mut messages);
                }
            }
            if stop_signal.load(Ordering::Relaxed) {
                return;
            }
            stream_answer(client, messages, stop_signal, turn).await;
        });
    }

    /// 接口地址、模型、密钥：环境变量优先，其次配置，面板模型默认 Qwen3 14B。
    fn ai_endpoint(&self) -> (String, String, String) {
        let predict = &self.config.predict;
        let base_url = std::env::var("QINGJIAN_AI_BASE_URL").unwrap_or_else(|_| {
            if predict.base_url.is_empty() {
                DEFAULT_BASE_URL.to_string()
            } else {
                predict.base_url.clone()
            }
        });
        let model =
            std::env::var("QINGJIAN_AI_MODEL").unwrap_or_else(|_| match predict.panel_model() {
                "" => DEFAULT_MODEL.to_string(),
                panel => panel.to_string(),
            });
        let api_key = std::env::var("QINGJIAN_AI_API_KEY").unwrap_or_else(|_| {
            predict.api_key.clone().unwrap_or_else(|| {
                std::env::var("QINGJIAN_API_KEY")
                    .or_else(|_| std::env::var("SILICONFLOW_API_KEY"))
                    .unwrap_or_default()
            })
        });
        (base_url, model, api_key)
    }
}

/// 联网检索；失败或没结果就是空（调用方按纯模型回答）。
async fn search_sources(prompt: &str) -> Vec<SearchResult> {
    match execute_web_search(prompt, 3).await {
        Ok(sources) => sources,
        Err(e) => {
            tracing::warn!(error = %e, "联网搜索不可用，继续纯模型生成");
            Vec::new()
        }
    }
}

/// 检索摘要拼到最后一条用户消息后面。
fn add_web_context(sources: &[SearchResult], messages: &mut [ChatMessage]) {
    let mut web_context = String::from("\n\n[联网检索实时结果]:\n");
    for (i, s) in sources.iter().enumerate() {
        web_context.push_str(&format!(
            "{}. 标题: {}\nURL: {}\n摘要: {}\n\n",
            i + 1,
            s.title,
            s.url,
            s.description
        ));
    }
    if let Some(last) = messages.last_mut() {
        last.content.push_str(&web_context);
    }
}

/// 流式请求并转发：正文与思考过程分别累积，结束 / 被打断 / 出错时把已有的回答记进会话。
async fn stream_answer(
    client: StreamChatClient,
    messages: Vec<ChatMessage>,
    stop_signal: Arc<AtomicBool>,
    turn: Turn,
) {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);
    let chat_stop = stop_signal.clone();
    tokio::spawn(async move {
        client
            .stream_chat(messages, MAX_TOKENS, event_tx, chat_stop)
            .await;
    });

    let mut answer = String::new();
    let mut reasoning = String::new();
    while let Some(event) = event_rx.recv().await {
        if stop_signal.load(Ordering::Relaxed) {
            break;
        }
        match event {
            StreamEvent::Delta(chunk) => {
                answer.push_str(&chunk);
                turn.sink.send_ai_chunk(chunk);
            }
            StreamEvent::Reasoning(chunk) => {
                reasoning.push_str(&chunk);
                turn.sink.send_ai_reasoning(chunk);
            }
            StreamEvent::WebSources(sources) => {
                turn.sink.send_ai_web_sources(sources);
            }
            StreamEvent::Done { ttft_ms, total_ms } => {
                turn.sink.send_ai_finish(ttft_ms, total_ms);
                turn.record(&answer, &reasoning);
                return;
            }
            StreamEvent::Error(err) => {
                turn.sink.send_ai_error(err);
                turn.record(&answer, &reasoning);
                return;
            }
        }
    }
    // 被打断（停止键、换会话、关窗口）：已收到的半截回答照样留下
    turn.record(&answer, &reasoning);
}
