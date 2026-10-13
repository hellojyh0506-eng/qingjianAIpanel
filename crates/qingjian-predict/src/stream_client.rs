//! Qwen 7B 真实流式问答客户端 (SSE 流式增量读取与指标监控)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::thinking::ThinkingSwitch;
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
    reasoning: Option<String>,
}

pub struct StreamChatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,

    /// 允许模型思考；默认关，面板要的是首字快。
    allow_thinking: bool,

    /// 采样温度；问答默认 0.7，学习助手要稳定的结构化输出，用 [`with_temperature`](Self::with_temperature) 调低。
    temperature: f32,
}

impl StreamChatClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .tcp_nodelay(true)
                .connect_timeout(Duration::from_secs(5))
                // SSE 读取空闲 20 秒仍无新数据时中止，避免 UI 永久转圈。
                .read_timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key,
            model,
            allow_thinking: false,
            temperature: 0.7,
        }
    }

    /// 允许思考（「深度」模式）。只对有专用开关的接口有区别。
    pub fn with_thinking(mut self, allow: bool) -> Self {
        self.allow_thinking = allow;
        self
    }

    /// 采样温度。
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// 组请求体：在有专用开关的接口上关思考，其余接口不多发参数。
    fn request_body(
        &self,
        messages: &[ChatMessage],
        stream: bool,
        max_tokens: u32,
        temperature: f32,
    ) -> serde_json::Value {
        let effective_model = if self.model == "auto" {
            "kilo-auto/free"
        } else {
            &self.model
        };
        let mut body = serde_json::json!({
            "model": effective_model,
            "messages": messages,
            "stream": stream,
            "max_tokens": max_tokens,
            "temperature": temperature,
        });
        let switch = ThinkingSwitch::for_url(&self.base_url);
        if !self.allow_thinking && switch.has_dedicated_flag() {
            switch.disable(&mut body);
        }
        body
    }

    /// 快速探测端点联通性与网络握手耗时
    pub async fn probe(&self) -> Result<(u16, u128), String> {
        let t_start = Instant::now();
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.api_key.trim().is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.api_key.trim())) {
                headers.insert(AUTHORIZATION, val);
            }
        } else if self.base_url.contains("kilo.ai") {
            headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer "));
        }

        if self.base_url.contains("kilo.ai") {
            headers.insert("User-Agent", HeaderValue::from_static("opencode-kilo-provider/7.8.8"));
            headers.insert("X-KILOCODE-EDITORNAME", HeaderValue::from_static("Kilo CLI 7.8.8"));
            headers.insert("X-KILOCODE-MACHINEID", HeaderValue::from_static("DESKTOP-FOMGFJO"));
        }

        let messages = [ChatMessage {
            role: "user".to_string(),
            content: "ping".to_string(),
        }];

        let body = self.request_body(&messages, false, 2, 0.1);

        let req = self.http.post(&url).headers(headers).json(&body);
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };

        let status = resp.status().as_u16();
        let latency_ms = t_start.elapsed().as_millis();
        Ok((status, latency_ms))
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
        if !self.api_key.trim().is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.api_key.trim())) {
                headers.insert(AUTHORIZATION, val);
            }
        } else if self.base_url.contains("kilo.ai") {
            headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer "));
        }

        if self.base_url.contains("kilo.ai") {
            headers.insert("User-Agent", HeaderValue::from_static("opencode-kilo-provider/7.8.8"));
            headers.insert("X-KILOCODE-EDITORNAME", HeaderValue::from_static("Kilo CLI 7.8.8"));
            headers.insert("X-KILOCODE-MACHINEID", HeaderValue::from_static("DESKTOP-FOMGFJO"));
        }

        let body = self.request_body(&messages, true, max_tokens, self.temperature);

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
        let mut has_content = false;
        let mut accumulated_reasoning = String::new();

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
                        if !has_content && !accumulated_reasoning.trim().is_empty() {
                            if ttft_ms.is_none() {
                                ttft_ms = Some(t_start.elapsed().as_millis());
                            }
                            let _ = event_tx.send(StreamEvent::Delta(accumulated_reasoning)).await;
                        }
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
                                    if let Some(reasoning) = delta.reasoning {
                                        if !reasoning.is_empty() {
                                            accumulated_reasoning.push_str(&reasoning);
                                        }
                                    }
                                    if let Some(content) = delta.content {
                                        if !content.is_empty() {
                                            has_content = true;
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

        if !has_content && !accumulated_reasoning.trim().is_empty() {
            if ttft_ms.is_none() {
                ttft_ms = Some(t_start.elapsed().as_millis());
            }
            let _ = event_tx.send(StreamEvent::Delta(accumulated_reasoning)).await;
        }

        let total_ms = t_start.elapsed().as_millis();
        let _ = event_tx.send(StreamEvent::Done {
            ttft_ms: ttft_ms.unwrap_or(total_ms),
            total_ms,
        }).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage { role: "user".to_owned(), content: "hi".to_owned() }]
    }

    fn client(base_url: &str) -> StreamChatClient {
        StreamChatClient::new(base_url.to_owned(), "k".to_owned(), "Qwen/Qwen3-14B".to_owned())
    }

    #[test]
    fn siliconflow_stream_requests_turn_thinking_off() {
        let body = client("https://api.siliconflow.cn/v1").request_body(&messages(), true, 2048, 0.7);
        assert_eq!(body["enable_thinking"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["model"], "Qwen/Qwen3-14B");
        assert_eq!(body["max_tokens"], 2048);
    }

    #[test]
    fn thinking_can_be_allowed_for_deep_mode() {
        let body = client("https://api.siliconflow.cn/v1")
            .with_thinking(true)
            .request_body(&messages(), true, 2048, 0.7);
        assert!(body.get("enable_thinking").is_none());
    }

    #[test]
    fn zhipu_uses_its_own_field_and_unknown_hosts_get_nothing_extra() {
        let body = client("https://open.bigmodel.cn/api/paas/v4").request_body(&messages(), true, 10, 0.7);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("enable_thinking").is_none());

        let body = client("https://api.openai.com/v1").request_body(&messages(), true, 10, 0.7);
        assert!(body.get("enable_thinking").is_none());
        assert!(body.get("thinking").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn auto_model_maps_to_kilo_free() {
        let mut c = client("https://api.kilo.ai/v1");
        c.model = "auto".to_owned();
        assert_eq!(c.request_body(&messages(), false, 2, 0.1)["model"], "kilo-auto/free");
    }
}
