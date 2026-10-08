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

    async fn run_async(mut self) -> Result<(), PredictError> {
        let mut latest_request: Option<PredictionRequest> = None;
        loop {
            let request = match latest_request.take() {
                Some(req) => req,
                None => match self.requests.recv().await {
                    Some(req) => req,
                    None => break,
                },
            };

            // 防抖窗口：在 debounce 期间持续收新请求，仅保留最新
            let mut req = request;
            let debounce_deadline = tokio::time::Instant::now() + self.debounce;
            while tokio::time::Instant::now() < debounce_deadline {
                let remaining =
                    debounce_deadline.saturating_duration_since(tokio::time::Instant::now());
                tokio::select! {
                    newer = self.requests.recv() => {
                        match newer {
                            Some(n) => req = n,
                            None => return Ok(()),
                        }
                    }
                    _ = tokio::time::sleep(remaining) => break,
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
                res = self.client.complete(&req) => {
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
            }
        }
        Ok(())
    }

    fn reply(&self, sequence: u64, reply: Cached) {
        // 接收端没了说明 Predictor 已经被 drop，线程随后也会退出
        let _ = self.responses.send(Prediction {
            sequence,
            words: reply.words,
            sentence: reply.sentence,
            alternatives: reply.alternatives,
        });
    }
}
