//! 区块校验与清洗：小模型常见的毛病是英文字段写成中文、标签太长、高亮短语不在句子里。
//! 能修的修（截断标签、清空对不上的高亮），修不了的丢掉，让 UI 永远只看到合格的内容。

use super::block::Block;

/// 一句对白最多几个词；提示词要求 ≤ 15，留一点余量不误杀。
const MAX_TURN_WORDS: usize = 22;

/// 校验并清洗一个区块；不合格返回 `None`。
pub(super) fn clean(block: Block) -> Option<Block> {
    match block {
        Block::Header { tags, level } => {
            let tags: Vec<String> = tags
                .into_iter()
                .map(|t| tidy(&t))
                .filter(|t| !t.is_empty() && t.chars().count() <= 4)
                .take(3)
                .collect();
            let level = tidy(&level);
            let level = if is_level(&level) {
                level
            } else {
                String::new()
            };
            (!tags.is_empty() || !level.is_empty()).then_some(Block::Header { tags, level })
        }
        Block::Meaning { text } => chinese(&text, 160).map(|text| Block::Meaning { text }),
        Block::Tone { text } => chinese(&text, 160).map(|text| Block::Tone { text }),
        Block::Usage { text } => chinese(&text, 200).map(|text| Block::Usage { text }),
        Block::Example { en, zh, hl } => {
            let en = english(&en, 40)?;
            let zh = chinese(&zh, 120)?;
            let hl = highlight(&en, &hl);
            Some(Block::Example { en, zh, hl })
        }
        Block::Detail { title, body } => {
            let title = tidy(&title);
            let body = chinese(&body, 200)?;
            (!title.is_empty() && title.chars().count() <= 12)
                .then_some(Block::Detail { title, body })
        }
        Block::Alt {
            i,
            expr,
            zh,
            tag,
            tone,
            diff,
            ex_en,
            ex_zh,
        } => {
            if !(1..=3).contains(&i) {
                return None;
            }
            let tag = tidy(&tag);
            let tag = if tag.chars().count() <= 4 {
                tag
            } else {
                String::new()
            };
            Some(Block::Alt {
                i,
                expr: english(&expr, 12)?,
                zh: chinese(&zh, 60)?,
                tag,
                tone: chinese(&tone, 60)?,
                diff: chinese(&diff, 120)?,
                ex_en: english(&ex_en, 30)?,
                ex_zh: chinese(&ex_zh, 100)?,
            })
        }
        Block::Scene { title } => {
            let title = tidy(&title);
            (!title.is_empty() && title.chars().count() <= 30).then_some(Block::Scene { title })
        }
        Block::Question { en, zh } => Some(Block::Question {
            en: english(&en, 40)?,
            zh: chinese(&zh, 100)?,
        }),
        Block::Turn { sp, en, zh, hl } => {
            let en = english(&en, MAX_TURN_WORDS)?;
            let zh = chinese(&zh, 100)?;
            let hl = highlight(&en, &hl);
            Some(Block::Turn { sp, en, zh, hl })
        }
        Block::Point { text } => chinese(&text, 80).map(|text| Block::Point { text }),
    }
}

/// 去首尾空白，内部换行与连续空白并成一个空格。
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}

/// 英文字段：不含汉字、至少两个字母、词数不超过 `max_words`。
fn english(text: &str, max_words: usize) -> Option<String> {
    let text = tidy(text);
    let letters = text.chars().filter(char::is_ascii_alphabetic).count();
    let ok = letters >= 2 && !text.chars().any(is_cjk) && text.split(' ').count() <= max_words;
    ok.then_some(text)
}

/// 中文字段：至少一个汉字、字数不超过 `max_chars`。
fn chinese(text: &str, max_chars: usize) -> Option<String> {
    let text = tidy(text);
    let ok = text.chars().any(is_cjk) && text.chars().count() <= max_chars;
    ok.then_some(text)
}

/// 高亮短语必须是句子里原样出现的片段（忽略大小写），对不上就不高亮；返回句子里的原写法。
fn highlight(sentence: &str, hl: &str) -> String {
    let hl = tidy(hl);
    if hl.is_empty() {
        return hl;
    }
    let lower_sentence = sentence.to_ascii_lowercase();
    match lower_sentence.find(&hl.to_ascii_lowercase()) {
        // 全是 ASCII 比较，字节下标与原句一致
        Some(start)
            if sentence.is_char_boundary(start) && sentence.is_char_boundary(start + hl.len()) =>
        {
            sentence[start..start + hl.len()].to_owned()
        }
        _ => String::new(),
    }
}

/// CEFR 等级：`B1` 或 `B1-B2`。
fn is_level(text: &str) -> bool {
    let single = |s: &str| matches!(s, "A1" | "A2" | "B1" | "B2" | "C1" | "C2");
    match text.split_once('-') {
        Some((low, high)) => single(low) && single(high),
        None => single(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion::block::Speaker;

    fn turn(en: &str, zh: &str, hl: &str) -> Block {
        Block::Turn {
            sp: Speaker::B,
            en: en.into(),
            zh: zh.into(),
            hl: hl.into(),
        }
    }

    #[test]
    fn header_trims_tags_and_checks_level() {
        let block = Block::Header {
            tags: vec![
                "自然口语".into(),
                "鼓励支持".into(),
                "太长的标签文字".into(),
                "常用表达".into(),
                "多余".into(),
            ],
            level: "B1-B2".into(),
        };
        let Some(Block::Header { tags, level }) = clean(block) else {
            panic!("应保留");
        };
        assert_eq!(tags, ["自然口语", "鼓励支持", "常用表达"]);
        assert_eq!(level, "B1-B2");
        let bad_level = clean(Block::Header {
            tags: vec!["日常".into()],
            level: "高级".into(),
        });
        assert!(matches!(bad_level, Some(Block::Header { level, .. }) if level.is_empty()));
        assert!(
            clean(Block::Header {
                tags: vec![],
                level: "X9".into()
            })
            .is_none()
        );
    }

    #[test]
    fn english_fields_must_be_english() {
        assert!(clean(turn("你好呀", "你好", "")).is_none());
        assert!(clean(turn("Just do your best.", "尽力就好。", "")).is_some());
        // 中文字段是英文也不要
        assert!(clean(turn("Just do your best.", "Just do it", "")).is_none());
        // 句子太长
        let long = "word ".repeat(30);
        assert!(clean(turn(long.trim(), "太长了", "")).is_none());
    }

    #[test]
    fn highlight_must_appear_in_the_sentence_and_keeps_sentence_casing() {
        let Some(Block::Turn { hl, .. }) = clean(turn(
            "You'll be fine. Just do your best.",
            "你没问题的。",
            "just DO your best",
        )) else {
            panic!();
        };
        assert_eq!(hl, "Just do your best");
        let Some(Block::Turn { hl, .. }) =
            clean(turn("You'll be fine.", "你没问题的。", "do your best"))
        else {
            panic!();
        };
        assert!(hl.is_empty());
    }

    #[test]
    fn alt_index_and_tag_rules() {
        let alt = |i: u8, tag: &str| Block::Alt {
            i,
            expr: "Give it your best shot".into(),
            zh: "尽力一试".into(),
            tag: tag.into(),
            tone: "鼓励、轻松".into(),
            diff: "强调尝试。".into(),
            ex_en: "You should give it your best shot.".into(),
            ex_zh: "你应该尽力一试。".into(),
        };
        assert!(clean(alt(0, "日常口语")).is_none());
        assert!(clean(alt(4, "日常口语")).is_none());
        assert!(
            matches!(clean(alt(2, "日常口语")), Some(Block::Alt { tag, .. }) if tag == "日常口语")
        );
        assert!(
            matches!(clean(alt(2, "非常非常口语化")), Some(Block::Alt { tag, .. }) if tag.is_empty())
        );
    }

    #[test]
    fn chinese_text_blocks_need_chinese() {
        assert!(
            clean(Block::Meaning {
                text: "尽你所能；尽力就好。".into()
            })
            .is_some()
        );
        assert!(
            clean(Block::Meaning {
                text: "Do your best".into()
            })
            .is_none()
        );
        assert!(clean(Block::Point { text: "  ".into() }).is_none());
    }

    #[test]
    fn level_forms() {
        assert!(is_level("B1") && is_level("A2-C1"));
        assert!(!is_level("B1-") && !is_level("B3") && !is_level(""));
    }
}
