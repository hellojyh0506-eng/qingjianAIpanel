//! 「真实对话」标签的内容：场景标题、对话句子（雅思场景第一句是题目）、表达亮点。

use super::turn::Turn;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DialogueContent {
    /// 如「朋友聊天（鼓励安慰）」；卡片上显示成「场景 N：xxx」。
    pub title: String,

    pub turns: Vec<Turn>,

    /// 表达亮点，一条一行。
    pub points: Vec<String>,
}

impl DialogueContent {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.turns.is_empty() && self.points.is_empty()
    }
}
