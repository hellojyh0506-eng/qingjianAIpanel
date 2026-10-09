use serde::{Deserialize, Serialize};

/// 英语学习模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LearningMode {
    /// 日常模式：默认模式，不主动打扰输入，不自动记录日常输入，性能最高。
    #[default]
    Daily,

    /// 学习模式：在正常候选窗口下方提供轻量英语辅助与自然口语提示。
    Learn,

    /// 训练模式：主动回忆与对比练习，可输入英文并进行地道度评测。
    Drill,
}

impl LearningMode {
    /// 全部取值，设置界面按这个顺序列出。
    pub const ALL: [Self; 2] = [Self::Daily, Self::Learn];

    /// 配置文件里的写法。
    pub fn key(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Learn => "learn",
            Self::Drill => "learn",
        }
    }

    /// 界面上的名字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Daily => "日常 (Daily)",
            Self::Learn => "学习 (Learn)",
            Self::Drill => "学习 (Learn)",
        }
    }

    /// 模式详细说明文案。
    pub fn description(self) -> &'static str {
        match self {
            Self::Daily => "日常输入，不主动打扰",
            Self::Learn => "输入时提供学习辅助与自然口语",
            Self::Drill => "输入时提供学习辅助与自然口语",
        }
    }

    /// 循环切换下一个模式：Daily <-> Learn。
    pub fn next(self) -> Self {
        match self {
            Self::Daily => Self::Learn,
            Self::Learn => Self::Daily,
            Self::Drill => Self::Daily,
        }
    }
}
