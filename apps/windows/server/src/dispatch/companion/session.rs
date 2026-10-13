//! 学习助手的一次会话：对一条表达按需发起三个标签的流式生成，区块经 [`CandidateSink`] 送到面板。
//!
//! 同一个 `(标签, 场景)` 同一时刻只跑一路；跑完的结果记在内存里（收藏与练习取例句用），合格的再写进磁盘缓存，
//! 下次同一条表达秒开。面板关闭或换了表达，代次对不上，旧的流自己停下。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use qingjian_predict::StreamChatClient;
use qingjian_predict::companion::{
    Block, BlockCache, CompanionRequest, Outcome, Scene, TEMPERATURE, Tab, consume, messages,
};

use crate::dispatch::CandidateSink;

/// 打开后空闲这么久，再悄悄生成真实对话（默认场景）。
const DIALOGUE_IDLE_DELAY: Duration = Duration::from_secs(2);

/// 联网模型没开时给面板的提示。
const DISABLED_MESSAGE: &str = "联网模型未开启，学习助手需要在设置里打开「预测」。";

/// 一路生成：标签序号与场景序号（只有真实对话的场景有意义，其余标签恒为 0）。
type Slot = (usize, usize);

/// 请求从哪来，决定重复请求怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// 用户点了标签 / 场景：面板上这块是空的，一定要把内容送过去。
    User,

    /// 后台预取：这路已经发过就不再发，免得内容重复。
    Prefetch,

    /// 用户点了重试：绕过缓存重新生成。
    Retry,
}

/// 调用模型所需的连接信息。
#[derive(Clone)]
pub(super) struct Endpoint {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Default)]
struct Inner {
    /// 当前对话场景序号。
    scene: usize,

    /// 正在生成的路，附带已经送出去的区块（用户中途切回来时补发）。
    streaming: HashMap<Slot, Vec<Block>>,

    /// 本次会话里完整生成过的路。
    done: HashMap<Slot, Vec<Block>>,

    /// 本次会话里已经发给面板的路。
    requested: HashSet<Slot>,
}

pub(crate) struct CompanionSession {
    zh: String,
    en: String,
    enabled: bool,
    endpoint: Endpoint,
    sink: Mutex<Box<dyn CandidateSink>>,
    runtime: Arc<tokio::runtime::Runtime>,
    generation: Arc<AtomicU64>,
    my_generation: u64,
    cache: Arc<Mutex<BlockCache>>,
    cache_path: Option<PathBuf>,
    inner: Mutex<Inner>,
    prefetched: AtomicBool,
}

impl CompanionSession {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        zh: String,
        en: String,
        enabled: bool,
        endpoint: Endpoint,
        sink: Box<dyn CandidateSink>,
        runtime: Arc<tokio::runtime::Runtime>,
        generation: Arc<AtomicU64>,
        cache: Arc<Mutex<BlockCache>>,
        cache_path: Option<PathBuf>,
    ) -> Arc<Self> {
        let my_generation = generation.load(Ordering::Relaxed);
        Arc::new(Self {
            zh,
            en,
            enabled,
            endpoint,
            sink: Mutex::new(sink),
            runtime,
            generation,
            my_generation,
            cache,
            cache_path,
            inner: Mutex::new(Inner::default()),
            prefetched: AtomicBool::new(false),
        })
    }

    pub(super) fn zh(&self) -> &str {
        &self.zh
    }

    pub(super) fn en(&self) -> &str {
        &self.en
    }

    pub(super) fn set_scene(&self, scene: usize) {
        self.lock().scene = scene;
    }

    /// 当前对话场景序号。
    pub(super) fn scene(&self) -> usize {
        self.lock().scene
    }

    /// 语感解析里的例句，收藏与练习用；还没生成出来就是 `None`。
    pub(super) fn example(&self) -> Option<(String, String, String)> {
        let inner = self.lock();
        let slot = (0, 0);
        let blocks = inner
            .done
            .get(&slot)
            .or_else(|| inner.streaming.get(&slot))?;
        blocks.iter().find_map(|block| match block {
            Block::Example { en, zh, hl } => Some((en.clone(), zh.clone(), hl.clone())),
            _ => None,
        })
    }

    /// 打开面板时的入口：先生成语感解析，其余按需。
    pub(super) fn start(self: &Arc<Self>) {
        self.request(0, 0, Origin::User);
    }

    /// 要一路内容。`scene` 只对真实对话标签有意义。
    pub(super) fn request(self: &Arc<Self>, tab: usize, scene: usize, origin: Origin) {
        let Some(tab_kind) = Tab::from_index(tab) else {
            return;
        };
        let scene = if tab_kind == Tab::Dialogue { scene } else { 0 };
        let slot = (tab, scene);
        let mut inner = self.lock();
        if origin == Origin::Prefetch && inner.requested.contains(&slot) {
            return;
        }
        if origin != Origin::Retry {
            if let Some(blocks) = inner.streaming.get(&slot) {
                self.replay(tab, scene, blocks, false);
                inner.requested.insert(slot);
                return;
            }
            if let Some(blocks) = inner.done.get(&slot) {
                self.replay(tab, scene, blocks, true);
                inner.requested.insert(slot);
                return;
            }
        } else if inner.streaming.contains_key(&slot) {
            return;
        }
        inner.requested.insert(slot);

        let request = CompanionRequest::new(
            tab_kind,
            Scene::from_index(scene).unwrap_or(Scene::Daily),
            &self.zh,
            &self.en,
        );
        if origin != Origin::Retry {
            let cached = self
                .cache
                .lock()
                .ok()
                .and_then(|mut cache| cache.get(&request.cache_key()));
            if let Some(blocks) = cached {
                self.replay(tab, scene, &blocks, true);
                inner.done.insert(slot, blocks);
                return;
            }
        }
        if !self.enabled {
            inner.requested.remove(&slot);
            self.with_sink(|sink| sink.fail_companion(tab, scene, DISABLED_MESSAGE.to_owned()));
            return;
        }
        inner.done.remove(&slot);
        inner.streaming.insert(slot, Vec::new());
        drop(inner);
        self.spawn_stream(request, tab_kind, slot);
    }

    /// 语感解析一出表头就预取「更自然地说」；解析完空闲一会儿再预取默认场景的对话。
    fn prefetch_after_header(self: &Arc<Self>) {
        if self.prefetched.swap(true, Ordering::Relaxed) {
            return;
        }
        self.request(1, 0, Origin::Prefetch);
    }

    fn prefetch_dialogue_later(self: &Arc<Self>) {
        let session = Arc::clone(self);
        self.runtime.spawn(async move {
            tokio::time::sleep(DIALOGUE_IDLE_DELAY).await;
            if session.is_current() {
                let scene = session.scene();
                session.request(2, scene, Origin::Prefetch);
            }
        });
    }

    fn spawn_stream(self: &Arc<Self>, request: CompanionRequest, tab_kind: Tab, slot: Slot) {
        let session = Arc::clone(self);
        let client = StreamChatClient::new(
            self.endpoint.base_url.clone(),
            self.endpoint.api_key.clone(),
            self.endpoint.model.clone(),
        )
        .with_thinking(false)
        .with_temperature(TEMPERATURE);
        let chat = messages(&request);
        let (tab, scene) = slot;
        self.runtime.spawn(async move {
            let (tx, rx) = tokio::sync::mpsc::channel(32);
            let stop = Arc::new(AtomicBool::new(false));
            let stream_stop = Arc::clone(&stop);
            let max_tokens = tab_kind.max_tokens();
            tokio::spawn(async move {
                client.stream_chat(chat, max_tokens, tx, stream_stop).await;
            });
            let sent_header = AtomicBool::new(false);
            let outcome = consume(
                rx,
                tab_kind,
                || !session.is_current(),
                |block| {
                    session.deliver(slot, block);
                    if tab == 0
                        && matches!(block, Block::Header { .. })
                        && !sent_header.swap(true, Ordering::Relaxed)
                    {
                        session.prefetch_after_header();
                    }
                },
            )
            .await;
            match outcome {
                Outcome::Cancelled => stop.store(true, Ordering::Relaxed),
                Outcome::Done(blocks) => {
                    session.complete(&request, tab_kind, slot, blocks);
                    if tab == 0 {
                        session.prefetch_dialogue_later();
                    }
                }
                Outcome::Failed { message, .. } => {
                    tracing::warn!(tab, scene, %message, "学习助手生成失败");
                    let mut inner = session.lock();
                    inner.streaming.remove(&slot);
                    inner.requested.remove(&slot);
                    session.with_sink(|sink| sink.fail_companion(tab, scene, message));
                }
            }
        });
    }

    /// 一个区块到了：记下来，同时送给面板（同一把锁里做，保证与补发不乱序）。
    fn deliver(&self, slot: Slot, block: &Block) {
        let mut inner = self.lock();
        if let Some(sent) = inner.streaming.get_mut(&slot) {
            sent.push(block.clone());
        }
        self.with_sink(|sink| sink.companion_block(slot.0, slot.1, block.clone()));
    }

    /// 一路正常结束：转入内存结果，合格的写缓存。
    fn complete(&self, request: &CompanionRequest, tab_kind: Tab, slot: Slot, blocks: Vec<Block>) {
        if BlockCache::worth_caching(tab_kind, &blocks)
            && let Ok(mut cache) = self.cache.lock()
        {
            cache.put(request.cache_key(), blocks.clone());
            if let Some(path) = &self.cache_path
                && let Err(error) = cache.save(path)
            {
                tracing::warn!(%error, "学习助手缓存写盘失败");
            }
        }
        let mut inner = self.lock();
        inner.streaming.remove(&slot);
        inner.done.insert(slot, blocks);
        self.with_sink(|sink| sink.finish_companion(slot.0, slot.1));
    }

    /// 把已有区块一次送给面板；`finished` 为真再补一个结束。
    fn replay(&self, tab: usize, scene: usize, blocks: &[Block], finished: bool) {
        self.with_sink(|sink| {
            for block in blocks {
                sink.companion_block(tab, scene, block.clone());
            }
            if finished {
                sink.finish_companion(tab, scene);
            }
        });
    }

    fn is_current(&self) -> bool {
        self.generation.load(Ordering::Relaxed) == self.my_generation
    }

    fn with_sink(&self, send: impl FnOnce(&dyn CandidateSink)) {
        if !self.is_current() {
            return;
        }
        if let Ok(sink) = self.sink.lock() {
            send(sink.as_ref());
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
