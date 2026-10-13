use super::lane::PredictionLane;

/// 一次联想请求要的是什么。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PredictionKind {
    /// 组句中：这段拼音对应的词，可能还要整句补全（旧兼容类型）。
    #[default]
    Compose,

    /// 只对当前输入的本地候选做重排；不得生成候选之外的新词或后文续写。
    /// 只建议当前长拼音的修正拼写，必须经过 Core 本地校验。
    Intent,

    /// 只对当前输入的本地候选做重排；不得生成候选之外的新词或后文续写。
    Rerank,

    /// 当前这一串到底想打什么：同时判断拼音有没有敲错、结合光标前后文选同音字。
    /// 合并了 Rerank 与 Intent，回复必须经过 Core 本地校验后才会生效。
    Resolve,

    /// 问字模式（`?` 开头）：用拼音问「三个木是什么字」之类的问题，要答案（字或短答案）与读音。
    Question,

    /// 翻译：把应用里选中的一段文字译成学习语言（壳里快捷键触发），译文放在结果的 `sentence` 里。
    Translate,

    /// 主动回忆对比：对比用户的英文表达尝试与地道参考口语，给出评价与建议。
    Compare,

    /// 深度英语学习与伴侣助手：生成音标、地道释义、多语域变体、高频搭配与例句。
    DeepLearn,
}

impl PredictionKind {
    /// 这类请求走哪条通道。问字和打字一样每键都会换，所以归打字通道。
    pub fn lane(self) -> PredictionLane {
        match self {
            Self::Compose | Self::Intent | Self::Rerank | Self::Resolve | Self::Question => {
                PredictionLane::Typing
            }
            Self::Translate => PredictionLane::Learning,
            Self::Compare | Self::DeepLearn => PredictionLane::Deep,
        }
    }

    /// 深度类结果不随输入变化而过期：用户继续打字时它仍然有效。
    pub fn outlives_typing(self) -> bool {
        matches!(self, Self::Compare | Self::DeepLearn)
    }
}
