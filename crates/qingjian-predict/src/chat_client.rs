use std::sync::LazyLock;
use std::time::Duration;

use async_openai::types::chat::{

    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs,
    CreateChatCompletionResponse, FinishReason, ReasoningEffort, ResponseFormat,
};
use qingjian_core::PredictionRequest;
use reqwest::header::{HeaderMap, HeaderValue};

use crate::config::PredictConfig;
use crate::error::PredictError;
use crate::prompt::{self, Reply};

/// 联想与翻译回复的 token 上限：口语即时翻译 120，组句 500，问字 300，对比评测 800，深度学习 1800（留足思考空间）。
const TRANSLATE_MAX_TOKENS: u32 = 120;
const COMPOSE_MAX_TOKENS: u32 = 500;
const RERANK_MAX_TOKENS: u32 = 512;
const QUESTION_MAX_TOKENS: u32 = 300;
const COMPARE_MAX_TOKENS: u32 = 800;
const DEEP_LEARN_MAX_TOKENS: u32 = 1800;

/// 采样温度：联想要稳，不要花。
const TEMPERATURE: f32 = 0.3;

/// OpenCode Zen / Go 自 2026-09-06 起要求每个请求带这个头，值是同一会话内稳定的 ID，
/// 他们靠它把同一会话路由到同一上游复用 prompt 缓存，缺了直接 400。
const OPENCODE_SESSION_HEADER: &str = "x-opencode-session";

/// 本进程的会话 ID：输入法没有「会话」概念，一个进程算一个，进程内所有客户端共用。
static SESSION_ID: LazyLock<String> = LazyLock::new(|| uuid::Uuid::new_v4().to_string());

/// OpenAI 兼容聊天接口的封装：一个请求进、若干联想条目出。
pub struct ChatClient {
    /// 底层 HTTP 客户端。
    http: reqwest::Client,

    /// 接口基础地址。
    base_url: String,

    /// API 密钥（绝不输出到日志）。
    api_key: String,

    /// 模型名。
    model: String,

    /// 超时。
    timeout: Duration,

    /// 推理强度；`None` 表示不发这个参数。
    reasoning_effort: Option<ReasoningEffort>,

    /// 接口关思考用的是哪种参数（按接口地址定）。
    thinking_switch: ThinkingSwitch,
}

impl ChatClient {
    pub fn new(config: &PredictConfig, api_key: String) -> Self {
        Self {
            http: http_client(&config.base_url),
            base_url: config.base_url.clone(),
            api_key,
            model: config.model.clone(),
            timeout: Duration::from_millis(config.timeout_ms),
            reasoning_effort: parse_reasoning_effort(&config.reasoning_effort),
            thinking_switch: ThinkingSwitch::for_url(&config.base_url),
        }
    }

    pub async fn complete(&self, request: &PredictionRequest) -> Result<Reply, PredictError> {
        let user = prompt::user_prompt(request);
        tracing::debug!(sequence = request.sequence, %user, "联想请求");
        let max_tokens = match request.kind {
            qingjian_core::PredictionKind::Translate => TRANSLATE_MAX_TOKENS,
            qingjian_core::PredictionKind::Compose => COMPOSE_MAX_TOKENS,
            qingjian_core::PredictionKind::Rerank => RERANK_MAX_TOKENS,
            qingjian_core::PredictionKind::Question => QUESTION_MAX_TOKENS,
            qingjian_core::PredictionKind::Compare => COMPARE_MAX_TOKENS,
            qingjian_core::PredictionKind::DeepLearn => DEEP_LEARN_MAX_TOKENS,
        };
        // 快速翻译采用有限总耗时；完整学习与对比请求可以使用更长预算。
        // 具体重试/读响应超时统一由 chat_with_timeout 的总 deadline 约束。
        let effective_timeout = match request.kind {
            qingjian_core::PredictionKind::Translate => self.timeout.max(Duration::from_millis(4500)),
            qingjian_core::PredictionKind::DeepLearn => self.timeout.max(Duration::from_millis(15000)),
            qingjian_core::PredictionKind::Compare => self.timeout.max(Duration::from_millis(10000)),
            _ => self.timeout,
        };
        let content = self
            .chat_with_timeout(prompt::system_prompt(request), &user, max_tokens, effective_timeout)
            .await?;
        let t_val = std::time::Instant::now();
        let reply = prompt::parse_reply(&content, request);
        let validation_ms = t_val.elapsed().as_millis();
        tracing::info!(sequence = request.sequence, validation_ms, "Translation校验完成");
        Ok(reply)
    }

    /// 一问一答：系统提示 + 用户消息，要 JSON 对象，返回正文。联想与释义兜底共用。
    pub async fn chat(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<String, PredictError> {
        self.chat_with_timeout(system, user, max_tokens, self.timeout).await
    }

    /// 支持自定义超时与自动故障重试的请求执行器
    pub async fn chat_with_timeout(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String, PredictError> {
        let t_start = std::time::Instant::now();
        let deadline = tokio::time::Instant::now() + timeout;
        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessage::from(system).into(),
            ChatCompletionRequestUserMessage::from(user).into(),
        ];
        let effective_model = if self.model == "auto" {
            "kilo-auto/free"
        } else {
            &self.model
        };
        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(effective_model)
            .messages(messages)
            .max_tokens(max_tokens)
            .temperature(TEMPERATURE)
            .response_format(ResponseFormat::JsonObject);
        if let Some(effort) = self.reasoning_effort.clone() {
            args.reasoning_effort(effort);
        }
        let mut body = serde_json::to_value(args.build()?)?;
        if matches!(self.reasoning_effort, Some(ReasoningEffort::None)) {
            self.thinking_switch.disable(&mut body);
        }
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        // 所有尝试共享一个总 deadline，响应体读取也受限；不能每次重试都再等完整 timeout。
        let max_attempts = 3;
        let mut last_err = None;

        for attempt in 0..max_attempts {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            if attempt > 0 {
                let backoff = Duration::from_millis(250 * attempt as u64).min(remaining);
                tracing::warn!(attempt, delay_ms = backoff.as_millis(), "正在进行LLM网关自动重试...");
                tokio::time::sleep(backoff).await;
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
            }

            let t_http_start = std::time::Instant::now();
            let mut req = self.http.post(&url);
            if !self.api_key.trim().is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.api_key.trim()));
            } else if self.base_url.contains("kilo.ai") {
                req = req.header("Authorization", "Bearer ");
            }
            let req = req.json(&body);
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let send_res = tokio::time::timeout(remaining, req.send()).await;
            let resp = match send_res {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    tracing::warn!(attempt, %e, "HTTP请求发送失败");
                    last_err = Some(PredictError::Http(e));
                    if attempt + 1 < max_attempts { continue; }
                    break;
                }
                Err(_) => {
                    tracing::warn!(attempt, timeout_ms = timeout.as_millis(), "HTTP请求到达总超时期限");
                    last_err = Some(PredictError::Timeout(timeout.as_millis() as u64));
                    break;
                }
            };

            let first_byte_ms = t_http_start.elapsed().as_millis();
            let status = resp.status();
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let raw_text = match tokio::time::timeout(remaining, resp.text()).await {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    last_err = Some(PredictError::Http(e));
                    if attempt + 1 < max_attempts { continue; }
                    break;
                }
                Err(_) => {
                    last_err = Some(PredictError::Timeout(timeout.as_millis() as u64));
                    break;
                }
            };
            let http_ms = t_http_start.elapsed().as_millis();

            // 若遇到 429 (限流) 或 5xx (网关故障)，进行重试
            if !status.is_success() {
                let code = status.as_u16();
                tracing::warn!(attempt, status = code, response = %raw_text, "LLM网关返回非成功状态码");
                let err_msg = serde_json::from_str::<serde_json::Value>(&raw_text)
                    .ok()
                    .and_then(|v| v.get("error").cloned())
                    .map(|e| e.get("message").and_then(|m| m.as_str()).unwrap_or(&e.to_string()).to_string())
                    .unwrap_or_else(|| format!("HTTP status {code}: {raw_text}"));

                last_err = Some(PredictError::Gateway(code, err_msg));
                if (code == 429 || code >= 500) && attempt + 1 < max_attempts {
                    continue;
                }
                break;
            }

            let t_parse_start = std::time::Instant::now();
            let raw: serde_json::Value = match serde_json::from_str(&raw_text) {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(PredictError::Encode(e));
                    continue;
                }
            };
            let response: Option<CreateChatCompletionResponse> = serde_json::from_value(raw.clone()).ok();
            let parse_ms = t_parse_start.elapsed().as_millis();
            let cut_off = response.as_ref().map_or(false, |r| {
                r.choices.iter().any(|choice| choice.finish_reason == Some(FinishReason::Length))
            });
            let content = response
                .and_then(|r| {
                    r.choices
                        .into_iter()
                        .inspect(
                            |choice| tracing::debug!(finish_reason = ?choice.finish_reason, "联想回复结束原因"),
                        )
                        .find_map(|choice| choice.message.content.filter(|c| !c.trim().is_empty()))
                })
                .or_else(|| {
                    let msg = raw.get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("message"));
                    msg.and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            msg.and_then(|m| m.get("reasoning_content").or_else(|| m.get("reasoning")))
                                .and_then(|r| r.as_str())
                                .filter(|s| !s.trim().is_empty())
                                .map(|s| s.to_string())
                        })
                })
                .ok_or_else(|| {
                    tracing::warn!(model = %self.model, response = %truncated(&raw), "接口回复里没有正文");
                    if cut_off {
                        PredictError::BudgetExhausted
                    } else {
                        PredictError::EmptyReply
                    }
                })?;
            let request_id = raw
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let provider = if self.base_url.contains("kilo.ai") {
                "Kilo Gateway"
            } else if self.base_url.contains("siliconflow") {
                "SiliconFlow"
            } else if self.base_url.contains("deepseek") {
                "DeepSeek"
            } else if self.base_url.contains("openai") {
                "OpenAI"
            } else {
                "OpenAI Compatible"
            };
            let total_ms = t_start.elapsed().as_millis();
            tracing::info!(
                provider = %provider,
                endpoint = %self.base_url,
                model = %self.model,
                request_id = %request_id,
                first_byte_ms = first_byte_ms,
                http_ms = http_ms,
                parse_ms = parse_ms,
                total_ms = total_ms,
                status = %status,
                content_chars = content.len(),
                "HTTP完成与耗时分解"
            );
            tracing::debug!(%content, "模型回复");
            return Ok(content);
        }

        Err(last_err.unwrap_or(PredictError::EmptyReply))
    }
}

/// 日志里的原始响应最多留这么多字符。
const LOGGED_RESPONSE_CHARS: usize = 2000;

fn truncated(response: &serde_json::Value) -> String {
    let text = response.to_string();
    match text.char_indices().nth(LOGGED_RESPONSE_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// 各家关思考的参数不一样，严格的接口遇到不认识的参数会报 400，所以按接口地址只发对的那个。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThinkingSwitch {
    /// OpenAI 一系：`reasoning_effort: "none"`，请求里已经带了。
    ReasoningEffort,

    /// 智谱（`bigmodel.cn` / `z.ai`）：`thinking: {"type": "disabled"}`，不认 `reasoning_effort`。
    ThinkingType,
}

impl ThinkingSwitch {
    fn for_url(base_url: &str) -> Self {
        let host = host_of(base_url).unwrap_or_default();
        let zhipu = ["bigmodel.cn", "z.ai"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
        if zhipu {
            Self::ThinkingType
        } else {
            Self::ReasoningEffort
        }
    }

    /// 把请求体改成这家接口关思考的写法。
    fn disable(self, body: &mut serde_json::Value) {
        let (Self::ThinkingType, Some(fields)) = (self, body.as_object_mut()) else {
            return;
        };
        fields.remove("reasoning_effort");
        fields.insert(
            "thinking".to_owned(),
            serde_json::json!({ "type": "disabled" }),
        );
    }
}

fn host_of(base_url: &str) -> Option<String> {
    reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

/// 按接口地址决定 HTTP 客户端：国内服务直连（绕过死代理），OpenCode 带上它要求的会话头。
fn http_client(base_url: &str) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(1500))
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30));
    if is_domestic(base_url) {
        builder = builder.no_proxy();
    }
    if is_opencode(base_url) {
        if let Ok(value) = HeaderValue::from_str(&SESSION_ID) {
            let mut headers = HeaderMap::new();
            headers.insert(OPENCODE_SESSION_HEADER, value);
            builder = builder.default_headers(headers);
        }
    }
    if is_kilo(base_url) {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static("opencode-kilo-provider/7.8.8"),
        );
        headers.insert(
            "X-KILOCODE-EDITORNAME",
            HeaderValue::from_static("Kilo CLI 7.8.8"),
        );
        headers.insert(
            "X-KILOCODE-MACHINEID",
            HeaderValue::from_static("DESKTOP-FOMGFJO"),
        );
        builder = builder.default_headers(headers);
    }
    builder.build().unwrap_or_default()
}

/// 接口地址是否指向 Kilo Gateway（`kilo.ai` 及其子域）。
fn is_kilo(base_url: &str) -> bool {
    host_of(base_url).is_some_and(|host| host == "kilo.ai" || host.ends_with(".kilo.ai"))
}

/// 接口地址是否指向国内服务，若是则直连，避免被死代理（如 127.0.0.1:7897）拖死。
fn is_domestic(base_url: &str) -> bool {
    host_of(base_url).is_some_and(|host| {
        host.ends_with(".cn")
            || host == "deepseek.com"
            || host.ends_with(".deepseek.com")
            || host == "z.ai"
            || host.ends_with(".z.ai")
            || host == "minimaxi.com"
            || host.ends_with(".minimaxi.com")
            || host == "stepfun.com"
            || host.ends_with(".stepfun.com")
            || host == "baichuan-ai.com"
            || host.ends_with(".baichuan-ai.com")
            || host == "localhost"
            || host == "127.0.0.1"
    })
}

/// 接口地址是否指向 OpenCode（`opencode.ai` 及其子域）。
fn is_opencode(base_url: &str) -> bool {
    host_of(base_url).is_some_and(|host| host == "opencode.ai" || host.ends_with(".opencode.ai"))
}

/// 配置里的推理强度字符串转成接口枚举；留空不发，认不得的值当留空并记一条警告。
fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffort> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "none" => Some(ReasoningEffort::None),
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        other => {
            tracing::warn!(value = other, "reasoning_effort 不认识，不发这个参数");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_effort_parses_known_values_and_ignores_the_rest() {
        assert!(matches!(
            parse_reasoning_effort("none"),
            Some(ReasoningEffort::None)
        ));
        assert!(matches!(
            parse_reasoning_effort(" High "),
            Some(ReasoningEffort::High)
        ));
        assert!(parse_reasoning_effort("").is_none());
        assert!(parse_reasoning_effort("maximum").is_none());
    }

    #[test]
    fn zhipu_hosts_disable_thinking_with_their_own_field() {
        let mut body = serde_json::json!({ "model": "glm", "reasoning_effort": "none" });
        ThinkingSwitch::for_url("https://open.bigmodel.cn/api/paas/v4").disable(&mut body);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").is_none());
        assert_eq!(
            ThinkingSwitch::for_url("https://api.z.ai/api/paas/v4"),
            ThinkingSwitch::ThinkingType
        );

        let mut body = serde_json::json!({ "model": "x", "reasoning_effort": "none" });
        ThinkingSwitch::for_url("https://api.deepseek.com").disable(&mut body);
        assert_eq!(body["reasoning_effort"], "none");
        assert!(body.get("thinking").is_none());
        assert_eq!(
            ThinkingSwitch::for_url("https://example.com/bigmodel.cn"),
            ThinkingSwitch::ReasoningEffort
        );
    }

    #[test]
    fn opencode_is_recognized_by_host_only() {
        assert!(is_opencode("https://opencode.ai/zen/go/v1"));
        assert!(is_opencode("https://OpenCode.ai/zen/v1/"));
        assert!(is_opencode("https://api.opencode.ai/v1"));
        assert!(!is_opencode("https://api.deepseek.com"));
        assert!(!is_opencode("https://example.com/opencode.ai"));
        assert!(!is_opencode("not a url"));
    }

    #[test]
    fn domestic_hosts_are_recognized() {
        assert!(is_domestic("https://api.siliconflow.cn/v1"));
        assert!(is_domestic("https://open.bigmodel.cn/api/paas/v4"));
        assert!(is_domestic("https://api.deepseek.com"));
        assert!(is_domestic("https://api.z.ai/v1"));
        assert!(is_domestic("http://127.0.0.1:11434/v1"));
        assert!(is_domestic("http://localhost:8080/v1"));
        assert!(!is_domestic("https://api.openai.com/v1"));
        assert!(!is_domestic("https://opencode.ai/zen/go/v1"));
    }

    #[test]
    fn session_id_is_stable_within_the_process() {
        assert_eq!(*SESSION_ID, *SESSION_ID);
        assert_eq!(SESSION_ID.len(), 36);
    }

    /// 真发一个请求到本地端口，看 OpenCode 客户端带了会话头、默认客户端没带。
    #[test]
    fn opencode_client_sends_the_session_header() {
        assert_eq!(
            header_seen_by_server(http_client("https://opencode.ai/zen/go/v1")),
            Some(SESSION_ID.clone())
        );
        assert_eq!(
            header_seen_by_server(http_client("https://api.deepseek.com")),
            None
        );
    }

    /// 起一个只答一次的 HTTP 服务，返回请求里 `x-opencode-session` 的值。
    fn header_seen_by_server(client: reqwest::Client) -> Option<String> {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = None;
            for line in BufReader::new(&stream).lines() {
                let line = line.unwrap();
                if line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case(OPENCODE_SESSION_HEADER)
                {
                    header = Some(value.trim().to_owned());
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .unwrap();
            header
        });
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { client.get(&url).send().await.unwrap() });
        server.join().unwrap()
    }
}
