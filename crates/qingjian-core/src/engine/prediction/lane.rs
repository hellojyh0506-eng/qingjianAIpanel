//! 云端请求的通道：互不打断，各自「最新请求优先」。

/// 请求按用途分三条通道。同一通道里新请求让旧请求过期；不同通道互不影响。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredictionLane {
    /// 打字链路：重排、意图纠错、组句联想、问字。每键都可能换，结果只对当前输入有效。
    Typing,

    /// 学习链路：候选或选区的翻译。
    Learning,

    /// 深度链路：深度学习与对比评测，耗时长，不被打字打断。
    Deep,
}

impl PredictionLane {
    /// 通道数量，用来开按通道索引的数组。
    pub const COUNT: usize = 3;

    /// 在按通道索引的数组里的下标。
    pub fn index(self) -> usize {
        match self {
            Self::Typing => 0,
            Self::Learning => 1,
            Self::Deep => 2,
        }
    }
}
