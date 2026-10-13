//! 单条样本一轮的测量结果。

use std::time::Duration;

use super::sample::Sample;

pub struct Outcome {
    pub category: String,

    /// 期望答案在本地候选里的位置（0 起）；不在为 `None`。
    pub local_rank: Option<usize>,

    /// 套上 AI 建议之后的位置。
    pub final_rank: Option<usize>,

    /// 是否真的发了请求（太短、被过滤的不发）。
    pub sent: bool,

    /// 发出到拿到结果（或超时）的耗时。
    pub latency: Option<Duration>,

    /// AI 给出了通过本地校验的建议。
    pub accepted: bool,

    /// 套上 AI 结果之后，第 1 位带「纠」标记。
    pub marked_first: bool,
}

impl Outcome {
    /// 引擎查不出候选：当作没命中。
    pub fn failed(sample: &Sample) -> Self {
        Self {
            category: sample.category.clone(),
            local_rank: None,
            final_rank: None,
            sent: false,
            latency: None,
            accepted: false,
            marked_first: false,
        }
    }
}
