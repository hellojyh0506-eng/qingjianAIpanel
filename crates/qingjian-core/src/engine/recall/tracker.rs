use crate::candidate::Language;

/// 主动回忆测试判定结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallOutcome {
    Success,
    Failure,
    Revealed,
}

/// 主动回忆对比状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ComparisonStatus {
    ExactMatch,
    NaturalBetter,
    /// 用户语法与语义正确（B2层级），但母语者习惯更地道C1或有更佳语域升级选择
    ValidB2Upgrade,
    GrammarIssue,
    Chinglish,
    EmptyInput,
}

/// 语域风格分类 (Register)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Register {
    /// 轻松日常口语 (Casual / Everyday Chatting)
    Casual,
    /// 自然通用美语 (Natural / Spoken English)
    Natural,
    /// 职场商务专业 (Professional / Workplace)
    Professional,
    /// 正式规范书面 (Formal / Academic Writing)
    Formal,
    /// 委婉得体/外交辞令 (Diplomatic / Tactful Disagreement)
    Diplomatic,
    /// 幽默俏皮/调侃 (Playful / Humorous)
    Playful,
    /// 直接明确/强调 (Direct / Assertive)
    Direct,
    /// 柔和缓和/亲切 (Soft / Gentle)
    Soft,
}

impl Default for Register {
    fn default() -> Self {
        Self::Natural
    }
}

impl Register {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Casual => "casual",
            Self::Natural => "natural",
            Self::Professional => "professional",
            Self::Formal => "formal",
            Self::Diplomatic => "diplomatic",
            Self::Playful => "playful",
            Self::Direct => "direct",
            Self::Soft => "soft",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Casual => "轻松口语",
            Self::Natural => "自然地道",
            Self::Professional => "职场专业",
            Self::Formal => "正式书面",
            Self::Diplomatic => "委婉得体",
            Self::Playful => "幽默调侃",
            Self::Direct => "直接明确",
            Self::Soft => "柔和缓和",
        }
    }
}

/// 表达进阶水平 (Sophistication / CEFR)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sophistication {
    B1,
    B2,
    C1,
    C2,
}

impl Default for Sophistication {
    fn default() -> Self {
        Self::B2
    }
}

impl Sophistication {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::C1 => "C1",
            Self::C2 => "C2",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::B1 => "基础通用 (B1)",
            Self::B2 => "中阶正确 (B2)",
            Self::C1 => "高阶地道 (C1)",
            Self::C2 => "母语级精妙 (C2)",
        }
    }
}

/// 语用学与社交场景感觉差异 (Pragmatic Nuance)
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PragmaticNuance {
    pub register: Register,
    pub expression: String,
    pub scenario: String,
    pub feeling: String,
}

/// B2 → C1 表达升级结构化结果
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionUpgradeRecord {
    pub prompt: String,
    pub b2_baseline: String,
    pub natural_expression: String,
    pub c1_expression: String,
    pub register_variants: Vec<PragmaticNuance>,
    pub why_better: String,
    pub nuance_explanation: String,
}

/// 评价四维拆解 (Four-dimensional Evaluation)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluationFourDimensions {
    pub correctness: bool,
    pub naturalness_score: u8,
    pub sophistication: Sophistication,
    pub detected_register: Register,
    pub upgrade_advice: Option<String>,
}

impl Default for EvaluationFourDimensions {
    fn default() -> Self {
        Self {
            correctness: true,
            naturalness_score: 5,
            sophistication: Sophistication::C1,
            detected_register: Register::Natural,
            upgrade_advice: None,
        }
    }
}

/// 顶级错误类别 (Major Category)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// 细分错误类型 (Error Subcategory)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorSubcategory {
    // Grammar
    Article,
    Tense,
    Preposition,
    WordOrder,
    Agreement,
    Countability,

    // Vocabulary
    WrongWord,
    LiteralTranslation,
    VocabularyPrecision,

    // Collocation
    UnnaturalCollocation,
    IncorrectVerbNounPairing,
    AdjectiveNounPairing,
    FixedExpression,

    // Naturalness
    AwkwardPhrasing,
    Redundancy,
    UnnaturalSentenceRhythm,
    Chinglish,

    // Phrasal / Idiomatic
    MissingPhrasalVerb,
    WrongIdiom,
    UnnaturalLiteralEquivalent,

    // Pragmatics
    TooDirect,
    TooFormal,
    TooCasual,
    WrongSocialTone,

    // Other/Extensible
    Other(String),
}

impl ErrorSubcategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Article => "冠词使用",
            Self::Tense => "时态语态",
            Self::Preposition => "介词搭配",
            Self::WordOrder => "词序语序",
            Self::Agreement => "主谓一致",
            Self::Countability => "可数单复数",

            Self::WrongWord => "用词不当",
            Self::LiteralTranslation => "字面直译",
            Self::VocabularyPrecision => "用词精度",

            Self::UnnaturalCollocation => "搭配生硬",
            Self::IncorrectVerbNounPairing => "动宾搭配不当",
            Self::AdjectiveNounPairing => "修饰搭配不当",
            Self::FixedExpression => "固定表达差异",

            Self::AwkwardPhrasing => "表述别扭",
            Self::Redundancy => "表达冗余",
            Self::UnnaturalSentenceRhythm => "节奏不自然",
            Self::Chinglish => "中式思维直译",

            Self::MissingPhrasalVerb => "缺少地道短语动词",
            Self::WrongIdiom => "习语误用",
            Self::UnnaturalLiteralEquivalent => "生硬对等词",

            Self::TooDirect => "过于生硬直接",
            Self::TooFormal => "语域偏书面正式",
            Self::TooCasual => "语域过于口语随意",
            Self::WrongSocialTone => "语气不合社交语境",

            Self::Other(_) => "其他建议",
        }
    }
}

/// 结构化单条反馈建议
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FeedbackItem {
    pub major_category: ErrorMajorCategory,
    pub subcategory: ErrorSubcategory,
    pub label: String,
    pub explanation: String,
    pub suggestion: Option<String>,
}

/// 主动回忆对比结果
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// 用户的一次主动回忆练习记录 (Attempt)
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecallAttempt {
    pub prompt: String,
    pub user_attempt: String,
    pub reference_expression: String,
    pub timestamp: i64,
    pub comparison: ComparisonResult,
    pub error_categories: Vec<String>,
}

/// 主动回忆条目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallItem {
    pub prompt: String,
    pub target: String,
    pub language: Language,
    pub seen: u32,
    pub success: u32,
    pub failure: u32,
    pub revealed: u32,
}

/// 主动回忆追踪器抽象。实现放 qingjian-learning，Core 只负责记录与调用。
pub trait RecallTracker: Send {
    /// 记一次主动回忆练习记录 (Attempt)
    fn record_attempt(&mut self, _language: Language, _attempt: &RecallAttempt) {}
    /// 记一次被动曝光 (用户输入中文并上屏带翻译的候选词时)
    fn record_exposure(&mut self, language: Language, prompt: &str, target: &str);

    /// 记一次主动回忆测试结果 (成功 / 失败 / 显式查看答案)
    fn record_recall(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
    );

    /// 记一次主动回忆测试结果，附带用户实际输入的英文（成功时用于记录自然同义替代表达）
    fn record_recall_with_input(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
        _input: &str,
    ) {
        self.record_recall(language, prompt, target, outcome);
    }

    /// 获取该中文词与目标翻译在用户个性化习惯下的自然口语首选替代表达（若已达到 Learned 状态）
    fn preferred_alternative(
        &self,
        _language: Language,
        _prompt: &str,
        _target: &str,
    ) -> Option<String> {
        None
    }

    /// 获取下一条待复习条目
    fn next_item(&self, language: Language) -> Option<RecallItem>;

    /// 待复习总数
    fn pending_count(&self, _language: Language) -> usize {
        0
    }

    /// 落盘
    fn flush(&mut self) {}
}

/// 缺省实现：不记录回忆条目
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRecallTracker;

impl RecallTracker for NoRecallTracker {
    fn record_exposure(&mut self, _language: Language, _prompt: &str, _target: &str) {}

    fn record_recall(
        &mut self,
        _language: Language,
        _prompt: &str,
        _target: &str,
        _outcome: RecallOutcome,
    ) {
    }

    fn next_item(&self, _language: Language) -> Option<RecallItem> {
        None
    }
}
