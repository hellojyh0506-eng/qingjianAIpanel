//! 交给 UI 线程执行的命令。

use qingjian_platform::protocol::{Frame, ScreenRect};

use super::companion::CompanionData;
use super::drill::DrillResultData;
use crate::dispatch::{RenderSettings, StatusView};

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

    /// 显示 / 更新 English Assistant（学习助手伴侣窗）。
    CompanionShow(Box<(CompanionData, ScreenRect)>),

    /// 收起 English Assistant。
    CompanionHide,

    /// 启动 Drill（主动练习独立窗口）。
    DrillStart(Box<(String, String, ScreenRect)>),

    /// 下发 Drill 对比批改结果。
    DrillResult(Box<DrillResultData>),

    /// 收起 Drill 窗口。
    DrillHide,

    /// 显示 / 聚焦 AI Assistant 独立对话窗口 (anchor, initial_prompt)
    AiAssistantShow(Box<(Option<windows::Win32::Foundation::RECT>, Option<String>)>),

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
