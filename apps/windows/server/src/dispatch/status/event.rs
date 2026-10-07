/// 用户在状态条上做的事，UI 线程发回 Router（打开设置不经过 Router，UI 线程自己起进程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusEvent {
    /// 点了「中 / 英」格：翻转模式。
    ToggleMode,

    /// 点了「，。」格：翻转当前模式的全角标点（中英各记一份）。
    TogglePunctuation,

    /// 点了「学 / 练」格：循环切换英语学习模式（Daily -> Learn -> Drill）。
    ToggleLearningMode,

    /// 菜单指定了具体的英语学习模式。
    SetLearningMode(qingjian_platform::LearningMode),

    /// 拖动结束，内容左上角的新位置（物理像素）。
    Moved(i32, i32),
}
