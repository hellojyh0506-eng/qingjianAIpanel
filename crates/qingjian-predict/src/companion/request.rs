//! 一次学习助手请求：要哪个标签、哪个场景、针对哪条表达。也是结果缓存的键。

use super::scene::Scene;
use super::tab::Tab;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompanionRequest {
    pub tab: Tab,

    /// 只有真实对话标签用得上；其它标签统一为 `Scene::Daily`，免得同一份结果缓存成多份。
    pub scene: Scene,

    /// 用户想说的中文。
    pub zh: String,

    /// 焦点英文表达（翻译结果）；可能为空，让模型自己挑一个最自然的。
    pub en: String,
}

impl CompanionRequest {
    pub fn new(tab: Tab, scene: Scene, zh: &str, en: &str) -> Self {
        Self {
            tab,
            scene: if tab == Tab::Dialogue {
                scene
            } else {
                Scene::Daily
            },
            zh: zh.trim().to_owned(),
            en: en.trim().to_owned(),
        }
    }

    /// 缓存键：`中文 | 英文 | 标签 | 场景`。
    pub fn cache_key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.zh,
            self.en,
            self.tab.index(),
            self.scene.index()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_only_matters_for_dialogue() {
        let a = CompanionRequest::new(Tab::Nuance, Scene::Ielts, "尽量", "Do your best");
        let b = CompanionRequest::new(Tab::Nuance, Scene::GenZ, " 尽量 ", "Do your best ");
        assert_eq!(a.cache_key(), b.cache_key());
        let c = CompanionRequest::new(Tab::Dialogue, Scene::Ielts, "尽量", "Do your best");
        let d = CompanionRequest::new(Tab::Dialogue, Scene::GenZ, "尽量", "Do your best");
        assert_ne!(c.cache_key(), d.cache_key());
    }
}
