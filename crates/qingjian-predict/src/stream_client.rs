//! Qwen 7B 真实流式问答客户端 (SSE 流式增量读取与指标监控)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::web_search::SearchResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 真实到达的文本 Token 增量
    Delta(String),
    /// 联网搜索提取的真实来源列表
    WebSources(Vec<SearchResult>),
    /// 成功结束通知，附带首 Token 延迟 (TTFT) 与总耗时
    Done { ttft_ms: u128, total_ms: u128 },
    /// 错误通知
    Error(String),
}

/// 流式请求体结构
#[derive(Serialize)]
struct ChatCompletionStreamRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct StreamChunkResponse {
    choices: Option<Vec<StreamChoice>>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

pub struct StreamChatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl StreamChatClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .tcp_nodelay(true)
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key,
            model,
        }
    }

    /// 发起流式对话并逐段发送事件。支持通过 stop_signal 立即中止。
    pub async fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: u32,
        event_tx: Sender<StreamEvent>,
        stop_signal: Arc<AtomicBool>,
    ) {
        let t_start = Instant::now();
        let mut ttft_ms: Option<u128> = None;

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.api_key.trim())) {
            headers.insert(AUTHORIZATION, val);
        }

        let body = ChatCompletionStreamRequest {
            model: &self.model,
            messages: &messages,
            stream: true,
            max_tokens,
            temperature: 0.7,
        };

        let req = match self.http.post(&url).headers(headers).json(&body).build() {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx.send(StreamEvent::Error(format!("构建请求失败: {}", e))).await;
                return;
            }
        };

        let mut resp = match self.http.execute(req).await {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx.send(StreamEvent::Error(format!("网络连接失败: {}", e))).await;
                return;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let _ = event_tx.send(StreamEvent::Error(format!("API 响应错误 ({}): {}", status, text))).await;
            return;
        }

        let mut buffer = Vec::new();

        loop {
            if stop_signal.load(Ordering::Relaxed) {
                tracing::info!("检测到用户停止信号，立即中止在途流式读取");
                break;
            }

            let chunk_opt = match resp.chunk().await {
                Ok(opt) => opt,
                Err(e) => {
                    let _ = event_tx.send(StreamEvent::Error(format!("流式读取中断: {}", e))).await;
                    return;
                }
            };

            let Some(chunk) = chunk_opt else {
                // 流已自然结束
                break;
            };

            buffer.extend_from_slice(&chunk);

            // 解析已收集到的完整行
            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                let line = String::from_utf8_lossy(&line_bytes).trim().to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data_str) = line.strip_prefix("data:") {
                    let trimmed = data_str.trim();
                    if trimmed == "[DONE]" {
                        let total_ms = t_start.elapsed().as_millis();
                        let _ = event_tx.send(StreamEvent::Done {
                            ttft_ms: ttft_ms.unwrap_or(total_ms),
                            total_ms,
                        }).await;
                        return;
                    }

                    if let Ok(parsed) = serde_json::from_str::<StreamChunkResponse>(trimmed) {
                        if let Some(choices) = parsed.choices {
                            for choice in choices {
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        if !content.is_empty() {
                                            if ttft_ms.is_none() {
                                                ttft_ms = Some(t_start.elapsed().as_millis());
                                            }
                                            if event_tx.send(StreamEvent::Delta(content)).await.is_err() {
                                                // 接收端已关闭（UI 被用户关闭或切走）
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let total_ms = t_start.elapsed().as_millis();
        let _ = event_tx.send(StreamEvent::Done {
            ttft_ms: ttft_ms.unwrap_or(total_ms),
            total_ms,
        }).await;
    }
}
