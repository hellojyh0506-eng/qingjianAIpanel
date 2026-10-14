//! 学习助手面板上能点的东西。窗口过程查 [`HitMap`](crate::panel::HitMap) 得到它，再交给业务层处理。

#[derive(Debug, Clone, PartialEq)]
pub enum CompanionHit {
    /// 标题栏空白处：按住拖动窗口。
    Drag,

    Close,

    /// 钉住 / 取消钉住（置顶）。
    Pin,

    /// 收起成只剩标题栏 / 展开。
    Minimize,

    /// 朗读这段英文。
    Speak(String),

    /// 复制这段英文。
    Copy(String),

    /// 切换标签页（0 语感解析、1 更自然地说、2 真实对话）。
    Tab(usize),

    /// 切换场景（对应 predict 的 `Scene::index`）。
    Scene(usize),

    /// 展开 / 收起「更多场景」菜单。
    MoreScenes,

    /// 点在菜单之外：关闭菜单。
    DismissMenu,

    /// 展开 / 收起「搭配与语法细节」。
    ToggleDetails,

    /// 重新生成当前标签。
    Retry,

    /// 问 AI。
    AskAi,
}
