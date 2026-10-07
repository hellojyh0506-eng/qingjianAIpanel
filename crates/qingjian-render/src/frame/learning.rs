//! 候选窗口绑定的轻量英语学习卡片数据模型。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningStatus {
    Loading,
    Ready,
    Failed,
}

impl Default for LearningStatus {
    fn default() -> Self {
        Self::Ready
    }
}

/// 单个自然美式口语表达及学习细览。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearningExpression {
    pub text: String,
    pub tag: String,
    pub meaning: String,
    pub tone: String,
    pub scenario: String,
    pub commonality: String,
    pub why_natural: String,
}

/// 轻量英语学习卡片（候选窗第三层及渐进展开层）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearningCard {
    pub target_text: String,
    pub status: LearningStatus,
    pub primary: Option<LearningExpression>,
    pub alternatives: Vec<LearningExpression>,
    pub expanded: bool,
    pub selected_index: Option<usize>,
}
