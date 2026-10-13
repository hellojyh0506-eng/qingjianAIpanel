//! 对话里的一条消息：原文、思考过程、解析好的 Markdown，以及按宽度缓存的排版。

use super::bubble::BubbleLayout;
use super::markdown::{Block, Inline, parse_blocks};
use super::role::Role;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,

    text: String,

    reasoning: String,

    /// 联网搜索的来源，一条一行（标题或网址）。
    pub sources: Vec<String>,

    /// 思考过程是否展开。
    pub reasoning_open: bool,

    blocks: Vec<Block>,

    /// 上一次排版的结果；文字变了或宽度变了就失效。
    pub(super) layout: Option<BubbleLayout>,
}

impl ChatMessage {
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        let mut message = Self {
            role,
            text: String::new(),
            reasoning: String::new(),
            sources: Vec::new(),
            reasoning_open: false,
            blocks: Vec::new(),
            layout: None,
        };
        message.push_text(&text.into());
        message
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn reasoning(&self) -> &str {
        &self.reasoning
    }

    pub(super) fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// 追加回答文字（流式）并重新解析本条。
    pub fn push_text(&mut self, chunk: &str) {
        self.text.push_str(chunk);
        self.blocks = match self.role {
            Role::Assistant => parse_blocks(&self.text),
            // 用户输入与错误提示按原文显示，不当 Markdown 解析
            Role::User | Role::Error if self.text.is_empty() => Vec::new(),
            Role::User | Role::Error => {
                vec![Block::Paragraph(vec![Inline::Plain(self.text.clone())])]
            }
        };
        self.layout = None;
    }

    /// 追加思考过程（流式）。
    pub fn push_reasoning(&mut self, chunk: &str) {
        self.reasoning.push_str(chunk);
        self.layout = None;
    }

    pub fn set_reasoning_open(&mut self, open: bool) {
        self.reasoning_open = open;
        self.layout = None;
    }

    pub fn set_sources(&mut self, sources: Vec<String>) {
        self.sources = sources;
        self.layout = None;
    }
}
