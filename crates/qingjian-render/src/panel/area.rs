//! 矩形区域（点）：面板布局与点击命中共用的几何单位。

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Area {
    pub x: f32,

    pub y: f32,

    pub width: f32,

    pub height: f32,
}

impl Area {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// 四边各向内收 `d`（负数为外扩），宽高不会收成负数。
    pub fn inset(&self, d: f32) -> Self {
        Self {
            x: self.x + d,
            y: self.y + d,
            width: (self.width - 2.0 * d).max(0.0),
            height: (self.height - 2.0 * d).max(0.0),
        }
    }

    /// 两个区域的交集；不相交返回 `None`。
    pub fn intersect(&self, other: Area) -> Option<Area> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > left && bottom > top).then(|| Area::new(left, top, right - left, bottom - top))
    }

    /// 点是否落在区域内：左、上边界算在内，右、下边界不算，相邻区域不会同时命中。
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_half_open() {
        let area = Area::new(10.0, 10.0, 20.0, 10.0);
        assert!(area.contains(10.0, 10.0));
        assert!(area.contains(29.9, 19.9));
        assert!(!area.contains(30.0, 15.0));
        assert!(!area.contains(15.0, 20.0));
    }

    #[test]
    fn intersect_clips_to_the_overlap() {
        let a = Area::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            a.intersect(Area::new(5.0, 5.0, 10.0, 10.0)),
            Some(Area::new(5.0, 5.0, 5.0, 5.0))
        );
        assert_eq!(a.intersect(Area::new(10.0, 0.0, 5.0, 5.0)), None);
    }

    #[test]
    fn inset_shrinks_and_never_goes_negative() {
        let area = Area::new(0.0, 0.0, 10.0, 4.0).inset(3.0);
        assert_eq!(area, Area::new(3.0, 3.0, 4.0, 0.0));
    }
}
