//! 候选窗口绑定的轻量英语学习卡片数据模型。

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LearningMode {
    #[default]
    Daily,
    Learn,
    Drill,
}

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
    Profile,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonStatus {
    ExactMatch,
    NaturalBetter,
    ValidB2Upgrade,
    GrammarIssue,
    Chinglish,
    EmptyInput,
}

/// 语域风格划分（Register）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    Casual,
    Natural,
    Professional,
    Formal,
    Diplomatic,
    Playful,
    Direct,
    Soft,
}

impl Register {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Casual => "随意口语",
            Self::Natural => "自然日常",
            Self::Professional => "职场专业",
            Self::Formal => "正式严谨",
            Self::Diplomatic => "得体委婉",
            Self::Playful => "幽默调侃",
            Self::Direct => "干脆直接",
            Self::Soft => "温和客气",
        }
    }
}

/// 语言成熟度等级（CEFR / Sophistication）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sophistication {
    B1,
    B2,
    C1,
    C2,
}

impl Sophistication {
    pub fn label(&self) -> &'static str {
        match self {
            Self::B1 => "基础 · B1",
            Self::B2 => "进阶 · B2",
            Self::C1 => "地道高阶 · C1",
            Self::C2 => "母语精通 · C2",
        }
    }
}

/// 语用细微差别（Pragmatic Nuance）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PragmaticNuance {
    pub register: Register,
    pub expression: String,
    pub scenario: String,
    pub feeling: String,
}

/// B2 → C1 表达升级知识库记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionUpgradeRecord {
    pub prompt: String,
    pub b2_baseline: String,
    pub natural_expression: String,
    pub c1_expression: String,
    pub register_variants: Vec<PragmaticNuance>,
    pub why_better: String,
    pub nuance_explanation: String,
}

/// 四维综合评估指标（Four-Dimension Assessment）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationFourDimensions {
    pub correctness: bool,
    pub naturalness_score: u8,
    pub sophistication: Sophistication,
    pub detected_register: Register,
    pub upgrade_advice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorMajorCategory {
    Grammar,
    Vocabulary,
    Collocation,
    Naturalness,
    PhrasalIdiomatic,
    Pragmatics,
}

impl ErrorMajorCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Grammar => "语法",
            Self::Vocabulary => "用词",
            Self::Collocation => "搭配",
            Self::Naturalness => "自然度",
            Self::PhrasalIdiomatic => "短语习语",
            Self::Pragmatics => "语用语域",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSubcategory {
    Article,
    Tense,
    Preposition,
    WordOrder,
    Agreement,
    Countability,
    WrongWord,
    LiteralTranslation,
    VocabularyPrecision,
    UnnaturalCollocation,
    IncorrectVerbNounPairing,
    AdjectiveNounPairing,
    FixedExpression,
    AwkwardPhrasing,
    Redundancy,
    UnnaturalSentenceRhythm,
    Chinglish,
    MissingPhrasalVerb,
    WrongIdiom,
    UnnaturalLiteralEquivalent,
    TooDirect,
    TooFormal,
    TooCasual,
    WrongSocialTone,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackItem {
    pub major_category: ErrorMajorCategory,
    pub subcategory: ErrorSubcategory,
    pub label: String,
    pub explanation: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComparisonResult {
    pub status: Option<ComparisonStatus>,
    pub key_advice: String,
    pub user_formatted: String,
    pub natural_expression: String,
    pub primary_category: Option<ErrorMajorCategory>,
    pub feedback_items: Vec<FeedbackItem>,
    pub upgrade: Option<ExpressionUpgradeRecord>,
    pub dimensions: Option<EvaluationFourDimensions>,
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
    pub today_due_count: usize,
    pub review_current_index: usize,
    pub review_total_count: usize,
    pub upgrade: Option<ExpressionUpgradeRecord>,
}
