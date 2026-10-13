//! 候选卡片上的鼠标交互动作，UI 线程发回 Router 处理。

/// 候选卡片与学习系统上的交互动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateAction {
    /// 打开独立 English Assistant（学习助手伴侣窗）
    OpenAssistant,

    /// 关闭独立 English Assistant
    CloseAssistant,

    /// 学习助手面板切到还没生成过的标签（0 语感解析、1 更自然地说、2 真实对话）
    CompanionSelectTab(usize),

    /// 学习助手面板切到还没生成过的对话场景
    CompanionSelectScene(usize),

    /// 学习助手面板重新生成某个标签（对话标签带场景序号）
    CompanionRetry { tab: usize, scene: usize },

    /// 学习助手面板：收藏当前表达并加入间隔复习
    CompanionSave,

    /// 学习助手面板：练一句（挖空补全）
    CompanionPractice,

    /// 切换展开/收起「更多译法」（兼容旧接口）
    ToggleExpand,

    /// 切换高亮某个更多译法选项
    ToggleAlternative(usize),

    /// 点击进入独立 Drill Session 练习
    StartDrill,

    /// 点击启动独立 Drill Session 练习（显式语义别名）
    StartDrillSession,

    /// 练习输入态：提交对比请求 (target_chinese, user_input, reference)
    DrillCompare(String, String, String),

    /// 练习输入态：点击「显示答案」
    Reveal,

    /// 练习输入态：点击「对比 (Enter)」
    Compare,

    /// 对比完成态：点击「🔄 再试一次 (R)」
    Retry,

    /// 对比完成态：点击「⭐ 保存到表达本 (S)」
    Save,

    /// 查看用户画像
    ShowProfile,

    /// 开始 FSRS 间隔复习
    StartReview,

    /// FSRS 评级：Again
    RateAgain,

    /// FSRS 评级：Good
    RateGood,

    /// FSRS 评级：Easy
    RateEasy,

    /// 清空生词本/错题库
    ClearVault,

    /// 退出 Drill / Review 面板
    Finish,

    /// 打开独立 AI Assistant 对话窗口
    OpenAiAssistant,

    /// 关闭独立 AI Assistant 对话窗口
    CloseAiAssistant,

    /// 发起 AI 流式对话问答
    AiChatSend {
        prompt: String,

        /// 深度开关：打开才允许模型思考。
        deep: bool,
        web_search: bool,
        session_id: String,
    },

    /// 停止当前正在生成的流式请求
    AiChatStop {
        session_id: String,
    },

    /// 开始全新对话会话
    AiChatNewSession,

    /// 切到侧栏里的另一个会话
    AiChatSwitch { session_id: String },

    /// 清空当前会话
    AiChatClear,
}
