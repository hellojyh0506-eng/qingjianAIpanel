//! 各家接口「关思考」的请求体写法，聊天与流式两个客户端共用。

use crate::chat_client::host_of;

/// 各家关思考的参数不一样，严格的接口遇到不认识的参数会报 400，所以按接口地址只发对的那个。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThinkingSwitch {
    /// OpenAI 一系：`reasoning_effort: "none"`，由配置决定是否发。
    ReasoningEffort,

    /// 智谱（`bigmodel.cn` / `z.ai`）：`thinking: {"type": "disabled"}`，不认 `reasoning_effort`。
    ThinkingType,

    /// SiliconFlow / 阿里百炼（DashScope）：`enable_thinking: false`。Qwen3 在这两家默认思考，不关会先想再答。
    EnableThinkingFlag,
}

impl ThinkingSwitch {
    pub(crate) fn for_url(base_url: &str) -> Self {
        let host = host_of(base_url).unwrap_or_default();
        let on_domain = |domains: &[&str]| {
            domains
                .iter()
                .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        };
        if on_domain(&["bigmodel.cn", "z.ai"]) {
            Self::ThinkingType
        } else if on_domain(&[
            "siliconflow.cn",
            "siliconflow.com",
            "dashscope.aliyuncs.com",
            "dashscope-intl.aliyuncs.com",
        ]) {
            Self::EnableThinkingFlag
        } else {
            Self::ReasoningEffort
        }
    }

    /// 这家接口有专用的关思考参数，打字类请求可以无视配置强制关闭。
    pub(crate) fn has_dedicated_flag(self) -> bool {
        !matches!(self, Self::ReasoningEffort)
    }

    /// 把请求体改成这家接口关思考的写法；`ReasoningEffort` 不动请求体。
    pub(crate) fn disable(self, body: &mut serde_json::Value) {
        let Some(fields) = body.as_object_mut() else {
            return;
        };
        match self {
            Self::ReasoningEffort => {}
            Self::ThinkingType => {
                fields.remove("reasoning_effort");
                fields.insert(
                    "thinking".to_owned(),
                    serde_json::json!({ "type": "disabled" }),
                );
            }
            Self::EnableThinkingFlag => {
                fields.remove("reasoning_effort");
                fields.insert("enable_thinking".to_owned(), serde_json::Value::Bool(false));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn siliconflow_and_dashscope_use_enable_thinking_flag() {
        for url in [
            "https://api.siliconflow.cn/v1",
            "https://api.siliconflow.com/v1",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        ] {
            assert_eq!(
                ThinkingSwitch::for_url(url),
                ThinkingSwitch::EnableThinkingFlag,
                "{url}"
            );
        }
        let mut body = serde_json::json!({ "model": "Qwen/Qwen3-14B", "reasoning_effort": "none" });
        ThinkingSwitch::for_url("https://api.siliconflow.cn/v1").disable(&mut body);
        assert_eq!(body["enable_thinking"], false);
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn other_hosts_keep_reasoning_effort_and_ignore_lookalike_paths() {
        let mut body = serde_json::json!({ "model": "x", "reasoning_effort": "none" });
        ThinkingSwitch::for_url("https://api.deepseek.com").disable(&mut body);
        assert_eq!(body["reasoning_effort"], "none");
        assert!(body.get("thinking").is_none());
        assert!(body.get("enable_thinking").is_none());
        assert_eq!(
            ThinkingSwitch::for_url("https://example.com/bigmodel.cn"),
            ThinkingSwitch::ReasoningEffort
        );
        assert_eq!(
            ThinkingSwitch::for_url("https://example.com/siliconflow.cn"),
            ThinkingSwitch::ReasoningEffort
        );
        assert!(!ThinkingSwitch::ReasoningEffort.has_dedicated_flag());
        assert!(ThinkingSwitch::EnableThinkingFlag.has_dedicated_flag());
    }
}
