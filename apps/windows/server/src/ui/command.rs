//! 交给 UI 线程执行的命令。

use qingjian_learning::chat_history::SessionSummary;
use qingjian_platform::protocol::{Frame, ScreenRect};
use qingjian_predict::companion::Block;

use super::drill::DrillResultData;
use crate::dispatch::{AiPanelState, RenderSettings, StatusView};

/// 交给 UI 线程执行的命令。`Frame` 较大，装箱免得枚举过胖。
pub(super) enum UiCommand {
    /// 把候选窗口摆到组句矩形下方并按帧重绘。
    Show(Box<(Frame, ScreenRect)>),

    /// 收起候选窗口。
    Hide,

    /// 显示 / 更新悬浮状态条。
    StatusShow(Box<StatusView>),

    /// 收起悬浮状态条。
    StatusHide,

    /// 换画法（渲染器 / 字体）。
    Configure(RenderSettings),

    /// 打开学习助手面板：(中文原意, 英文表达, 是否已收藏, 组句矩形)。
    CompanionShow(Box<(String, String, bool, ScreenRect)>),

    /// 学习助手面板某个标签（对话标签还带场景序号）收到一个区块。
    CompanionBlock { tab: usize, scene: usize, block: Block },

    /// 学习助手面板某个标签生成完成。
    CompanionFinish { tab: usize, scene: usize },

    /// 学习助手面板某个标签生成失败。
    CompanionError { tab: usize, scene: usize, message: String },

    /// 学习助手面板底部短暂提示；`saved` 为真同时把收藏按钮置为已收藏。
    CompanionNotice { text: String, saved: bool },

    /// 收起 English Assistant。
    CompanionHide,

    /// 启动 Drill（主动练习独立窗口）。
    DrillStart(Box<(String, String, Option<String>, ScreenRect)>),

    /// 下发 Drill 对比批改结果。
    DrillResult(Box<DrillResultData>),

    /// 收起 Drill 窗口。
    DrillHide,

    /// 显示 / 聚焦 AI 问答窗口：组句矩形 + 整面状态（侧栏、当前会话）。
    AiAssistantShow(Box<(Option<windows::Win32::Foundation::RECT>, AiPanelState)>),

    /// 整面状态更新（新对话 / 切换 / 清空会话之后）。
    AiAssistantSession(Box<AiPanelState>),

    /// 只刷新侧栏会话列表。
    AiAssistantSessions(Vec<SessionSummary>),

    /// 思考过程增量到达
    AiAssistantReasoning(String),

    /// 收起 AI Assistant 对话窗口
    AiAssistantHide,

    /// 流式增量内容到达
    AiAssistantChunk(String),

    /// 真实网络搜索结果到达
    AiAssistantWebSources(Vec<qingjian_predict::SearchResult>),

    /// 流式生成结束
    AiAssistantFinish { ttft_ms: u128, total_ms: u128 },

    /// 流式生成异常
    AiAssistantError(String),
}
