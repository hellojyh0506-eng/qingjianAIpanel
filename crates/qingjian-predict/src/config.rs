use qingjian_core::PredictionPolicy;
use serde::{Deserialize, Serialize};

/// 云联想配置。默认**关闭**，开启后光标附近的文本会发往 `base_url`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PredictConfig {
    /// 是否启用。
    pub enabled: bool,

    /// OpenAI 兼容接口地址（不含 `/chat/completions`）。
    pub base_url: String,

    /// 模型名；`typing_model` / `panel_model` 留空时都回落到它。
    pub model: String,

    /// 打字链路（重排、纠错、翻译）用的模型，追求快；留空回落到 `model`。
    pub typing_model: String,

    /// 学习助手与 AI 问答面板用的模型，追求质量；留空回落到 `model`。
    pub panel_model: String,

    /// 密钥。留空则读 `api_key_env` 指定的环境变量。
    pub api_key: Option<String>,

    /// 存放密钥的环境变量名。
    pub api_key_env: String,

    /// 单次请求超时（毫秒），超时即丢。
    pub timeout_ms: u64,

    /// 防抖：停止敲键多久之后才真正发请求（毫秒）。
    pub debounce_ms: u64,

    /// 光标前最多发多少个字符。
    pub lookback: usize,

    /// 光标后最多发多少个字符。
    pub lookahead: usize,

    /// 云端词最多补进候选窗口第一页末尾几格；0 表示不要云端词，只要整句补全。
    pub slots: usize,

    /// 组句中要不要整句补全。
    pub sentence: bool,

    /// 推理强度，随请求发 `reasoning_effort`：`none` 关掉模型的思考（联想要的是快，不是想），
    /// 其余 minimal / low / medium / high / xhigh 照传；留空则不发（给不认这个参数的接口）。
    /// DeepSeek V4 这类默认带思考的模型不关会把 token 预算全花在思考上，正文为空。
    pub reasoning_effort: String,
}

/// 默认打字模型：按 `docs/notes/ai-model-bench.md` 的实测选的（上下文选字最准、P95 最低、价格最低）。
/// 只在默认的 SiliconFlow 接口上有意义；换了接口的用户要在配置里自己填。
pub const DEFAULT_TYPING_MODEL: &str = "Qwen/Qwen3.5-35B-A3B";

impl Default for PredictConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "https://api.siliconflow.cn/v1".to_owned(),
            model: "Qwen/Qwen3-14B".to_owned(),
            typing_model: DEFAULT_TYPING_MODEL.to_owned(),
            panel_model: String::new(),
            api_key: None,
            api_key_env: "QINGJIAN_API_KEY".to_owned(),
            timeout_ms: 3000,
            debounce_ms: 150,
            lookback: 64,
            lookahead: 32,
            slots: 2,
            sentence: true,
            reasoning_effort: "".to_owned(),
        }
    }
}

impl PredictConfig {
    /// 打字链路实际用的模型名。
    pub fn typing_model(&self) -> &str {
        non_blank(&self.typing_model).unwrap_or_else(|| self.model.trim())
    }

    /// 面板（学习助手、AI 问答）实际用的模型名。
    pub fn panel_model(&self) -> &str {
        non_blank(&self.panel_model).unwrap_or_else(|| self.model.trim())
    }

    /// 配置里的密钥优先，其次环境变量；两边都没有返回 `None`。
    pub fn resolve_api_key(&self) -> Option<String> {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_owned)
            .or_else(|| std::env::var(&self.api_key_env).ok())
            .or_else(|| std::env::var("KILO_API_KEY").ok())
            .or_else(|| std::env::var("QINGJIAN_API_KEY").ok())
            .filter(|k| !k.trim().is_empty())
    }

    pub fn policy(&self) -> PredictionPolicy {
        PredictionPolicy {
            before: self.lookback,
            after: self.lookahead,
            slots: self.slots,
            // 比槽位多要两条，与本地候选重复的去掉后还能填满；问字模式的答案也按这个数要
            max_items: self.slots.max(1) + 2,
            sentence: self.sentence,
        }
    }
}

fn non_blank(value: &str) -> Option<&str> {
    Some(value.trim()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_fall_back_to_the_shared_model() {
        let mut config = PredictConfig::default();
        assert_eq!(config.typing_model(), DEFAULT_TYPING_MODEL);
        assert_eq!(config.panel_model(), "Qwen/Qwen3-14B");

        config.typing_model = String::new();
        assert_eq!(config.typing_model(), "Qwen/Qwen3-14B");

        config.typing_model = " Qwen/Qwen3-8B ".to_owned();
        assert_eq!(config.typing_model(), "Qwen/Qwen3-8B");
        assert_eq!(config.panel_model(), "Qwen/Qwen3-14B");

        config.panel_model = "x/panel".to_owned();
        config.model = String::new();
        assert_eq!(config.panel_model(), "x/panel");
    }
}
