//! 一轮问答进行到哪一步。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,

    /// 已发出，还没收到任何内容：显示「正在思考…」。
    Waiting,

    /// 正在流式接收：发送键变停止键。
    Streaming,
}

impl Phase {
    /// 这一轮还没结束。
    pub fn is_busy(self) -> bool {
        self != Phase::Idle
    }
}
