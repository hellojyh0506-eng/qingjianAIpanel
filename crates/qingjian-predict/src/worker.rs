//! 后台线程：收请求、防抖、查缓存、发网络请求、回结果。

use std::cell::{Cell, RefCell};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use qingjian_core::{Prediction, PredictionKind, PredictionLane, PredictionRequest};

use crate::cache::PredictionCache;
use crate::chat_client::ChatClient;
use crate::error::PredictError;
use crate::prompt::Reply;

/// 打字与学习两条通道合起来最多同时飞几个请求（限流友好）；深度通道耗时长、很少用，不占这个名额。
const MAX_CONCURRENT_REQUESTS: usize = 2;

/// 缓存条数。
const CACHE_CAPACITY: usize = 64;

/// 距上次网络请求超过这么久，下一个请求前先预热连接（服务端多在一分钟内回收空闲连接）。
const WARM_UP_AFTER_IDLE: Duration = Duration::from_secs(45);

/// 完整长拼音的候选重排稍作延迟，给快速打字时可能紧随其后的尾部错字留出合并窗口。
const LONG_RERANK_DEBOUNCE: Duration = Duration::from_millis(280);
const INTENT_DEBOUNCE: Duration = Duration::from_millis(50);

/// Resolve 防抖：快速连敲时只发最后一个，120 ms 内没有新键才发。
const RESOLVE_DEBOUNCE: Duration = Duration::from_millis(120);

fn effective_debounce(request: &PredictionRequest, normal: Duration) -> Duration {
    match request.kind {
        qingjian_core::PredictionKind::Compare => Duration::ZERO,
        qingjian_core::PredictionKind::Intent => INTENT_DEBOUNCE,
        qingjian_core::PredictionKind::Resolve => RESOLVE_DEBOUNCE,
        qingjian_core::PredictionKind::Rerank if request.letters.len() >= 7 => {
            LONG_RERANK_DEBOUNCE
        }
        _ => normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: qingjian_core::PredictionKind, letters: &str) -> PredictionRequest {
        PredictionRequest {
            resolve_candidates: Vec::new(),
            sequence: 1,
            kind,
            before: String::new(),
            after: String::new(),
            pinyin: letters.to_owned(),
            letters: letters.to_owned(),
            syllables: 0,
            candidates: Vec::new(),
            candidate_readings: Vec::new(),
            guess: String::new(),
            current_expression: String::new(),
            max_items: 0,
            want_sentence: false,
            text: String::new(),
            target_language: String::new(),
            user_attempt: String::new(),
            reference: String::new(),
        }
    }

    #[test]
    fn long_rerank_waits_for_a_possible_trailing_typo_but_intent_is_fast() {
        assert_eq!(
            effective_debounce(&request(qingjian_core::PredictionKind::Rerank, "kaifazhe"), Duration::from_millis(150)),
            Duration::from_millis(280)
        );
        assert_eq!(
            effective_debounce(&request(qingjian_core::PredictionKind::Intent, "kaifazhes"), Duration::from_millis(150)),
            Duration::from_millis(50)
        );
        assert_eq!(
            effective_debounce(
                &request(qingjian_core::PredictionKind::Resolve, "kaifazhes"),
                Duration::from_millis(300)
            ),
            Duration::from_millis(120)
        );
        assert_eq!(
            effective_debounce(&request(qingjian_core::PredictionKind::Compare, "hello"), Duration::from_millis(150)),
            Duration::ZERO
        );
    }
}

pub struct Worker {
    /// 请求入口。主线程 drop 掉发送端后通道自然关闭。
    requests: UnboundedReceiver<PredictionRequest>,

    /// 结果出口。
    responses: Sender<Prediction>,

    /// 网络客户端。
    client: ChatClient,

    /// 防抖窗口。
    debounce: Duration,

    /// 结果缓存。
    cache: PredictionCache,
}

/// 三条通道共用的状态。后台线程是单线程运行时，三个通道循环只在 await 处交替运行，所以用 `RefCell` / `Cell`。
struct Shared {
    responses: Sender<Prediction>,
    client: ChatClient,
    debounce: Duration,
    cache: RefCell<PredictionCache>,

    /// 上次碰网络（含预热）的时刻。
    last_network: Cell<Option<Instant>>,

    /// 打字与学习通道共用的并发名额。
    permits: Semaphore,
}

/// 缓存里存的是解析后的回复。
type Cached = Reply;

impl Worker {
    pub fn new(
        requests: UnboundedReceiver<PredictionRequest>,
        responses: Sender<Prediction>,
        client: ChatClient,
        debounce: Duration,
    ) -> Self {
        Self {
            requests,
            responses,
            client,
            debounce,
            cache: PredictionCache::with_capacity(CACHE_CAPACITY),
        }
    }

    /// 阻塞运行直到发送端全部关闭。
    pub fn run(self) -> Result<(), PredictError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.run_async())
    }

    /// 一个分发循环把请求按通道派给三个独立循环，各自防抖、各自「最新优先」，互不打断。
    async fn run_async(self) -> Result<(), PredictError> {
        let shared = Shared {
            responses: self.responses,
            client: self.client,
            debounce: self.debounce,
            cache: RefCell::new(self.cache),
            last_network: Cell::new(None),
            permits: Semaphore::new(MAX_CONCURRENT_REQUESTS),
        };
        let (typing_tx, typing_rx) = unbounded_channel();
        let (learning_tx, learning_rx) = unbounded_channel();
        let (deep_tx, deep_rx) = unbounded_channel();
        let senders: [UnboundedSender<PredictionRequest>; PredictionLane::COUNT] =
            [typing_tx, learning_tx, deep_tx];
        shared.warm_up_if_idle();
        tokio::join!(
            route(self.requests, senders, &shared),
            shared.lane_loop(PredictionLane::Typing, typing_rx),
            shared.lane_loop(PredictionLane::Learning, learning_rx),
            shared.lane_loop(PredictionLane::Deep, deep_rx),
        );
        Ok(())
    }
}

/// 把收到的请求派到各自通道；入口关闭后 senders 随之释放，通道循环自然退出。
async fn route(
    mut requests: UnboundedReceiver<PredictionRequest>,
    senders: [UnboundedSender<PredictionRequest>; PredictionLane::COUNT],
    shared: &Shared,
) {
    while let Some(request) = requests.recv().await {
        // 与防抖窗口并行握手，真正发请求时连接已经热了
        shared.warm_up_if_idle();
        let _ = senders[request.kind.lane().index()].send(request);
    }
}

impl Shared {
    async fn lane_loop(&self, lane: PredictionLane, mut requests: UnboundedReceiver<PredictionRequest>) {
        // 深度请求（深度学习、对比）是用户明确要的，不被同通道的新请求打断
        let interruptible = lane != PredictionLane::Deep;
        let mut latest_request: Option<PredictionRequest> = None;
        loop {
            let request = match latest_request.take() {
                Some(req) => req,
                None => match requests.recv().await {
                    Some(req) => req,
                    None => return,
                },
            };

            // Trailing-edge debounce: each newer keystroke resets the window. Long full-pinyin
            // reranks wait briefly, so a trailing typo can replace the request with Intent before
            // any network call starts.
            let mut req = request;
            loop {
                let effective = effective_debounce(&req, self.debounce);
                if effective.is_zero() {
                    break;
                }
                let deadline = tokio::time::Instant::now() + effective;
                tokio::select! {
                    newer = requests.recv() => {
                        match newer {
                            Some(new_request) => req = new_request,
                            None => return,
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => break,
                }
            }

            // 查缓存
            let key = PredictionCache::key(&req);
            let cached = self.cache.borrow().get(&key).cloned();
            if let Some(reply) = cached {
                tracing::debug!(sequence = req.sequence, kind = ?req.kind, "联想命中缓存");
                self.reply(req.sequence, req.kind, reply);
                continue;
            }

            let start = Instant::now();
            let res = if interruptible {
                tokio::select! {
                    newer = requests.recv() => {
                        match newer {
                            Some(n) => {
                                tracing::info!(
                                    ?lane,
                                    interrupted_seq = req.sequence,
                                    new_seq = n.sequence,
                                    "同通道新请求到达，立刻打断在途网络请求"
                                );
                                latest_request = Some(n);
                                continue;
                            }
                            None => return,
                        }
                    }
                    res = self.send(lane, &req) => res,
                }
            } else {
                self.send(lane, &req).await
            };

            self.last_network.set(Some(Instant::now()));
            match res {
                Ok(reply) => {
                    tracing::info!(
                        ?lane,
                        sequence = req.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        words = reply.words.len(),
                        sentence = reply.sentence.is_some(),
                        "联想完成"
                    );
                    if !reply.is_empty() {
                        self.cache.borrow_mut().insert(key, reply.clone());
                    }
                    self.reply(req.sequence, req.kind, reply);
                }
                Err(error) => {
                    tracing::warn!(
                        ?lane,
                        sequence = req.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        %error,
                        "联想失败，下发空兜底通知"
                    );
                    self.reply(req.sequence, req.kind, Reply::default());
                }
            }
        }
    }

    /// 占一个并发名额（深度通道不占）再发请求；取消时名额随 future 一起释放。
    async fn send(&self, lane: PredictionLane, request: &PredictionRequest) -> Result<Reply, PredictError> {
        let _permit = if lane == PredictionLane::Deep {
            None
        } else {
            self.permits.acquire().await.ok()
        };
        self.client.complete(request).await
    }

    /// 空闲够久（或还没连过）就在后台预热连接，不等结果。
    fn warm_up_if_idle(&self) {
        let idle = self
            .last_network
            .get()
            .is_none_or(|last| last.elapsed() > WARM_UP_AFTER_IDLE);
        if idle {
            self.last_network.set(Some(Instant::now()));
            tokio::spawn(self.client.warm_up_task());
        }
    }

    fn reply(&self, sequence: u64, kind: PredictionKind, reply: Cached) {
        // 接收端没了说明 Predictor 已经被 drop，线程随后也会退出
        let _ = self.responses.send(Prediction {
            sequence,
            kind,
            words: reply.words,
            sentence: reply.sentence,
            alternatives: reply.alternatives,
            phonetic: reply.phonetic,
            explanation: reply.explanation,
            collocations: reply.collocations,
            examples: reply.examples,
            ielts_friendly: reply.ielts_friendly,
            tone: reply.tone,
            scenario: reply.scenario,
            nuance: reply.nuance,
            dialogue: reply.dialogue,
            resolved: None,
        });
    }
}
