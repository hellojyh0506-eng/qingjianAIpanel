//! 点击区域表：绘制时同步登记「这块区域点下去是什么」，窗口过程只查表，不再另算一遍布局。
//!
//! 坐标与 [`PanelSurface`](super::PanelSurface) 一致，是相对窗口左上角的点；后登记的盖在先登记的上面。

use super::area::Area;

#[derive(Debug, Clone, PartialEq)]
pub struct HitMap<H> {
    entries: Vec<(Area, H)>,
}

impl<H> Default for HitMap<H> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<H> HitMap<H> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一块可点区域。
    pub fn push(&mut self, area: Area, hit: H) {
        self.entries.push((area, hit));
    }

    /// 点 `(x, y)` 下最上层的命中；没有返回 `None`。
    pub fn hit(&self, x: f32, y: f32) -> Option<&H> {
        self.entries
            .iter()
            .rev()
            .find(|(area, _)| area.contains(x, y))
            .map(|(_, hit)| hit)
    }

    /// 全部登记项，按登记顺序（先到后）。
    pub fn entries(&self) -> &[(Area, H)] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_registration_wins_and_misses_return_none() {
        let mut map = HitMap::new();
        map.push(Area::new(0.0, 0.0, 100.0, 100.0), "card");
        map.push(Area::new(10.0, 10.0, 20.0, 20.0), "button");
        assert_eq!(map.hit(15.0, 15.0), Some(&"button"));
        assert_eq!(map.hit(50.0, 50.0), Some(&"card"));
        assert_eq!(map.hit(150.0, 50.0), None);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn adjacent_areas_do_not_overlap() {
        let mut map = HitMap::new();
        map.push(Area::new(0.0, 0.0, 10.0, 10.0), 1);
        map.push(Area::new(10.0, 0.0, 10.0, 10.0), 2);
        assert_eq!(map.hit(9.99, 5.0), Some(&1));
        assert_eq!(map.hit(10.0, 5.0), Some(&2));
    }
}
