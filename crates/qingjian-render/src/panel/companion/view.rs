//! 学习助手面板的视图模型：窗口持有它，每次变化改字段后重画一帧。

use super::alternative::Alternative;
use super::dialogue::DialogueContent;
use super::nuance::NuanceContent;
use super::status::TabStatus;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompanionView {
    /// 用户的中文原意。
    pub zh: String,

    /// 当前要学的英文表达。
    pub en: String,

    /// 特点标签（最多 3 个）。
    pub tags: Vec<String>,

    /// 难度等级，如 `B1-B2`。
    pub level: String,

    /// 当前标签页：0 语感解析、1 更自然地说、2 真实对话。
    pub tab: usize,

    pub nuance: NuanceContent,

    pub nuance_status: TabStatus,

    pub alternatives: Vec<Alternative>,

    pub alternatives_status: TabStatus,

    /// 当前场景的对话。
    pub dialogue: DialogueContent,

    pub dialogue_status: TabStatus,

    /// 当前场景序号（对应 predict 的 `Scene::index`，0..6）。
    pub scene: usize,

    /// 「更多场景」菜单是否展开。
    pub scene_menu_open: bool,

    /// 「搭配与语法细节」是否展开。
    pub details_open: bool,

    pub pinned: bool,

    /// 收起成只剩标题栏。
    pub minimized: bool,

    /// 底部短暂提示（「已复制」）。
    pub notice: Option<String>,

    /// 内容区向上滚过的距离（点）。
    pub scroll: f32,

    /// 转圈图标的角度（度），由窗口的定时器推进。
    pub spin: f32,
}

impl CompanionView {
    pub fn new(zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            zh: zh.into(),
            en: en.into(),
            ..Self::default()
        }
    }

    /// 当前标签页的状态。
    pub fn status(&self) -> &TabStatus {
        match self.tab {
            0 => &self.nuance_status,
            1 => &self.alternatives_status,
            _ => &self.dialogue_status,
        }
    }
}
