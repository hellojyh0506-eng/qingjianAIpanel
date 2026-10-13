//! 流式 JSONL 解析：模型一边吐字一边按行切，每凑齐一行就解析、校验、限数，产出合格的区块。
//!
//! 坏行（不是 JSON、种类不对、校验不过）与超出数量的多余行直接丢弃并计数，不影响后面的行。

use std::collections::HashMap;

use super::block::Block;
use super::tab::Tab;
use super::validate::clean;

/// 解析过程的计数，日志与回归测试用。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// 产出的合格区块数。
    pub accepted: usize,

    /// 解析或校验失败、种类不属于本标签的行数。
    pub bad: usize,

    /// 合格但超出该种类数量上限的行数。
    pub extra: usize,
}

pub struct BlockParser {
    tab: Tab,

    /// 还没凑成整行的尾巴。
    pending: String,

    /// 各种类已产出的个数。
    counts: HashMap<&'static str, usize>,

    stats: ParseStats,
}

impl BlockParser {
    pub fn new(tab: Tab) -> Self {
        Self {
            tab,
            pending: String::new(),
            counts: HashMap::new(),
            stats: ParseStats::default(),
        }
    }

    /// 喂一段新到的文字，返回这一段里新凑齐的合格区块。
    pub fn push(&mut self, chunk: &str) -> Vec<Block> {
        self.pending.push_str(chunk);
        let mut blocks = Vec::new();
        while let Some(end) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=end).collect();
            blocks.extend(self.parse_line(&line));
        }
        blocks
    }

    /// 流结束：把没有换行结尾的最后一行也解析掉。
    pub fn finish(&mut self) -> Vec<Block> {
        let line = std::mem::take(&mut self.pending);
        self.parse_line(&line).into_iter().collect()
    }

    pub fn stats(&self) -> ParseStats {
        self.stats
    }

    fn parse_line(&mut self, line: &str) -> Option<Block> {
        let line = line.trim();
        // 空行与代码围栏不算坏行
        if line.is_empty() || line.starts_with("```") {
            return None;
        }
        let Some(block) = parse_object(line).and_then(clean) else {
            self.stats.bad += 1;
            return None;
        };
        if block.tab() != self.tab {
            self.stats.bad += 1;
            return None;
        }
        let count = self.counts.entry(block.kind()).or_insert(0);
        if *count >= limit(self.tab, &block) {
            self.stats.extra += 1;
            return None;
        }
        *count += 1;
        self.stats.accepted += 1;
        Some(block)
    }
}

/// 取行里第一个 `{` 到最后一个 `}` 当 JSON 解析（容忍行首的项目符号、行尾的逗号）。
fn parse_object(line: &str) -> Option<Block> {
    let start = line.find('{')?;
    let end = line.rfind('}')?;
    serde_json::from_str(line.get(start..=end)?).ok()
}

/// 该种类在本标签里最多要几个。
fn limit(tab: Tab, block: &Block) -> usize {
    match (tab, block) {
        (Tab::Nuance, Block::Detail { .. }) => 3,
        (Tab::Alternatives, Block::Alt { .. }) => 3,
        (Tab::Dialogue, Block::Turn { .. }) => 3,
        (Tab::Dialogue, Block::Point { .. }) => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(blocks: &[Block]) -> Vec<&'static str> {
        blocks.iter().map(Block::kind).collect()
    }

    #[test]
    fn lines_split_across_chunks_are_reassembled() {
        let mut parser = BlockParser::new(Tab::Nuance);
        let mut out = parser.push("{\"k\":\"meaning\",\"te");
        assert!(out.is_empty());
        out.extend(parser.push("xt\":\"尽你所能；尽力就好。\"}\n{\"k\":\"tone\",\"text\":\"鼓励、支持。\"}\n{\"k\":\"usa"));
        assert_eq!(kinds(&out), ["meaning", "tone"]);
        out.extend(parser.push("ge\",\"text\":\"用于安慰别人。\"}"));
        assert_eq!(out.len(), 2, "没有换行结尾的最后一行要等 finish");
        out.extend(parser.finish());
        assert_eq!(kinds(&out), ["meaning", "tone", "usage"]);
        assert_eq!(
            parser.stats(),
            ParseStats {
                accepted: 3,
                bad: 0,
                extra: 0
            }
        );
    }

    #[test]
    fn bad_lines_are_dropped_and_counted_without_stopping_the_stream() {
        let mut parser = BlockParser::new(Tab::Nuance);
        let text = "```json\nnot json at all\n{\"k\":\"meaning\",\"text\":\"English only\"}\n{\"k\":\"point\",\"text\":\"属于别的标签\"}\n- {\"k\":\"tone\",\"text\":\"鼓励、支持。\"},\n```\n";
        let out = parser.push(text);
        assert_eq!(kinds(&out), ["tone"]);
        assert_eq!(parser.stats().bad, 3);
        assert_eq!(parser.stats().accepted, 1);
    }

    #[test]
    fn extra_blocks_beyond_the_limit_are_ignored() {
        let mut parser = BlockParser::new(Tab::Nuance);
        let example = "{\"k\":\"example\",\"en\":\"Just do your best.\",\"zh\":\"尽力就好。\",\"hl\":\"do your best\"}\n";
        let detail =
            "{\"k\":\"detail\",\"title\":\"搭配\",\"body\":\"do one's best 是固定搭配。\"}\n";
        let text = format!("{example}{example}{detail}{detail}{detail}{detail}");
        let out = parser.push(&text);
        assert_eq!(kinds(&out), ["example", "detail", "detail", "detail"]);
        assert_eq!(parser.stats().extra, 2);
    }

    #[test]
    fn dialogue_keeps_three_turns_and_two_points() {
        let mut parser = BlockParser::new(Tab::Dialogue);
        let turn = |sp: &str| {
            format!(
                "{{\"k\":\"turn\",\"sp\":\"{sp}\",\"en\":\"Just do your best.\",\"zh\":\"尽力就好。\"}}\n"
            )
        };
        let point = "{\"k\":\"point\",\"text\":\"语气积极友好。\"}\n";
        let text = format!(
            "{{\"k\":\"scene\",\"title\":\"朋友聊天（鼓励安慰）\"}}\n{}{}{}{}{point}{point}{point}",
            turn("A"),
            turn("B"),
            turn("A"),
            turn("B")
        );
        let out = parser.push(&text);
        assert_eq!(
            kinds(&out),
            ["scene", "turn", "turn", "turn", "point", "point"]
        );
        assert_eq!(parser.stats().extra, 2);
    }
}
