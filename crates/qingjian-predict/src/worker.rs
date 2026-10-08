//! 后台线程：收请求、防抖、查缓存、发网络请求、回结果。

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use qingjian_core::{Prediction, PredictionRequest};

use crate::prompt::Reply;

use crate::cache::PredictionCache;
use crate::chat_client::ChatClient;
use crate::error::PredictError;

/// 缓存条数。
const CACHE_CAPACITY: usize = 64;

pub struct Worker {
    /// 请求入口。主线程 drop 掉发送端后线程自然退出。
    requests: Receiver<PredictionRequest>,

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
        requests: Receiver<PredictionRequest>,
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
    pub fn run(mut self) -> Result<(), PredictError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        while let Ok(mut latest) = self.requests.recv() {
            // 立即排空积压的过期请求，直接跳到最新请求，防止多请求串行阻塞
            while let Ok(newer) = self.requests.try_recv() {
                latest = newer;
            }
            let Some(request) = self.debounce(latest) else {
                return Ok(());
            };
            let key = PredictionCache::key(&request);
            if let Some(reply) = self.cache.get(&key) {
                tracing::debug!(sequence = request.sequence, kind = ?request.kind, "联想命中缓存");
                self.reply(request.sequence, reply.clone());
                continue;
            }
            let start = Instant::now();
            match runtime.block_on(self.client.complete(&request)) {
                Ok(reply) => {
                    tracing::info!(
                        sequence = request.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        words = reply.words.len(),
                        sentence = reply.sentence.is_some(),
                        "联想完成"
                    );
                    // 空回复不进缓存：模型偶尔什么都不给（问字尤其），缓存住就等于这个问题以后永远没答案，
                    // 用户重打一遍也只会命中缓存（2026-09-07 `?mumumu` 就是这么卡住的）
                    if !reply.is_empty() {
                        self.cache.insert(key, reply.clone());
                    }
                    self.reply(request.sequence, reply);
                }
                Err(error) => {
                    tracing::warn!(
                        sequence = request.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        %error,
                        "联想失败，下发空兜底通知"
                    );
                    // 关键修复：发生网络超时或模型拒绝时，绝不可静默吞掉请求导致 UI 陷入永久 loading！
                    // 必须下发带 sequence 的空回复，通知上层转入 Failed / 暂无译文 状态
                    self.reply(request.sequence, Reply::default());
                }
            }
        }
        Ok(())
    }

    /// 防抖：在窗口内持续收到新请求就一直等，只保留最后一个。发送端关闭返回 `None`。
    fn debounce(&self, mut latest: PredictionRequest) -> Option<PredictionRequest> {
        loop {
            match self.requests.recv_timeout(self.debounce) {
                Ok(newer) => latest = newer,
                Err(RecvTimeoutError::Timeout) => return Some(latest),
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
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
