//! 个人英语学习画像与账本核心数据结构 (Personal English Learning Profile & Vault Models)

use serde::{Deserialize, Serialize};

use super::tracker::{ErrorMajorCategory, ErrorSubcategory};

/// 简单掌握程度分级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MasteryLevel {
    /// 未复习 / 仅初见
    Unseen,
    /// 学习训练中
    Learning,
    /// 较熟练
    Familiar,
    /// 已牢固掌握
    Mastered,
}

impl Default for MasteryLevel {
    fn default() -> Self {
        Self::Unseen
    }
}

impl MasteryLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unseen => "unseen",
            Self::Learning => "learning",
            Self::Familiar => "familiar",
            Self::Mastered => "mastered",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Unseen => "未复习",
            Self::Learning => "学习中",
            Self::Familiar => "较熟练",
            Self::Mastered => "已掌握",
        }
    }
}

/// 用户主动收藏的地道表达 (Saved Expression)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedExpression {
    /// 中文提示 / 语义场景
    pub prompt: String,
    /// 地道英文表达
    pub expression: String,
    /// 语域/语境标签 (如 "Casual / Spoken")
    pub tag: Option<String>,
    /// 学习要点与解析
    pub notes: Option<String>,
    /// 收藏时间戳 (unix seconds)
    pub saved_at: i64,
    /// 当前掌握度
    pub mastery: MasteryLevel,
}

/// 待复习项目 (Review Item)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub prompt: String,
    pub target: String,
    pub attempts_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_reviewed_at: i64,
    pub next_review_at: i64,
    pub mastery: MasteryLevel,
}

/// 用户单次主动回忆/对比尝试记录 (User Attempt)
/// 严格保护隐私：只记录训练本身所需的最小字段，不抓取日常无关输入
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAttempt {
    pub prompt: String,
    pub user_input: String,
    pub reference: String,
    pub is_success: bool,
    pub error_categories: Vec<String>,
    pub timestamp: i64,
}

/// 聚合错误记录 (Error Record)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub major_category: ErrorMajorCategory,
    pub subcategory: Option<ErrorSubcategory>,
    pub user_fragment: String,
    pub corrected_fragment: String,
    pub explanation: String,
    pub occurrence_count: u32,
    pub last_occurred_at: i64,
}

/// 掌握度跟踪记录 (Mastery Record)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasteryRecord {
    pub prompt: String,
    pub target: String,
    pub level: MasteryLevel,
    pub consecutive_successes: u32,
    pub total_reviews: u32,
    pub updated_at: i64,
}

/// 英语学习画像评估维度 (Profile Dimensions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProfileDimension {
    Vocabulary,
    Grammar,
    Collocation,
    Naturalness,
    PhrasalVerbs,
    Pragmatics,
    Register,
}

impl ProfileDimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Grammar => "grammar",
            Self::Collocation => "collocation",
            Self::Naturalness => "naturalness",
            Self::PhrasalVerbs => "phrasal_verbs",
            Self::Pragmatics => "pragmatics",
            Self::Register => "register",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Vocabulary => "Vocabulary",
            Self::Grammar => "Grammar",
            Self::Collocation => "Collocations",
            Self::Naturalness => "Naturalness",
            Self::PhrasalVerbs => "Phrasal Verbs",
            Self::Pragmatics => "Pragmatics",
            Self::Register => "Register",
        }
    }
}

/// 单个能力维度的统计数据
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionStat {
    pub total_tested: u32,
    pub errors: u32,
    pub successes: u32,
    /// 0~100 掌握度/能力分估算
    pub score: u8,
}

/// 个人英语学习画像 (User English Profile)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserEnglishProfile {
    pub vocabulary: DimensionStat,
    pub grammar: DimensionStat,
    pub collocation: DimensionStat,
    pub naturalness: DimensionStat,
    pub phrasal_verbs: DimensionStat,
    pub pragmatics: DimensionStat,
    pub register: DimensionStat,

    pub total_saved: u32,
    pub total_reviews: u32,
    pub total_attempts: u32,
    pub top_weaknesses: Vec<String>,
}

impl UserEnglishProfile {
    /// 格式化为轻量直观的 ASCII 进度柱状图
    pub fn format_ascii_bars(&self) -> String {
        let dim_line = |name: &str, score: u8| -> String {
            let filled = ((score as usize + 5) / 10).min(10);
            let bar: String = "█".repeat(filled);
            let pad = 14usize.saturating_sub(name.len());
            format!("{}{:width$} {:<10} ({}%)", name, "", bar, score, width = pad)
        };

        let mut lines = Vec::new();
        lines.push("Your English Profile:".to_string());
        lines.push(dim_line("Vocabulary", self.vocabulary.score));
        lines.push(dim_line("Grammar", self.grammar.score));
        lines.push(dim_line("Collocations", self.collocation.score));
        lines.push(dim_line("Naturalness", self.naturalness.score));
        lines.push(dim_line("Phrasal Verbs", self.phrasal_verbs.score));
        lines.push(dim_line("Pragmatics", self.pragmatics.score));
        lines.push(dim_line("Register", self.register.score));

        if !self.top_weaknesses.is_empty() {
            lines.push(format!("\n你的主要提升机会：{}", self.top_weaknesses.join(" / ")));
        } else {
            lines.push("\n各项能力表现均衡，继续保持主动输出！".to_string());
        }
        lines.join("\n")
    }

    /// 简短摘要，用于卡片小标题或状态栏
    pub fn format_summary(&self) -> String {
        if !self.top_weaknesses.is_empty() {
            format!("待强化: {}", self.top_weaknesses.join(" / "))
        } else {
            "状态良好".to_string()
        }
    }
}
