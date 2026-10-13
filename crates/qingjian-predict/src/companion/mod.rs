//! 学习助手（语感解析 / 更自然地说 / 真实对话）的生成：提示词、流式 JSONL 解析与校验。
//!
//! 模型按约定逐行输出 JSON，每行一个 [`Block`]；[`BlockParser`] 边收边切、校验、限数，UI 每拿到一个区块就填一张卡片。
//! 这里只管文本协议，不碰网络与界面：请求由调用方用 [`messages`] 的结果加 [`Tab::max_tokens`] 交给 `StreamChatClient`。

mod block;
mod cache;
mod jsonl;
mod prompt;
mod request;
mod runner;
mod scene;
mod tab;
mod validate;

pub use block::{Block, Speaker};
pub use cache::{BlockCache, CAPACITY as CACHE_CAPACITY};
pub use jsonl::{BlockParser, ParseStats};
pub use prompt::messages;
pub use request::CompanionRequest;
pub use runner::{Outcome, consume};
pub use scene::Scene;
pub use tab::Tab;

/// 学习助手请求的采样温度：要稳定的结构化输出，比问答低。
pub const TEMPERATURE: f32 = 0.5;
