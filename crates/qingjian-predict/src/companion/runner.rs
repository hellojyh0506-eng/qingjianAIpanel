//! 消费一路流式事件：边收边切成区块交给调用方，最后给出结果。网络请求由调用方发起，这里只管事件到区块这一段，方便单测。

use tokio::sync::mpsc::Receiver;

use super::block::Block;
use super::jsonl::BlockParser;
use super::tab::Tab;
use crate::stream_client::StreamEvent;

/// 一路流的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// 正常结束，附带全部合格区块。
    Done(Vec<Block>),

    /// 失败；`blocks` 是失败前已经交出去的区块（可能为空）。
    Failed { message: String, blocks: Vec<Block> },

    /// 调用方说不用了（窗口关了或换了表达）。
    Cancelled,
}

/// 流里一个合格区块都没有时给用户看的话。
const EMPTY_MESSAGE: &str = "模型没有返回可用的内容，请重试。";

/// 读完 `events`；每凑齐一个区块调一次 `on_block`。`cancelled` 在每个事件前问一次，真了就立刻返回 [`Outcome::Cancelled`]。
pub async fn consume(
    mut events: Receiver<StreamEvent>,
    tab: Tab,
    cancelled: impl Fn() -> bool,
    mut on_block: impl FnMut(&Block),
) -> Outcome {
    let mut parser = BlockParser::new(tab);
    let mut blocks = Vec::new();
    let mut error = None;
    while let Some(event) = events.recv().await {
        if cancelled() {
            return Outcome::Cancelled;
        }
        match event {
            StreamEvent::Delta(text) => {
                for block in parser.push(&text) {
                    on_block(&block);
                    blocks.push(block);
                }
            }
            StreamEvent::Error(message) => error = Some(message),
            StreamEvent::Done { .. } | StreamEvent::WebSources(_) => {}
        }
    }
    if cancelled() {
        return Outcome::Cancelled;
    }
    for block in parser.finish() {
        on_block(&block);
        blocks.push(block);
    }
    let stats = parser.stats();
    tracing::info!(
        ?tab,
        accepted = stats.accepted,
        bad = stats.bad,
        extra = stats.extra,
        "学习助手一路流解析完成"
    );
    match error {
        Some(message) => Outcome::Failed { message, blocks },
        None if blocks.is_empty() => Outcome::Failed {
            message: EMPTY_MESSAGE.to_owned(),
            blocks,
        },
        None => Outcome::Done(blocks),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use tokio::sync::mpsc;

    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    }

    /// 把几段文字当作流事件塞进通道，随后关闭。
    fn stream(events: Vec<StreamEvent>) -> Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(events.len().max(1));
        for event in events {
            tx.try_send(event).unwrap();
        }
        rx
    }

    fn delta(text: &str) -> StreamEvent {
        StreamEvent::Delta(text.to_owned())
    }

    #[test]
    fn blocks_are_delivered_as_lines_complete_even_when_split_mid_line() {
        let rx = stream(vec![
            delta("{\"k\":\"meaning\",\"text\":\"尽你"),
            delta("所能。\"}\n{\"k\":\"tone\",\"text\":\"鼓励。\"}"),
            StreamEvent::Done {
                ttft_ms: 1,
                total_ms: 2,
            },
        ]);
        let seen = Cell::new(0);
        let outcome = runtime().block_on(consume(
            rx,
            Tab::Nuance,
            || false,
            |_| seen.set(seen.get() + 1),
        ));
        // 最后一行没有换行，流结束时才补出来
        assert_eq!(seen.get(), 2);
        assert!(matches!(outcome, Outcome::Done(blocks) if blocks.len() == 2));
    }

    #[test]
    fn errors_keep_the_blocks_already_delivered() {
        let rx = stream(vec![
            delta("{\"k\":\"meaning\",\"text\":\"尽你所能。\"}\n"),
            StreamEvent::Error("网络连接超时".into()),
        ]);
        let outcome = runtime().block_on(consume(rx, Tab::Nuance, || false, |_| {}));
        match outcome {
            Outcome::Failed { message, blocks } => {
                assert_eq!(message, "网络连接超时");
                assert_eq!(blocks.len(), 1);
            }
            other => panic!("应失败：{other:?}"),
        }
    }

    #[test]
    fn a_stream_without_usable_lines_is_a_failure_not_a_success() {
        let rx = stream(vec![
            delta("抱歉，我无法……\n"),
            StreamEvent::Done {
                ttft_ms: 1,
                total_ms: 2,
            },
        ]);
        let outcome = runtime().block_on(consume(rx, Tab::Alternatives, || false, |_| {}));
        assert!(matches!(outcome, Outcome::Failed { ref message, .. } if message == EMPTY_MESSAGE));
    }

    #[test]
    fn cancellation_stops_before_any_block_is_delivered() {
        let rx = stream(vec![delta("{\"k\":\"meaning\",\"text\":\"尽你所能。\"}\n")]);
        let seen = Cell::new(0);
        let outcome = runtime().block_on(consume(
            rx,
            Tab::Nuance,
            || true,
            |_| seen.set(seen.get() + 1),
        ));
        assert_eq!(outcome, Outcome::Cancelled);
        assert_eq!(seen.get(), 0);
    }
}
