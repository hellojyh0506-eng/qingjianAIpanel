//! 「语感解析」标签的内容：意思、语气、使用场景各一条，一条自然例句，若干搭配与语法细节。

use super::detail::Detail;
use super::example::Example;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NuanceContent {
    pub meaning: String,

    pub tone: String,

    pub usage: String,

    pub example: Option<Example>,

    pub details: Vec<Detail>,
}

impl NuanceContent {
    pub fn is_empty(&self) -> bool {
        self.meaning.is_empty()
            && self.tone.is_empty()
            && self.usage.is_empty()
            && self.example.is_none()
            && self.details.is_empty()
    }
}
