use serde::{Deserialize, Serialize};

/// 轻量英语学习卡片状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearningState {
    /// 正在后台异步翻译中，候选窗口展示柔和的 loading 状态，绝不阻塞输入
    Loading,
    /// 翻译就绪，展示地道美式口语推荐
    Ready,
    /// 翻译失败或异常，静默降级，不破坏候选窗口
    Failed,
}

impl Default for LearningState {
    fn default() -> Self {
        Self::Ready
    }
}

/// 单个自然美式口语表达及学习细览。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningExpression {
    /// 英文表达文本（如 "I've been swamped lately."）
    pub text: String,
    /// 语域标签：Casual / Natural / Professional / Formal / Idiomatic 等
    pub tag: String,
    /// 精准中文含义（如 "最近忙得晕头转向/忙不过来"）
    pub meaning: String,
    /// 语气色彩（如 "轻松随和 · 美式地道口语"）
    pub tone: String,
    /// 适用场景（如 "朋友日常聊天 / 微信工作沟通"）
    pub scenario: String,
    /// 常见程度（如 "极高频，Native Speaker 必备"）
    pub commonality: String,
    /// 为什么更自然/地道（如 "用 swamped 生动表达被事务淹没，避免平淡的 I'm busy"）
    pub why_natural: String,
}

/// Active Recall 交互阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrillStage {
    /// 英文输入/训练阶段
    Input,
    /// 正在对比阶段
    Comparing,
    /// 对比完成展示阶段
    Compared,
    /// 个人学习画像与账本查看阶段
    Profile,
}

pub use qingjian_core::{
    ComparisonResult, ComparisonStatus, DimensionStat, ErrorMajorCategory, ErrorRecord,
    ErrorSubcategory, FeedbackItem, MasteryLevel, MasteryRecord, ProfileDimension, RecallAttempt,
    ReviewItem, SavedExpression, UserAttempt, UserEnglishProfile,
};

/// 候选窗口第三层：轻量英语学习卡片帧数据。
/// 严格绑定当前高亮候选词，在 Learn 模式或主动展开时呈现。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningCardFrame {
    /// 严格绑定的目标候选词（例如 "他充电了吗" / "解耦" / "我日他哥"）
    pub target_text: String,
    /// 当前翻译状态
    pub state: LearningState,
    /// 首选最佳自然口语表达
    pub primary: Option<LearningExpression>,
    /// 更多译法候选表达（2~5 个不同语域表达）
    pub alternatives: Vec<LearningExpression>,
    /// 是否展开「更多译法」列表
    pub expanded: bool,
    /// 当前展开查看详情的表达下标（None 表示未展开详情）
    pub selected_index: Option<usize>,
    /// Active Recall 交互阶段（None 为普通学习/答案隐藏状态）
    pub drill_stage: Option<DrillStage>,
    /// 用户在 Drill Input 阶段输入的英文内容
    pub drill_input: String,
    /// 用户完成对比后的 Attempt 结果
    pub drill_attempt: Option<RecallAttempt>,
    /// 提示信息（例如 "⭐ 已保存到生词本"）
    pub drill_notice: Option<String>,
}
