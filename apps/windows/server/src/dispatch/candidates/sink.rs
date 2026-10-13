//! 候选窗口的输出端。

use qingjian_platform::CandidateRenderer;
use qingjian_platform::protocol::{Frame, ScreenRect};

/// 候选窗口 / 状态条的画法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSettings {
    /// 由谁画（`[general] renderer`）。
    pub renderer: CandidateRenderer,

    /// 字族名（`[general] font`），空为系统字体。
    pub font: String,
}

/// Router 只产出帧，画交给它；Windows 上由 UI 线程实现。
pub trait CandidateSink: Send {
    /// 把候选窗口摆到 `rect`（组句范围的屏幕矩形）下方并按 `frame` 重绘。
    fn show(&self, frame: Frame, rect: ScreenRect);

    fn hide(&self);

    /// 换画法：装上时与配置热加载后调，只在设置变了时调。
    fn configure(&self, settings: RenderSettings);

    /// 打开学习助手面板：中文原意、英文表达、是否已收藏，`rect` 是组句矩形。
    fn show_companion(&self, zh: String, en: String, saved: bool, rect: ScreenRect) {
        let _ = (zh, en, saved, rect);
    }

    /// 学习助手某个标签（对话标签带场景序号）收到一个区块。
    fn companion_block(&self, tab: usize, scene: usize, block: qingjian_predict::companion::Block) {
        let _ = (tab, scene, block);
    }

    /// 学习助手某个标签生成完成。
    fn finish_companion(&self, tab: usize, scene: usize) {
        let _ = (tab, scene);
    }

    /// 学习助手某个标签生成失败。
    fn fail_companion(&self, tab: usize, scene: usize, message: String) {
        let _ = (tab, scene, message);
    }

    /// 学习助手面板底部短暂提示；`saved` 为真同时把收藏按钮置为已收藏。
    fn companion_notice(&self, text: String, saved: bool) {
        let _ = (text, saved);
    }

    /// 收起 English Assistant
    fn hide_companion(&self) {}

    /// 启动 Drill 独立练习窗口
    /// `cloze` 是挖空练习的英文句（被挖掉的短语换成 `____`），没有就是整句翻译练习。
    fn start_drill(&self, target_chinese: String, reference: String, cloze: Option<String>, rect: ScreenRect) {
        let _ = (target_chinese, reference, cloze, rect);
    }

    /// 下发 Drill 对比批改结果
    fn show_drill_result(&self, result: crate::ui::drill::DrillResultData) {
        let _ = result;
    }

    /// 收起 Drill 窗口
    fn hide_drill(&self) {}

    /// 打开 / 聚焦 AI Assistant 独立对话窗口，并下发整面状态（会话列表、当前会话）
    fn show_ai_assistant(
        &self,
        anchor: Option<windows::Win32::Foundation::RECT>,
        state: crate::dispatch::AiPanelState,
    ) {
        let _ = (anchor, state);
    }

    /// 换了会话（新建 / 切换 / 清空）：用新的整面状态重建窗口内容，不动窗口位置
    fn send_ai_session(&self, state: crate::dispatch::AiPanelState) {
        let _ = state;
    }

    /// 只刷新侧栏会话列表（消息数变了）
    fn send_ai_sessions(&self, sessions: Vec<qingjian_learning::chat_history::SessionSummary>) {
        let _ = sessions;
    }

    /// 发送思考过程增量到 AI Assistant 窗口
    fn send_ai_reasoning(&self, chunk: String) {
        let _ = chunk;
    }

    /// 收起 AI Assistant 对话窗口
    fn hide_ai_assistant(&self) {}

    /// 发送流式文本增量到 AI Assistant 窗口
    fn send_ai_chunk(&self, chunk: String) {
        let _ = chunk;
    }

    /// 发送联网搜索结果到 AI Assistant 窗口
    fn send_ai_web_sources(&self, sources: Vec<qingjian_predict::SearchResult>) {
        let _ = sources;
    }

    /// 发送流式生成结束通知到 AI Assistant 窗口
    fn send_ai_finish(&self, ttft_ms: u128, total_ms: u128) {
        let _ = (ttft_ms, total_ms);
    }

    /// 发送流式生成异常通知到 AI Assistant 窗口
    fn send_ai_error(&self, error: String) {
        let _ = error;
    }

    /// 克隆一个 Sink 发送端
    fn clone_sink(&self) -> Box<dyn CandidateSink>;
}

/// 不画候选窗口的空实现。
pub struct NoopSink;

impl CandidateSink for NoopSink {
    fn show(&self, _frame: Frame, _rect: ScreenRect) {}

    fn hide(&self) {}

    fn configure(&self, _settings: RenderSettings) {}

    fn clone_sink(&self) -> Box<dyn CandidateSink> {
        Box::new(NoopSink)
    }
}
