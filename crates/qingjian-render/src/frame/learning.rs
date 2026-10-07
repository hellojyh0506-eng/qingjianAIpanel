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

/// Active Recall 交互阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillStage {
    Input,
    Comparing,
    Compared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonStatus {
    ExactMatch,
    NaturalBetter,
    GrammarIssue,
    Chinglish,
    EmptyInput,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComparisonResult {
    pub status: Option<ComparisonStatus>,
    pub key_advice: String,
    pub user_formatted: String,
    pub natural_expression: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecallAttempt {
    pub prompt: String,
    pub user_attempt: String,
    pub reference_expression: String,
    pub timestamp: i64,
    pub comparison: ComparisonResult,
    pub error_categories: Vec<String>,
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
    pub drill_stage: Option<DrillStage>,
    pub drill_input: String,
    pub drill_attempt: Option<RecallAttempt>,
    pub drill_notice: Option<String>,
}
