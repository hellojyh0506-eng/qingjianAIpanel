//! 一个标签页的生成状态。

#[derive(Debug, Clone, PartialEq, Default)]
pub enum TabStatus {
    /// 还没请求过：显示「点击上方选项卡查看内容」。
    #[default]
    Idle,

    /// 已发请求，一个区块都没到：转圈 + 骨架条。
    Loading,

    /// 区块陆续到达：已到的卡片先显示，还差的补骨架或步骤提示。
    Streaming,

    /// 全部到齐。
    Done,

    /// 失败，带给用户看的原因。
    Failed(String),
}

impl TabStatus {
    /// 是否有内容可显示（流式中与完成）。
    pub fn has_content(&self) -> bool {
        matches!(self, TabStatus::Streaming | TabStatus::Done)
    }
}
