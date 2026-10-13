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
use crate::thinking::ThinkingSwitch;

/// 联想与翻译回复的 token 上限：口语即时翻译 120，组句 500，问字 300，对比评测 800，深度学习 1800（留足思考空间）。
const TRANSLATE_MAX_TOKENS: u32 = 120;
const COMPOSE_MAX_TOKENS: u32 = 500;
const RERANK_MAX_TOKENS: u32 = 512;
const QUESTION_MAX_TOKENS: u32 = 300;
const COMPARE_MAX_TOKENS: u32 = 800;
const DEEP_LEARN_MAX_TOKENS: u32 = 1800;

/// 兜底采样温度（释义兜底、连通性测试）。
const TEMPERATURE: f32 = 0.3;

/// 预热请求的总时限；只为让连接握手完成，失败无所谓。
const WARM_UP_TIMEOUT: Duration = Duration::from_millis(3000);

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

    /// 打字链路用的模型。
    typing_model: String,

    /// 学习 / 对比等面板链路用的模型。
    panel_model: String,

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
            typing_model: config.typing_model().to_owned(),
            panel_model: config.panel_model().to_owned(),
            timeout: Duration::from_millis(config.timeout_ms),
            reasoning_effort: parse_reasoning_effort(&config.reasoning_effort),
            thinking_switch: ThinkingSwitch::for_url(&config.base_url),
        }
    }

    pub async fn complete(&self, request: &PredictionRequest) -> Result<Reply, PredictError> {
        let user = prompt::user_prompt(request);
        tracing::debug!(sequence = request.sequence, %user, "联想请求");
        let profile = CallProfile::of(request.kind);
        // 快速翻译采用有限总耗时；完整学习与对比请求可以使用更长预算。
        // 具体重试/读响应超时统一由 chat_with_timeout 的总 deadline 约束。
        let effective_timeout = match request.kind {
            qingjian_core::PredictionKind::Translate => self.timeout.max(Duration::from_millis(4500)),
            qingjian_core::PredictionKind::DeepLearn => self.timeout.max(Duration::from_millis(15000)),
            qingjian_core::PredictionKind::Compare => self.timeout.max(Duration::from_millis(10000)),
            _ => self.timeout,
        };
        let model = if profile.panel { &self.panel_model } else { &self.typing_model };
        let call = Call {
            model,
            max_tokens: profile.max_tokens,
            temperature: profile.temperature,
            allow_thinking: profile.allow_thinking,
            timeout: effective_timeout,
        };
        let content = self.chat_with(prompt::system_prompt(request), &user, &call).await?;
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

    /// 打字链路的一问一答，自定义总超时。
    pub async fn chat_with_timeout(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<String, PredictError> {
        let call = Call {
            model: &self.typing_model,
            max_tokens,
            temperature: TEMPERATURE,
            allow_thinking: false,
            timeout,
        };
        self.chat_with(system, user, &call).await
    }

    /// 返回一个自带所有权的预热任务：发一个 1 token 的请求，把握手和连接池准备好。失败静默。
    pub fn warm_up_task(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        let http = self.http.clone();
        let url = self.chat_url();
        let authorization = self.authorization();
        let body = self.warm_up_body();
        async move {
            let mut req = http.post(url).json(&body);
            if let Some(value) = authorization {
                req = req.header("Authorization", value);
            }
            let sent = tokio::time::timeout(WARM_UP_TIMEOUT, async {
                let resp = req.send().await?;
                resp.bytes().await.map(|bytes| bytes.len())
            })
            .await;
            tracing::debug!(ok = matches!(sent, Ok(Ok(_))), "连接预热完成");
        }
    }

    /// 支持自定义超时与自动故障重试的请求执行器
    async fn chat_with(
        &self,
        system: &str,
        user: &str,
        call: &Call<'_>,
    ) -> Result<String, PredictError> {
        let timeout = call.timeout;
        let t_start = std::time::Instant::now();
        let deadline = tokio::time::Instant::now() + timeout;
        let body = self.request_body(system, user, call)?;
        let url = self.chat_url();

        // 所有尝试共享一个总 deadline，响应体读取也受限；不能每次重试都再等完整 timeout。
        let max_attempts = 3;
        let mut last_err = None;
        let mut rate_limited = false;

        for attempt in 0..max_attempts {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            if attempt > 0 {
                let backoff = retry_backoff(attempt, rate_limited).min(remaining);
                tracing::warn!(attempt, delay_ms = backoff.as_millis(), "正在进行LLM网关自动重试...");
                tokio::time::sleep(backoff).await;
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
            }

            let t_http_start = std::time::Instant::now();
            let mut req = self.http.post(&url);
            if let Some(value) = self.authorization() {
                req = req.header("Authorization", value);
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
                rate_limited = code == 429;
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
                    tracing::warn!(model = %call.model, response = %truncated(&raw), "接口回复里没有正文");
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
                model = %call.model,
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

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn authorization(&self) -> Option<String> {
        if !self.api_key.trim().is_empty() {
            Some(format!("Bearer {}", self.api_key.trim()))
        } else if self.base_url.contains("kilo.ai") {
            Some("Bearer ".to_owned())
        } else {
            None
        }
    }

    /// 组请求体。打字类请求在有专用关思考参数的接口上一律关思考，不受配置影响；
    /// 允许思考的请求（深度学习、对比）和不认识的接口仍按 `reasoning_effort` 配置走。
    fn request_body(
        &self,
        system: &str,
        user: &str,
        call: &Call<'_>,
    ) -> Result<serde_json::Value, PredictError> {
        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessage::from(system).into(),
            ChatCompletionRequestUserMessage::from(user).into(),
        ];
        let effective_model = if call.model == "auto" {
            "kilo-auto/free"
        } else {
            call.model
        };
        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(effective_model)
            .messages(messages)
            .max_tokens(call.max_tokens)
            .temperature(call.temperature)
            .response_format(ResponseFormat::JsonObject);
        if let Some(effort) = self.reasoning_effort.clone() {
            args.reasoning_effort(effort);
        }
        let mut body = serde_json::to_value(args.build()?)?;
        let forced_off = !call.allow_thinking && self.thinking_switch.has_dedicated_flag();
        if forced_off || matches!(self.reasoning_effort, Some(ReasoningEffort::None)) {
            self.thinking_switch.disable(&mut body);
        }
        Ok(body)
    }

    fn warm_up_body(&self) -> serde_json::Value {
        let model = if self.typing_model == "auto" {
            "kilo-auto/free"
        } else {
            &self.typing_model
        };
        let mut body = serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": "." }],
            "max_tokens": 1,
            "temperature": 0.0,
        });
        self.thinking_switch.disable(&mut body);
        body
    }
}

/// 一次请求的可变参数。
struct Call<'a> {
    /// 模型名。
    model: &'a str,

    /// 回复 token 上限。
    max_tokens: u32,

    /// 采样温度。
    temperature: f32,

    /// 允许模型思考；`false` 时在有专用开关的接口上强制关闭。
    allow_thinking: bool,

    /// 总超时（含重试）。
    timeout: Duration,
}

/// 各类请求的固定参数。
struct CallProfile {
    max_tokens: u32,
    temperature: f32,
    allow_thinking: bool,

    /// 走面板模型而不是打字模型。
    panel: bool,
}

impl CallProfile {
    /// 打字类温度 0（重排、纠错要稳）；翻译 0.3；面板类 0.5。只有深度学习与对比允许思考。
    fn of(kind: qingjian_core::PredictionKind) -> Self {
        use qingjian_core::PredictionKind as Kind;
        let (max_tokens, temperature, allow_thinking, panel) = match kind {
            Kind::Translate => (TRANSLATE_MAX_TOKENS, 0.3, false, false),
            Kind::Compose => (COMPOSE_MAX_TOKENS, 0.3, false, false),
            Kind::Intent => (96, 0.0, false, false),
            Kind::Rerank => (RERANK_MAX_TOKENS, 0.0, false, false),
            Kind::Question => (QUESTION_MAX_TOKENS, 0.3, false, false),
            Kind::Compare => (COMPARE_MAX_TOKENS, 0.5, true, true),
            Kind::DeepLearn => (DEEP_LEARN_MAX_TOKENS, 0.5, true, true),
        };
        Self { max_tokens, temperature, allow_thinking, panel }
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

pub(crate) fn host_of(base_url: &str) -> Option<String> {
    reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

/// 按接口地址决定 HTTP 客户端：国内服务直连（绕过死代理），OpenCode 带上它要求的会话头。
fn http_client(base_url: &str) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(1500))
        .pool_idle_timeout(Duration::from_secs(300))
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

/// 第 `attempt`（从 1 开始）次重试前的等待：指数退避；限流（429）基数更大，避免立刻再撞限流。
fn retry_backoff(attempt: u32, rate_limited: bool) -> Duration {
    let base_ms: u64 = if rate_limited { 400 } else { 250 };
    Duration::from_millis(base_ms << attempt.saturating_sub(1).min(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_grows_exponentially_and_waits_longer_on_rate_limit() {
        assert_eq!(retry_backoff(1, false), Duration::from_millis(250));
        assert_eq!(retry_backoff(2, false), Duration::from_millis(500));
        assert_eq!(retry_backoff(1, true), Duration::from_millis(400));
        assert_eq!(retry_backoff(2, true), Duration::from_millis(800));
        assert!(retry_backoff(30, true) <= Duration::from_secs(10));
    }

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

    fn client_for(base_url: &str, effort: &str) -> ChatClient {
        let config = PredictConfig {
            base_url: base_url.to_owned(),
            typing_model: "Qwen/Qwen3-8B".to_owned(),
            reasoning_effort: effort.to_owned(),
            ..PredictConfig::default()
        };
        ChatClient::new(&config, "k".to_owned())
    }

    fn body_for(client: &ChatClient, kind: qingjian_core::PredictionKind) -> serde_json::Value {
        let profile = CallProfile::of(kind);
        let model = if profile.panel { &client.panel_model } else { &client.typing_model };
        let call = Call {
            model,
            max_tokens: profile.max_tokens,
            temperature: profile.temperature,
            allow_thinking: profile.allow_thinking,
            timeout: Duration::from_secs(1),
        };
        client.request_body("s", "u", &call).unwrap()
    }

    #[test]
    fn typing_requests_force_thinking_off_on_siliconflow_even_without_config() {
        use qingjian_core::PredictionKind as Kind;
        let client = client_for("https://api.siliconflow.cn/v1", "");
        for kind in [Kind::Rerank, Kind::Intent, Kind::Translate, Kind::Compose, Kind::Question] {
            let body = body_for(&client, kind);
            assert_eq!(body["enable_thinking"], false, "{kind:?}");
            assert_eq!(body["model"], "Qwen/Qwen3-8B", "{kind:?}");
            assert!(body.get("reasoning_effort").is_none(), "{kind:?}");
        }
        assert_eq!(body_for(&client, Kind::Intent)["temperature"], 0.0);
        assert!((body_for(&client, Kind::Translate)["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn deep_requests_use_panel_model_and_keep_thinking_unless_configured_off() {
        use qingjian_core::PredictionKind as Kind;
        let body = body_for(&client_for("https://api.siliconflow.cn/v1", ""), Kind::DeepLearn);
        assert!(body.get("enable_thinking").is_none());
        assert_eq!(body["model"], "Qwen/Qwen3-14B");

        let body = body_for(&client_for("https://api.siliconflow.cn/v1", "none"), Kind::Compare);
        assert_eq!(body["enable_thinking"], false);
        assert_eq!(body["model"], "Qwen/Qwen3-14B");
    }

    #[test]
    fn unknown_hosts_follow_the_config_only() {
        use qingjian_core::PredictionKind as Kind;
        let body = body_for(&client_for("https://api.deepseek.com", ""), Kind::Rerank);
        assert!(body.get("enable_thinking").is_none());
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("thinking").is_none());

        let body = body_for(&client_for("https://api.deepseek.com", "none"), Kind::Rerank);
        assert_eq!(body["reasoning_effort"], "none");
    }

    #[test]
    fn warm_up_body_is_one_token_with_thinking_off() {
        let body = client_for("https://api.siliconflow.cn/v1", "").warm_up_body();
        assert_eq!(body["max_tokens"], 1);
        assert_eq!(body["enable_thinking"], false);
        assert_eq!(body["model"], "Qwen/Qwen3-8B");
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
