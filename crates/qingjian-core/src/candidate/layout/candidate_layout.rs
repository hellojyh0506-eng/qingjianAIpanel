use std::collections::HashMap;

use super::super::{Candidate, CandidateKind};
use super::Cell;

/// 本地候选 + 云端词的分页排布。索引空间是「格」：第一页先本地后云端，之后各页全是本地候选。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateLayout {
    /// 本地候选，顺序就是 Engine 排好的顺序。
    local: Vec<Candidate>,

    /// 已到的云端词（≤ 槽位数，与本地候选去过重）。
    cloud: Vec<Candidate>,

    /// 每页几格。
    page_size: usize,

    /// 配置的云端槽位数；0 表示不要云端词。
    slots: usize,
}

impl CandidateLayout {
    pub fn new(local: Vec<Candidate>, page_size: usize, slots: usize) -> Self {
        Self {
            local,
            cloud: Vec::new(),
            page_size: page_size.max(1),
            slots,
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn local(&self) -> &[Candidate] {
        &self.local
    }

    pub fn cloud(&self) -> &[Candidate] {
        &self.cloud
    }

    /// 第一页最多给云端几格：本地候选至少占第一格；没有本地候选（问字模式）时整页都给云端。
    pub fn capacity(&self) -> usize {
        if self.local.is_empty() {
            self.page_size
        } else {
            let fixed = self
                .local
                .iter()
                .filter_map(|c| match c.kind {
                    CandidateKind::Custom(n) => Some(n),
                    _ => None,
                })
                .max()
                .unwrap_or(1);
            self.slots.min(self.page_size.saturating_sub(fixed.max(1)))
        }
    }

    /// 云端词到了：与本地候选同文的不要（本地已经能给），其余按顺序填进第一页末尾，多出来的丢掉。返回填进去的条数。
    pub fn set_cloud(&mut self, words: Vec<Candidate>) -> usize {
        self.cloud.clear();
        let capacity = self.capacity();
        for mut word in words {
            if self.cloud.len() >= capacity
                || self.local.iter().any(|c| c.text == word.text)
                || self.cloud.iter().any(|c| c.text == word.text)
            {
                continue;
            }
            word.kind = CandidateKind::Cloud;
            self.cloud.push(word);
        }
        self.cloud.len()
    }

    /// 本地格数包含最大固定位置之前的空格。
    fn local_len(&self) -> usize {
        self.local
            .iter()
            .filter_map(|c| match c.kind {
                CandidateKind::Custom(position) => Some(position),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .max(self.local.len())
    }

    /// 真实本地候选按固定位置放置，其余候选依次填入空格。
    fn local_cells(&self) -> Vec<Cell<'_>> {
        let mut cells = vec![Cell::Empty; self.local_len()];
        for candidate in &self.local {
            if let CandidateKind::Custom(position) = candidate.kind
                && let Some(cell) = position.checked_sub(1).and_then(|i| cells.get_mut(i))
            {
                *cell = Cell::Local(candidate);
            }
        }
        let mut normal = self
            .local
            .iter()
            .filter(|c| !matches!(c.kind, CandidateKind::Custom(_)));
        for cell in &mut cells {
            if matches!(cell, Cell::Empty)
                && let Some(candidate) = normal.next()
            {
                *cell = Cell::Local(candidate);
            }
        }
        cells
    }

    /// 全部格子按索引顺序排开；空位只属于布局。
    pub fn cells(&self) -> Vec<Cell<'_>> {
        let mut cells = self.local_cells();
        let first = cells.len().min(self.page_size - self.cloud.len());
        cells.splice(first..first, self.cloud.iter().map(Cell::Cloud));
        cells
    }

    /// 只在现有本地候选原本所在的格子间重排。未被模型评价的候选、英文候选和 Custom 固定位置不移动；
    /// 排序内容必须来自候选列表自身，不会创建、删除或改写候选。
    pub fn reorder_local_by_text(&mut self, order: &[String]) -> bool {
        if order.len() < 2 {
            return false;
        }
        let ranks: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(index, text)| (text.as_str(), index))
            .collect();
        let positions: Vec<usize> = self
            .local
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (!matches!(candidate.kind, CandidateKind::Custom(_))
                    && ranks.contains_key(candidate.text.as_str()))
                .then_some(index)
            })
            .collect();
        let ranked: Vec<Candidate> = order
            .iter()
            .filter_map(|text| {
                self.local
                    .iter()
                    .find(|candidate| {
                        candidate.text == *text
                            && !matches!(candidate.kind, CandidateKind::Custom(_))
                    })
                    .cloned()
            })
            .collect();
        if positions.len() != ranked.len() || ranked.len() < 2 {
            return false;
        }
        let before: Vec<String> = positions
            .iter()
            .map(|index| self.local[*index].text.clone())
            .collect();
        for (position, candidate) in positions.iter().copied().zip(ranked) {
            self.local[position] = candidate;
        }
        let after: Vec<String> = positions
            .iter()
            .map(|index| self.local[*index].text.clone())
            .collect();
        after != before
    }

    pub fn len(&self) -> usize {
        self.local_len() + self.cloud.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pages(&self) -> usize {
        self.len().div_ceil(self.page_size)
    }

    /// 第 `index` 格的候选；越界返回 `None`。
    pub fn candidate(&self, index: usize) -> Option<&Candidate> {
        self.cells().get(index).and_then(|cell| cell.candidate())
    }

    /// 第 `page` 页的格子。
    pub fn page(&self, page: usize) -> Vec<Cell<'_>> {
        self.cells()
            .into_iter()
            .skip(page * self.page_size)
            .take(self.page_size)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> Candidate {
        Candidate {
            text: text.into(),
            kind: CandidateKind::Chinese,
            syllables: vec!["zhang".into(), "tao".into()],
            reading: None,
            translation: None,
            aux_code: None,
        }
    }

    fn cloud(text: &str) -> Candidate {
        Candidate {
            kind: CandidateKind::Cloud,
            ..local(text)
        }
    }

    fn texts(cells: &[Cell<'_>]) -> Vec<String> {
        cells
            .iter()
            .map(|cell| match cell {
                Cell::Local(c) => c.text.clone(),
                Cell::Cloud(c) => format!("☁{}", c.text),
                Cell::Empty => "<empty>".into(),
            })
            .collect()
    }

    fn many(count: usize) -> Vec<Candidate> {
        (0..count).map(|i| local(&format!("本{i}"))).collect()
    }

    #[test]
    fn rerank_reorders_only_existing_local_candidates_and_keeps_unlisted_order() {
        let mut layout = CandidateLayout::new(
            vec![local("我"), local("卧"), local("握"), local("窝"), local("沃")],
            3,
            0,
        );
        assert!(layout.reorder_local_by_text(&["窝".into(), "我".into(), "握".into()]));
        assert_eq!(
            layout.local().iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["窝", "卧", "我", "握", "沃"]
        );
        assert!(!layout.reorder_local_by_text(&["不存在".into(), "也不存在".into()]));
        assert_eq!(layout.local().len(), 5);
    }

    #[test]
    fn rerank_can_promote_an_existing_candidate_from_a_later_page() {
        let candidates = (0..30)
            .map(|i| local(&format!("词{i}")))
            .collect::<Vec<_>>();
        let mut layout = CandidateLayout::new(candidates, 9, 0);
        let order = (10..27)
            .chain(0..10)
            .map(|i| format!("词{i}"))
            .collect::<Vec<_>>();
        assert!(layout.reorder_local_by_text(&order));
        assert_eq!(layout.local()[0].text, "词10");
        assert_eq!(layout.local()[8].text, "词18");
        assert_eq!(layout.local()[9].text, "词19");
        assert_eq!(layout.local().len(), 30);
    }

    #[test]
    fn rerank_keeps_custom_slots_pinned_while_promoting_from_a_later_page() {
        let mut candidates = many(30);
        candidates.push(Candidate {
            kind: CandidateKind::Custom(10),
            ..local("固定短语")
        });
        let mut layout = CandidateLayout::new(candidates, 9, 0);
        let order = (10..27)
            .chain(0..10)
            .map(|i| format!("本{i}"))
            .collect::<Vec<_>>();

        assert!(layout.reorder_local_by_text(&order));
        assert_eq!(layout.candidate(0).unwrap().text, "本10");
        assert_eq!(layout.candidate(9).unwrap().text, "固定短语");
        assert_eq!(layout.local().len(), 31);
        assert_eq!(
            layout.local().iter().filter(|c| matches!(c.kind, CandidateKind::Custom(10))).count(),
            1
        );
    }

    #[test]
    fn cloud_words_take_the_end_of_the_first_page_and_only_bump_those_cells() {
        let mut layout = CandidateLayout::new(many(12), 9, 3);
        assert_eq!(
            texts(&layout.page(0)),
            [
                "本0", "本1", "本2", "本3", "本4", "本5", "本6", "本7", "本8"
            ]
        );
        assert_eq!(layout.pages(), 2);

        // 四条云端词只取前三条；前六格不动，原来 7–9 格的本地候选挪到第二页开头
        let filled = layout.set_cloud(vec![cloud("云0"), cloud("云1"), cloud("云2"), cloud("云3")]);
        assert_eq!(filled, 3);
        assert_eq!(
            texts(&layout.page(0)),
            [
                "本0", "本1", "本2", "本3", "本4", "本5", "☁云0", "☁云1", "☁云2"
            ]
        );
        assert_eq!(
            texts(&layout.page(1)),
            ["本6", "本7", "本8", "本9", "本10", "本11"]
        );
        assert_eq!(layout.candidate(6).unwrap().text, "云0");
        assert_eq!(layout.candidate(9).unwrap().text, "本6");
    }

    #[test]
    fn duplicates_of_local_candidates_are_dropped_and_fewer_words_take_fewer_cells() {
        let mut layout = CandidateLayout::new(many(4), 9, 2);
        assert_eq!(layout.set_cloud(vec![cloud("本2"), cloud("云0")]), 1);
        assert_eq!(texts(&layout.page(0)), ["本0", "本1", "本2", "本3", "☁云0"]);
        assert_eq!(layout.local()[2].kind, CandidateKind::Chinese);
        assert_eq!(layout.pages(), 1);
    }

    #[test]
    fn no_words_means_nothing_changes() {
        let mut layout = CandidateLayout::new(many(12), 9, 2);
        assert_eq!(layout.set_cloud(Vec::new()), 0);
        assert_eq!(layout.page(0).len(), 9);
        assert_eq!(texts(&layout.page(1)), ["本9", "本10", "本11"]);
    }

    #[test]
    fn zero_slots_means_no_cloud_cells_at_all() {
        let mut layout = CandidateLayout::new(many(3), 9, 0);
        assert_eq!(layout.set_cloud(vec![cloud("云0")]), 0);
        assert_eq!(texts(&layout.page(0)), ["本0", "本1", "本2"]);
    }

    #[test]
    fn without_local_candidates_the_whole_page_goes_to_the_cloud() {
        let mut layout = CandidateLayout::new(Vec::new(), 9, 2);
        assert_eq!(
            layout.set_cloud(vec![cloud("森"), cloud("淼"), cloud("焱")]),
            3
        );
        assert_eq!(texts(&layout.page(0)), ["☁森", "☁淼", "☁焱"]);
        assert_eq!(layout.candidate(0).unwrap().text, "森");
    }

    #[test]
    fn slots_never_push_the_first_local_candidate_off_the_first_page() {
        let mut layout = CandidateLayout::new(many(5), 3, 5);
        assert_eq!(layout.capacity(), 2);
        layout.set_cloud(vec![cloud("云0"), cloud("云1"), cloud("云2")]);
        assert_eq!(texts(&layout.page(0)), ["本0", "☁云0", "☁云1"]);
        assert_eq!(texts(&layout.page(1)), ["本1", "本2", "本3"]);
    }

    #[test]
    fn sparse_custom_positions_are_layout_cells_not_candidates() {
        let fixed = Candidate {
            kind: CandidateKind::Custom(3),
            ..local("短语")
        };
        let mut layout = CandidateLayout::new(vec![fixed, local("普通")], 5, 2);
        assert_eq!(layout.local().len(), 2);
        assert_eq!(texts(&layout.page(0)), ["普通", "<empty>", "短语"]);
        assert!(layout.candidate(1).is_none());
        assert_eq!(layout.set_cloud(vec![cloud("云")]), 1);
        assert_eq!(texts(&layout.page(0)), ["普通", "<empty>", "短语", "☁云"]);
    }

    #[test]
    fn ninth_position_stays_on_second_page_without_cloud_displacement() {
        let mut layout = CandidateLayout::new(
            vec![Candidate {
                kind: CandidateKind::Custom(9),
                ..local("第九")
            }],
            5,
            2,
        );
        assert_eq!(layout.pages(), 2);
        assert_eq!(layout.capacity(), 0);
        assert_eq!(layout.set_cloud(vec![cloud("云")]), 0);
        assert_eq!(layout.candidate(8).unwrap().text, "第九");
        assert!(layout.candidate(7).is_none());
    }
}
