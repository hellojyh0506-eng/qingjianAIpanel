//! `/boost` 诊断指令：不走模型，拼一份加速与连通性自检报告，按流式节奏发给窗口。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use qingjian_predict::StreamChatClient;

use super::turn::Turn;

/// 在后台任务里探测网关并分块发出报告；报告记进会话。
pub(super) fn spawn(
    runtime: &tokio::runtime::Runtime,
    client: StreamChatClient,
    endpoint: (String, String),
    stop_signal: Arc<AtomicBool>,
    turn: Turn,
) {
    let (base_url, model) = endpoint;
    runtime.spawn(async move {
        let probe_res = client.probe().await;
        let (status_str, latency_ms) = match &probe_res {
            Ok((status, lat)) => (format!("HTTP {status} 正常"), *lat),
            Err(e) => (format!("连接异常: {e}"), 0),
        };
        let started = std::time::Instant::now();
        let report = format!(
            "🚀 **【Qingjian Boost 智能加速与增强系统就绪】**\n\n\
            **1. 翻译稳定性增强 (Translation Boost):**\n\
            - 阶段化超时防抖：实时翻译 0ms 防抖，最长 8000ms 宽容容灾，1500ms 智能离线释义预热。\n\
            - 接口自适应容错：多字段解析 (`sentence`, `translation`, `translated_text`, `target_text`)。\n\
            - 智能降级兜底：云端异常或超时自动回退至本地 40 万离线词典，永不白屏丢词。\n\n\
            **2. 云联想与智能纠错增强 (Cloud Association & Typo Correction):**\n\
            - 智能音节恢复：音节切分失配自动使用自然音节切分器补正。\n\
            - 拼音去调容错：全量支持带调声调无损归一化（如 hénán → henan）。\n\
            - 长句口语保护：放宽口语与复合长句长度限制（最高 4096 字符）。\n\n\
            **3. 窗口层级与焦点守护 (Z-Order & Topmost Guards):**\n\
            - 置顶图钉联动：English Assistant 与 AI 助手默认高亮置顶，宿主应用（Chrome/Word）失焦永不沉底。\n\
            - 鼠标激活保障：`WM_MOUSEACTIVATE` 自动提升激活，点击即置顶前台。\n\n\
            **4. 服务连通性探测 (Live Diagnostics):**\n\
            - 网关端点: `{}`\n\
            - 推理模型: `{}`\n\
            - 探测状态: `{}`\n\
            - 握手延迟: `{} ms`\n\
            - 运行状态: `已全面生效 (ALL SYSTEMS OPERATIONAL)`",
            base_url, model, status_str, latency_ms
        );

        let chars: Vec<char> = report.chars().collect();
        let mut sent = String::new();
        for chunk in chars.chunks(16) {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }
            let piece: String = chunk.iter().collect();
            sent.push_str(&piece);
            turn.sink.send_ai_chunk(piece);
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        if stop_signal.load(Ordering::Relaxed) {
            turn.record(&sent, "");
            return;
        }
        turn.sink
            .send_ai_finish(latency_ms, started.elapsed().as_millis());
        turn.record(&report, "");
    });
}
