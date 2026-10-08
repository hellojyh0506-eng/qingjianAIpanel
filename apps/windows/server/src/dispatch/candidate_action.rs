//! 候选卡片上的鼠标交互动作，UI 线程发回 Router 处理。

/// 候选卡片上的交互动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAction {
    /// 切换展开/收起「更多译法」
    ToggleExpand,

    /// 切换高亮某个更多译法选项
    ToggleAlternative(usize),

    /// 点击「Try English」开启主动回忆练习
    StartDrill,

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
}
