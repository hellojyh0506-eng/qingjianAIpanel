//! 模型逐行输出的一个区块：UI 每收到一个就填一张卡片。字段名与 `docs/plan/ai-panel-rebuild.md` 8.3 的 JSONL 约定一致。

use serde::{Deserialize, Serialize};

use super::tab::Tab;

/// 对话里的说话人。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speaker {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Block {
    /// 语感解析顶部：特点标签（≤ 3 个）与 CEFR 等级。
    Header { tags: Vec<String>, level: String },

    /// 意思。
    Meaning { text: String },

    /// 语气。
    Tone { text: String },

    /// 使用场景。
    Usage { text: String },

    /// 自然例句，`hl` 是句中要高亮（加粗、挖空）的短语。
    Example {
        en: String,
        zh: String,
        #[serde(default)]
        hl: String,
    },

    /// 搭配与语法细节的一项。
    Detail { title: String, body: String },

    /// 更自然地说的一条替换表达。
    Alt {
        i: u8,
        expr: String,
        zh: String,
        #[serde(default)]
        tag: String,
        tone: String,
        diff: String,
        ex_en: String,
        ex_zh: String,
    },

    /// 对话场景标题。
    Scene { title: String },

    /// 雅思场景的考官问题。
    Question { en: String, zh: String },

    /// 一句对白。
    Turn {
        sp: Speaker,
        en: String,
        zh: String,
        #[serde(default)]
        hl: String,
    },

    /// 表达亮点的一条。
    Point { text: String },
}

impl Block {
    /// 区块种类名，与 JSONL 的 `k` 一致；按种类限数量时用。
    pub fn kind(&self) -> &'static str {
        match self {
            Block::Header { .. } => "header",
            Block::Meaning { .. } => "meaning",
            Block::Tone { .. } => "tone",
            Block::Usage { .. } => "usage",
            Block::Example { .. } => "example",
            Block::Detail { .. } => "detail",
            Block::Alt { .. } => "alt",
            Block::Scene { .. } => "scene",
            Block::Question { .. } => "question",
            Block::Turn { .. } => "turn",
            Block::Point { .. } => "point",
        }
    }

    /// 这种区块属于哪个标签。
    pub fn tab(&self) -> Tab {
        match self {
            Block::Header { .. }
            | Block::Meaning { .. }
            | Block::Tone { .. }
            | Block::Usage { .. }
            | Block::Example { .. }
            | Block::Detail { .. } => Tab::Nuance,
            Block::Alt { .. } => Tab::Alternatives,
            Block::Scene { .. }
            | Block::Question { .. }
            | Block::Turn { .. }
            | Block::Point { .. } => Tab::Dialogue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_lines() {
        let line =
            r#"{"k":"turn","sp":"B","en":"You'll be fine.","zh":"你会没问题的。","hl":"be fine"}"#;
        let block: Block = serde_json::from_str(line).unwrap();
        assert_eq!(
            block,
            Block::Turn {
                sp: Speaker::B,
                en: "You'll be fine.".into(),
                zh: "你会没问题的。".into(),
                hl: "be fine".into()
            }
        );
        let header: Block =
            serde_json::from_str(r#"{"k":"header","tags":["自然口语"],"level":"B1-B2"}"#).unwrap();
        assert!(matches!(header, Block::Header { .. }));
        // hl 可以省略
        let example: Block =
            serde_json::from_str(r#"{"k":"example","en":"Do it.","zh":"去做。"}"#).unwrap();
        assert!(matches!(example, Block::Example { ref hl, .. } if hl.is_empty()));
    }

    #[test]
    fn every_kind_belongs_to_one_tab() {
        let alt = Block::Alt {
            i: 1,
            expr: "x".into(),
            zh: "y".into(),
            tag: String::new(),
            tone: String::new(),
            diff: String::new(),
            ex_en: String::new(),
            ex_zh: String::new(),
        };
        assert_eq!((alt.kind(), alt.tab()), ("alt", Tab::Alternatives));
        assert_eq!(Block::Point { text: "x".into() }.tab(), Tab::Dialogue);
        assert_eq!(Block::Meaning { text: "x".into() }.tab(), Tab::Nuance);
    }

    #[test]
    fn serializes_back_with_the_tag() {
        let json = serde_json::to_string(&Block::Point {
            text: "自然".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"k":"point","text":"自然"}"#);
    }
}
