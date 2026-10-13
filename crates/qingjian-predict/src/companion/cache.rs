//! 学习助手结果缓存：同一条表达的同一标签（对话再分场景）只生成一次，下次打开直接显示。
//!
//! 最近用到的排在后面，超过 [`CAPACITY`] 条丢最旧的；整份存成一个 JSON 文件，读不出来就当空缓存。

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::block::Block;
use super::tab::Tab;

/// 最多缓存的条数（一条 = 一个标签或一个对话场景的完整结果）。
pub const CAPACITY: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    key: String,

    blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BlockCache {
    /// 旧的在前，新的在后。
    entries: Vec<Entry>,
}

impl BlockCache {
    /// 从文件读；文件不存在或坏了都返回空缓存，不报错。
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// 先写临时文件再改名，写到一半断电也不会留下半份。
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string(self).map_err(io::Error::other)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }

    /// 取缓存并把它标成最近用过。
    pub fn get(&mut self, key: &str) -> Option<Vec<Block>> {
        let at = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(at);
        let blocks = entry.blocks.clone();
        self.entries.push(entry);
        Some(blocks)
    }

    /// 存一条；同键覆盖，超限丢最旧的。
    pub fn put(&mut self, key: String, blocks: Vec<Block>) {
        self.entries.retain(|entry| entry.key != key);
        self.entries.push(Entry { key, blocks });
        if self.entries.len() > CAPACITY {
            let extra = self.entries.len() - CAPACITY;
            self.entries.drain(..extra);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 这批区块是否完整到值得缓存：缺块的残缺结果不存，下次重新生成。
    pub fn worth_caching(tab: Tab, blocks: &[Block]) -> bool {
        let count = |kind: &str| blocks.iter().filter(|block| block.kind() == kind).count();
        match tab {
            Tab::Nuance => ["meaning", "tone", "usage", "example"]
                .iter()
                .all(|kind| count(kind) >= 1),
            Tab::Alternatives => count("alt") >= 3,
            Tab::Dialogue => count("turn") >= 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(text: &str) -> Vec<Block> {
        vec![Block::Point { text: text.into() }]
    }

    #[test]
    fn get_refreshes_recency_and_put_overwrites() {
        let mut cache = BlockCache::default();
        cache.put("a".into(), point("1"));
        cache.put("b".into(), point("2"));
        assert_eq!(cache.get("a"), Some(point("1")));
        cache.put("b".into(), point("3"));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("b"), Some(point("3")));
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn evicts_the_least_recently_used_past_capacity() {
        let mut cache = BlockCache::default();
        for i in 0..CAPACITY {
            cache.put(format!("k{i}"), point("x"));
        }
        // 用一下最旧的，它就不再是最旧
        assert!(cache.get("k0").is_some());
        cache.put("new".into(), point("y"));
        assert_eq!(cache.len(), CAPACITY);
        assert!(cache.get("k0").is_some());
        assert!(cache.get("k1").is_none(), "最久没用的 k1 应被淘汰");
        assert!(cache.get("new").is_some());
    }

    #[test]
    fn survives_a_round_trip_and_bad_files() {
        let dir = std::env::temp_dir().join(format!("qj-cache-{}", std::process::id()));
        let path = dir.join("companion-cache.json");
        let mut cache = BlockCache::default();
        cache.put("a".into(), point("1"));
        cache.save(&path).unwrap();
        assert_eq!(BlockCache::load(&path), cache);
        fs::write(&path, "{ not json").unwrap();
        assert!(BlockCache::load(&path).is_empty());
        assert!(BlockCache::load(&dir.join("none.json")).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn only_complete_results_are_worth_caching() {
        let turn = |sp| Block::Turn {
            sp,
            en: "Hi.".into(),
            zh: "你好。".into(),
            hl: String::new(),
        };
        let three = vec![
            turn(super::super::block::Speaker::A),
            turn(super::super::block::Speaker::B),
            turn(super::super::block::Speaker::A),
        ];
        assert!(BlockCache::worth_caching(Tab::Dialogue, &three));
        assert!(!BlockCache::worth_caching(Tab::Dialogue, &three[..2]));
        assert!(!BlockCache::worth_caching(Tab::Alternatives, &three));
        let mut nuance = vec![
            Block::Meaning {
                text: "意思".into(),
            },
            Block::Tone {
                text: "语气".into(),
            },
            Block::Usage {
                text: "场景".into(),
            },
        ];
        assert!(!BlockCache::worth_caching(Tab::Nuance, &nuance));
        nuance.push(Block::Example {
            en: "Do it.".into(),
            zh: "去做。".into(),
            hl: String::new(),
        });
        assert!(BlockCache::worth_caching(Tab::Nuance, &nuance));
    }
}
