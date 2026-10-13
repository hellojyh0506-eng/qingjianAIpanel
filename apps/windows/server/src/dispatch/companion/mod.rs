//! 学习助手面板（English Assistant）的业务：开关、按标签 / 场景按需生成、收藏进间隔复习、挖空练习。
//! 生成流程在 [`session`]；面板窗口在 `ui::companion`，两边只通过 [`CandidateSink`] 与 [`CandidateAction`] 说话。

mod session;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use qingjian_platform::protocol::ScreenRect;
use qingjian_predict::companion::{BlockCache, cloze};

pub(crate) use self::session::CompanionSession;
use self::session::{Endpoint, Origin};
use super::Router;
use super::composed;

/// 面板磁盘缓存的文件名，放在用户目录。
const CACHE_FILE: &str = "companion-cache.json";

/// 默认的面板模型：小而快，关思考。
const DEFAULT_PANEL_MODEL: &str = "Qwen/Qwen3-14B";

const DEFAULT_BASE_URL: &str = "https://api.siliconflow.cn/v1";

impl Router {
    /// 打开学习助手面板：定下要学的中文与英文，立刻显示骨架并开始生成语感解析。
    pub(crate) fn open_assistant(&mut self) {
        self.assistant_open = true;
        self.companion_generation.fetch_add(1, Ordering::Relaxed);
        self.notice = Some("📖 已开启 English Assistant (Ctrl+Alt+E)".to_string());
        if let Some(card) = &mut self.passive_learning_card {
            card.expanded = true;
        }
        let rect = self.effective_display_rect().unwrap_or(ScreenRect {
            left: 200,
            top: 200,
            right: 400,
            bottom: 230,
        });
        let (zh, en) = self.assistant_subject();
        let saved = is_saved(&zh, &en);
        self.candidates
            .show_companion(zh.clone(), en.clone(), saved, rect);

        let session = CompanionSession::new(
            zh,
            en,
            self.config.predict.enabled,
            self.panel_endpoint(),
            self.candidates.clone_sink(),
            Arc::clone(&self.ai_runtime),
            Arc::clone(&self.companion_generation),
            self.companion_cache(),
            cache_path(),
        );
        session.start();
        self.companion = Some(session);
    }

    /// 关闭学习助手面板；还在跑的流自己会停。
    pub(crate) fn close_assistant(&mut self) {
        self.assistant_open = false;
        self.companion_generation.fetch_add(1, Ordering::Relaxed);
        self.companion = None;
        if let Some(card) = &mut self.passive_learning_card {
            card.expanded = false;
        }
        self.candidates.hide_companion();
        self.notice = Some("已收起 English Assistant".to_string());
    }

    /// 面板切到还没生成过的标签。
    pub(crate) fn companion_select_tab(&mut self, tab: usize) {
        if let Some(session) = self.companion.clone() {
            session.request(tab, session.scene(), Origin::User);
        }
    }

    /// 面板切到另一个对话场景。
    pub(crate) fn companion_select_scene(&mut self, scene: usize) {
        if let Some(session) = self.companion.clone() {
            session.set_scene(scene);
            session.request(2, scene, Origin::User);
        }
    }

    /// 面板上点了重试。
    pub(crate) fn companion_retry(&mut self, tab: usize, scene: usize) {
        if let Some(session) = self.companion.clone() {
            session.request(tab, scene, Origin::Retry);
        }
    }

    /// 收藏当前表达并加入间隔复习；例句存进备注，复习时能看到。
    pub(crate) fn companion_save(&mut self) {
        let Some(session) = self.companion.clone() else {
            return;
        };
        let (zh, en) = (session.zh().to_owned(), session.en().to_owned());
        if en.trim().is_empty() {
            self.candidates
                .companion_notice("还没有可收藏的英文表达".to_string(), false);
            return;
        }
        let Some(user_dir) = qingjian_platform::dirs::user_dir() else {
            self.candidates
                .companion_notice("找不到用户目录，收藏失败".to_string(), false);
            return;
        };
        let notes = session
            .example()
            .map(|(example_en, example_zh, _)| format!("{example_en}\n{example_zh}"));
        let mut vault = qingjian_learning::LearningVault::open(&user_dir);
        vault.save_expression(&zh, &en, Some("Natural"), notes.as_deref());
        match vault.flush() {
            Ok(()) => self
                .candidates
                .companion_notice("已收藏，并加入复习".to_string(), true),
            Err(error) => {
                tracing::warn!(%error, "收藏写盘失败");
                self.candidates
                    .companion_notice("收藏失败，请稍后重试".to_string(), false);
            }
        }
    }

    /// 练一句：把语感解析的例句里要学的短语挖掉让用户补；还没有例句就退回整句翻译练习。
    pub(crate) fn companion_practice(&mut self) {
        let Some(session) = self.companion.clone() else {
            return;
        };
        let blank = session
            .example()
            .and_then(|(example_en, example_zh, phrase)| {
                cloze(&example_en, &phrase).map(|sentence| (example_zh, phrase, sentence))
            });
        let Some((example_zh, phrase, sentence)) = blank else {
            self.start_drill_session();
            return;
        };
        let rect = self.effective_display_rect().unwrap_or(ScreenRect {
            left: 200,
            top: 200,
            right: 400,
            bottom: 230,
        });
        self.begin_drill(example_zh, phrase, Some(sentence), rect);
    }

    /// 面板要学的中文与英文：优先被动学习卡，其次翻译结果、当前高亮候选。
    fn assistant_subject(&mut self) -> (String, String) {
        let zh = self
            .passive_learning_card
            .as_ref()
            .map(|c| c.target_text.clone())
            .or_else(|| self.passive_translation_target.clone())
            .or_else(|| self.layout_candidate(self.highlight).map(|c| c.text))
            .or_else(|| self.layout_candidate(0).map(|c| c.text))
            .unwrap_or_else(|| "当前表达".to_string());
        let en = self
            .passive_learning_card
            .as_ref()
            .and_then(|c| c.primary.as_ref().map(|p| p.text.clone()))
            .filter(|s| composed::is_valid_english_text(s))
            .or_else(|| {
                self.passive_translation
                    .clone()
                    .filter(|s| composed::is_valid_english_text(s))
            })
            .or_else(|| {
                self.engine
                    .translate_word(&zh)
                    .and_then(|t| t.senses().first().map(|s| s.text.clone()))
                    .filter(|s| composed::is_valid_english_text(s))
            })
            .or_else(|| composed::fallback_idiom_english(&zh))
            .unwrap_or_default();
        (zh, en)
    }

    /// 面板用的模型与密钥：没配 `panel_model` 就用小模型，关思考由会话负责。
    fn panel_endpoint(&self) -> Endpoint {
        let predict = &self.config.predict;
        let base_url = if predict.base_url.is_empty() {
            DEFAULT_BASE_URL.to_string()
        } else {
            predict.base_url.clone()
        };
        let api_key = predict.api_key.clone().unwrap_or_else(|| {
            std::env::var("QINGJIAN_API_KEY")
                .or_else(|_| std::env::var("SILICONFLOW_API_KEY"))
                .unwrap_or_default()
        });
        let model = match predict.panel_model() {
            "" => DEFAULT_PANEL_MODEL.to_string(),
            panel => panel.to_string(),
        };
        Endpoint {
            base_url,
            api_key,
            model,
        }
    }

    /// 磁盘缓存：第一次用到时才读，之后整个进程共用一份。
    fn companion_cache(&mut self) -> Arc<Mutex<BlockCache>> {
        let cache = self.companion_cache.get_or_insert_with(|| {
            let loaded = cache_path()
                .map(|path| BlockCache::load(&path))
                .unwrap_or_default();
            Arc::new(Mutex::new(loaded))
        });
        Arc::clone(cache)
    }
}

fn cache_path() -> Option<PathBuf> {
    qingjian_platform::dirs::user_dir().map(|dir| dir.join(CACHE_FILE))
}

/// 这条表达（中文 + 英文）已经收藏过。
fn is_saved(zh: &str, en: &str) -> bool {
    let Some(user_dir) = qingjian_platform::dirs::user_dir() else {
        return false;
    };
    qingjian_learning::LearningVault::open(&user_dir)
        .saved_expressions()
        .iter()
        .any(|saved| {
            saved.prompt == zh.trim() && (en.trim().is_empty() || saved.expression == en.trim())
        })
}
