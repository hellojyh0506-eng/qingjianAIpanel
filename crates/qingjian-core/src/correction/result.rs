use crate::engine::{MarkedKind, MarkedSegment};
use crate::parser::Segmentation;

use super::edit::Edit;

/// 一次拼写纠正：用户敲的串、纠正后的串、那一处编辑，以及纠正后串的完整音节切分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    /// 用户敲的（作用域里的拼音，不含 `'`）。
    pub original: String,

    /// 纠正后的拼音。
    pub corrected: String,

    /// 从原串到纠正后串的那一处编辑。
    pub edit: Edit,

    /// 纠正后串的切分：每个音节都完整，或（相邻换位时）只有末尾一个还没敲完。
    pub segmentation: Segmentation,
}

impl Correction {
    /// 返回本次纠错覆盖到的音节级错拼对；同一音节有两处编辑时只记一次。
    pub fn typo_pairs(&self, consumed: usize) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        for index in 0..self.edit.components().len() {
            if let Some(pair) = self.typo_pair_for_edit(index, consumed)
                && !pairs.contains(&pair)
            {
                pairs.push(pair);
            }
        }
        pairs
    }

    /// 兼容旧的单编辑调用：返回第一处编辑对应的错拼对。
    pub fn typo_pair(&self, consumed: usize) -> Option<(String, String)> {
        self.typo_pair_for_edit(0, consumed)
    }

    /// 取得复合编辑中某一步对应的错拼对。每一步的编辑下标先映射到最终拼音，
    /// 音节边界则逆序映射回原串。
    pub(crate) fn typo_pair_for_edit(
        &self,
        edit_index: usize,
        consumed: usize,
    ) -> Option<(String, String)> {
        let edits = self.edit.components();
        let edit = *edits.get(edit_index)?;
        let mut at = match edit {
            Edit::Substitute { index, .. }
            | Edit::Insert { index }
            | Edit::Transpose { index, .. } => *index,
            Edit::Delete { index, .. } => index.saturating_sub(1),
            Edit::Multiple { .. } => return None,
        };
        for later in edits.iter().skip(edit_index + 1) {
            at = map_position_through_edit(at, later);
        }
        if at >= consumed {
            return None;
        }

        let mut start = 0;
        for syllable in &self.segmentation.syllables {
            let end = start + syllable.text.len();
            if at < end {
                if !syllable.complete {
                    return None;
                }
                let typed_start = self.edit.to_original(start);
                let typed_end = self.edit.to_original(end);
                let typed = self.original.get(typed_start..typed_end)?;
                return (!typed.is_empty() && typed != syllable.text)
                    .then(|| (typed.to_owned(), syllable.text.clone()));
            }
            start = end;
        }
        None
    }

    /// preedit 的分段：全部改动都映射到最终切分，旧字母以 Corrected 样式显示在原位置。
    pub fn marked_segments(&self) -> Vec<MarkedSegment> {
        let display = self.segmentation.joined("'");
        let edits = self.edit.components();
        let mut markers = Vec::new();
        for (index, edit) in edits.iter().enumerate() {
            if let Some((mut at, struck)) = edit.struck() {
                for later in edits.iter().skip(index + 1) {
                    at = map_position_through_edit(at, later);
                }
                markers.push((at, struck));
            }
        }
        markers.sort_by_key(|(at, _)| *at);

        let mut segments = Vec::with_capacity(markers.len() * 2 + 1);
        let mut cursor = 0;
        for (at, struck) in markers {
            let split = display_byte_for_letter(&display, at);
            if split < cursor {
                continue;
            }
            if split > cursor {
                segments.push(MarkedSegment::new(
                    &display[cursor..split],
                    MarkedKind::Typed,
                ));
            }
            segments.push(MarkedSegment::new(struck, MarkedKind::Corrected));
            cursor = split;
        }
        if cursor < display.len() {
            segments.push(MarkedSegment::new(&display[cursor..], MarkedKind::Typed));
        }
        if segments.is_empty() {
            segments.push(MarkedSegment::new(display, MarkedKind::Typed));
        }
        segments
    }
}

fn map_position_through_edit(position: usize, edit: &Edit) -> usize {
    match edit {
        Edit::Insert { index } if *index <= position => position.saturating_add(1),
        Edit::Delete { index, .. } if *index < position => position.saturating_sub(1),
        _ => position,
    }
}

fn display_byte_for_letter(display: &str, target: usize) -> usize {
    let mut letters = 0;
    for (index, ch) in display.char_indices() {
        if ch == '\'' {
            continue;
        }
        if letters == target {
            return index;
        }
        letters += 1;
    }
    display.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Syllable;

    fn segmentation(syllables: &[&str]) -> Segmentation {
        Segmentation {
            syllables: syllables.iter().map(|s| Syllable::complete(s)).collect(),
        }
    }

    fn texts(segments: &[MarkedSegment]) -> Vec<(String, MarkedKind)> {
        segments.iter().map(|s| (s.text.clone(), s.kind)).collect()
    }

    #[test]
    fn strikes_the_replaced_letter_in_place() {
        let correction = Correction {
            original: "nihooma".into(),
            corrected: "nihaoma".into(),
            edit: Edit::Substitute {
                index: 3,
                from: 'o',
            },
            segmentation: segmentation(&["ni", "hao", "ma"]),
        };
        assert_eq!(
            texts(&correction.marked_segments()),
            [
                ("ni'h".to_owned(), MarkedKind::Typed),
                ("o".to_owned(), MarkedKind::Corrected),
                ("ao'ma".to_owned(), MarkedKind::Typed),
            ]
        );
    }

    #[test]
    fn typo_pair_is_the_syllable_around_the_edit() {
        let correction = Correction {
            original: "nihooma".into(),
            corrected: "nihaoma".into(),
            edit: Edit::Substitute {
                index: 3,
                from: 'o',
            },
            segmentation: segmentation(&["ni", "hao", "ma"]),
        };
        assert_eq!(
            correction.typo_pair(7),
            Some(("hoo".to_owned(), "hao".to_owned()))
        );
        // 只吃到 ni 就上屏：没接受纠正
        assert_eq!(correction.typo_pair(2), None);
        let correction = Correction {
            original: "meiganxi".into(),
            corrected: "meiguanxi".into(),
            edit: Edit::Insert { index: 4 },
            segmentation: segmentation(&["mei", "guan", "xi"]),
        };
        assert_eq!(
            correction.typo_pair(9),
            Some(("gan".to_owned(), "guan".to_owned()))
        );
        let correction = Correction {
            original: "zhegge".into(),
            corrected: "zhege".into(),
            edit: Edit::Delete {
                index: 3,
                removed: 'g',
            },
            segmentation: segmentation(&["zhe", "ge"]),
        };
        // 多敲的 g 算在前一个音节上
        assert_eq!(
            correction.typo_pair(5),
            Some(("zheg".to_owned(), "zhe".to_owned()))
        );
        // 换位落在没敲完的末尾音节里：不记
        let correction = Correction {
            original: "mingita".into(),
            corrected: "mingtia".into(),
            edit: Edit::Transpose {
                index: 4,
                first: 'i',
                second: 't',
            },
            segmentation: Segmentation {
                syllables: vec![Syllable::complete("ming"), Syllable::partial("tia")],
            },
        };
        assert_eq!(correction.typo_pair(7), None);
    }

    #[test]
    fn extra_letter_at_the_end_and_missing_letter() {
        let correction = Correction {
            original: "nihaox".into(),
            corrected: "nihao".into(),
            edit: Edit::Delete {
                index: 5,
                removed: 'x',
            },
            segmentation: segmentation(&["ni", "hao"]),
        };
        assert_eq!(
            texts(&correction.marked_segments()),
            [
                ("ni'hao".to_owned(), MarkedKind::Typed),
                ("x".to_owned(), MarkedKind::Corrected),
            ]
        );
        let correction = Correction {
            original: "nhao".into(),
            corrected: "nihao".into(),
            edit: Edit::Insert { index: 1 },
            segmentation: segmentation(&["ni", "hao"]),
        };
        assert_eq!(
            texts(&correction.marked_segments()),
            [("ni'hao".to_owned(), MarkedKind::Typed)]
        );
    }
}
