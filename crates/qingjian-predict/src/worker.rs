//! 后台线程：收请求、防抖、查缓存、发网络请求、回结果。

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

use qingjian_core::{Prediction, PredictionRequest};

use crate::cache::PredictionCache;
use crate::chat_client::ChatClient;
use crate::error::PredictError;
use crate::prompt::Reply;

/// 缓存条数。
const CACHE_CAPACITY: usize = 64;

/// 距上次网络请求超过这么久，下一个请求前先预热连接（服务端多在一分钟内回收空闲连接）。
const WARM_UP_AFTER_IDLE: Duration = Duration::from_secs(45);

/// 完整长拼音的候选重排稍作延迟，给快速打字时可能紧随其后的尾部错字留出合并窗口。
const LONG_RERANK_DEBOUNCE: Duration = Duration::from_millis(280);
const INTENT_DEBOUNCE: Duration = Duration::from_millis(50);

fn effective_debounce(request: &PredictionRequest, normal: Duration) -> Duration {
    match request.kind {
        qingjian_core::PredictionKind::Compare => Duration::ZERO,
        qingjian_core::PredictionKind::Intent => INTENT_DEBOUNCE,
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

    /// 上次碰网络（含预热）的时刻。
    last_network: Option<Instant>,
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
            last_network: None,
        }
    }

    /// 阻塞运行直到发送端全部关闭。
    pub fn run(self) -> Result<(), PredictError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.run_async())
    }

    async fn run_async(mut self) -> Result<(), PredictError> {
        let mut latest_request: Option<PredictionRequest> = None;
        self.warm_up_if_idle();
        loop {
            let request = match latest_request.take() {
                Some(req) => req,
                None => match self.requests.recv().await {
                    Some(req) => {
                        // 与防抖窗口并行握手，真正发请求时连接已经热了
                        self.warm_up_if_idle();
                        req
                    }
                    None => break,
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
                    newer = self.requests.recv() => {
                        match newer {
                            Some(new_request) => req = new_request,
                            None => return Ok(()),
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => break,
                }
            }

            // 查缓存
            let key = PredictionCache::key(&req);
            if let Some(reply) = self.cache.get(&key) {
                tracing::debug!(sequence = req.sequence, kind = ?req.kind, "联想命中缓存");
                self.reply(req.sequence, reply.clone());
                continue;
            }

            let start = Instant::now();
            let is_critical = req.kind == qingjian_core::PredictionKind::DeepLearn
                || req.kind == qingjian_core::PredictionKind::Compare;

            let res = if is_critical {
                self.client.complete(&req).await
            } else {
                tokio::select! {
                    newer = self.requests.recv() => {
                        match newer {
                            Some(n) => {
                                tracing::info!(
                                    interrupted_seq = req.sequence,
                                    new_seq = n.sequence,
                                    "新按键/候选到达，立刻打断在途网络请求"
                                );
                                latest_request = Some(n);
                                continue;
                            }
                            None => return Ok(()),
                        }
                    }
                    res = self.client.complete(&req) => res,
                }
            };

            self.last_network = Some(Instant::now());
            match res {
                Ok(reply) => {
                    tracing::info!(
                        sequence = req.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        words = reply.words.len(),
                        sentence = reply.sentence.is_some(),
                        "联想完成"
                    );
                    if !reply.is_empty() {
                        self.cache.insert(key, reply.clone());
                    }
                    self.reply(req.sequence, reply);
                }
                Err(error) => {
                    tracing::warn!(
                        sequence = req.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        %error,
                        "联想失败，下发空兜底通知"
                    );
                    self.reply(req.sequence, Reply::default());
                }
            }
        }
        Ok(())
    }

    /// 空闲够久（或还没连过）就在后台预热连接，不等结果。
    fn warm_up_if_idle(&mut self) {
        let idle = self
            .last_network
            .is_none_or(|last| last.elapsed() > WARM_UP_AFTER_IDLE);
        if idle {
            self.last_network = Some(Instant::now());
            tokio::spawn(self.client.warm_up_task());
        }
    }

    fn reply(&self, sequence: u64, reply: Cached) {
        // 接收端没了说明 Predictor 已经被 drop，线程随后也会退出
        let _ = self.responses.send(Prediction {
            sequence,
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
        });
    }
}
