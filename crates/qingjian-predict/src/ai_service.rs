//! 统一服务端 AI Service 编排层。
//! 集中管理 Provider、Model、Timeout、Generation ID、取消、响应解析、耗时指标与错误处理。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use qingjian_core::{PredictionKind, PredictionRequest};

use crate::chat_client::ChatClient;
use crate::config::PredictConfig;
use crate::error::PredictError;
use crate::prompt::Reply;

/// AI 编排指标监控。
#[derive(Debug, Default)]
pub struct AiMetrics {
    pub translation_requests: AtomicU64,
    pub compare_requests: AtomicU64,
    pub last_http_ms: AtomicU64,
    pub last_first_byte_ms: AtomicU64,
    pub last_parse_ms: AtomicU64,
    pub last_validation_ms: AtomicU64,
    pub last_total_ms: AtomicU64,
}

impl AiMetrics {
    pub fn update(&self, http: u64, first_byte: u64, parse: u64, val: u64, total: u64) {
        self.last_http_ms.store(http, Ordering::Relaxed);
        self.last_first_byte_ms.store(first_byte, Ordering::Relaxed);
        self.last_parse_ms.store(parse, Ordering::Relaxed);
        self.last_validation_ms.store(val, Ordering::Relaxed);
        self.last_total_ms.store(total, Ordering::Relaxed);
    }
}

/// 统一服务端 AI 服务编排器。
pub struct AiService {
    config: PredictConfig,
    client: ChatClient,
    metrics: Arc<AiMetrics>,
}

impl AiService {
    pub fn new(config: PredictConfig, api_key: String) -> Self {
        let client = ChatClient::new(&config, api_key);
        Self {
            config,
            client,
            metrics: Arc::new(AiMetrics::default()),
        }
    }

    pub fn metrics(&self) -> Arc<AiMetrics> {
        Arc::clone(&self.metrics)
    }

    pub fn provider(&self) -> &'static str {
        if self.config.base_url.contains("siliconflow") {
            "SiliconFlow"
        } else if self.config.base_url.contains("deepseek") {
            "DeepSeek"
        } else if self.config.base_url.contains("openai") {
            "OpenAI"
        } else {
            "Custom Provider"
        }
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.config.timeout_ms)
    }

    /// 1. 候选翻译：根据当前中文候选生成地道美式英语。
    pub async fn translate_candidate(&self, text: &str) -> Result<Reply, PredictError> {
        self.metrics.translation_requests.fetch_add(1, Ordering::Relaxed);
        let request = PredictionRequest {
            sequence: 0,
            kind: PredictionKind::Translate,
            before: String::new(),
            after: String::new(),
            pinyin: String::new(),
            letters: String::new(),
            syllables: 0,
            candidates: Vec::new(),
            candidate_readings: Vec::new(),
            guess: String::new(),
            current_expression: String::new(),
            max_items: 1,
            want_sentence: true,
            text: text.trim().to_owned(),
            target_language: "en".to_owned(),
            user_attempt: String::new(),
            reference: String::new(),
        };
        self.client.complete(&request).await
    }

    /// 2. 主动回忆对比：对比用户英文表达与参考答案，生成结构化错误分类与反馈。
    pub async fn compare_attempt(
        &self,
        prompt: &str,
        user_attempt: &str,
        reference: &str,
    ) -> Result<Reply, PredictError> {
        self.metrics.compare_requests.fetch_add(1, Ordering::Relaxed);
        let request = PredictionRequest {
            sequence: 0,
            kind: PredictionKind::Compare,
            before: String::new(),
            after: String::new(),
            pinyin: String::new(),
            letters: String::new(),
            syllables: 0,
            candidates: Vec::new(),
            candidate_readings: Vec::new(),
            guess: String::new(),
            current_expression: reference.trim().to_owned(),
            max_items: 1,
            want_sentence: true,
            text: prompt.trim().to_owned(),
            target_language: "en".to_owned(),
            user_attempt: user_attempt.trim().to_owned(),
            reference: reference.trim().to_owned(),
        };
        self.client.complete(&request).await
    }

    /// 3. 生成更多口语变体与语气说明（深度学习伴侣助手）。
    pub async fn generate_expression_variants(
        &self,
        chinese_text: &str,
        primary: &str,
    ) -> Result<Reply, PredictError> {
        let request = PredictionRequest {
            sequence: 0,
            kind: PredictionKind::DeepLearn,
            before: String::new(),
            after: String::new(),
            pinyin: String::new(),
            letters: String::new(),
            syllables: 0,
            candidates: Vec::new(),
            candidate_readings: Vec::new(),
            guess: String::new(),
            current_expression: primary.trim().to_owned(),
            max_items: 5,
            want_sentence: true,
            text: chinese_text.trim().to_owned(),
            target_language: "en".to_owned(),
            user_attempt: String::new(),
            reference: primary.trim().to_owned(),
        };
        self.client.complete(&request).await
    }

    /// 4. 英文输入诊断：直通模式下的不自然表达轻量诊断。
    pub async fn diagnose_english(&self, text: &str) -> Result<Reply, PredictError> {
        self.compare_attempt("English Diagnostic", text, "").await
    }

    /// 5. 生成复习训练项。
    pub async fn generate_recall_item(&self, prompt: &str) -> Result<Reply, PredictError> {
        self.translate_candidate(prompt).await
    }
}
