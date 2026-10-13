//! 对话头像：A（男生，左）与 B（女生，右）两张圆形透明 PNG，内嵌进二进制，按 DPI 选 @1x / @2x / @3x。
//!
//! 素材已经是圆形、圆外透明、边缘抗锯齿，这里不再做圆形裁切，直接按 alpha 贴图。素材说明见 `assets/ui/avatars/README.md`。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Avatar {
    /// 角色 A，男生，对话里在左。
    Boy,

    /// 角色 B，女生，对话里在右。
    Girl,
}

/// 同一头像的几档分辨率（边长像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Resolution {
    X1,
    X2,
    X3,
    Full,
}

impl Resolution {
    /// 能盖住 `target_px`（目标边长，像素）的最小一档，都不够用原图。
    pub(super) fn for_target(target_px: f32) -> Self {
        match target_px {
            t if t <= 40.0 => Self::X1,
            t if t <= 80.0 => Self::X2,
            t if t <= 120.0 => Self::X3,
            _ => Self::Full,
        }
    }
}

impl Avatar {
    pub(super) fn png(self, resolution: Resolution) -> &'static [u8] {
        match (self, resolution) {
            (Self::Boy, Resolution::X1) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_a_boy.png")
            }
            (Self::Boy, Resolution::X2) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_a_boy@2x.png")
            }
            (Self::Boy, Resolution::X3) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_a_boy@3x.png")
            }
            (Self::Boy, Resolution::Full) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_a_boy_512.png")
            }
            (Self::Girl, Resolution::X1) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_b_girl.png")
            }
            (Self::Girl, Resolution::X2) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_b_girl@2x.png")
            }
            (Self::Girl, Resolution::X3) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_b_girl@3x.png")
            }
            (Self::Girl, Resolution::Full) => {
                include_bytes!("../../../../assets/ui/avatars/avatar_b_girl_512.png")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_smallest_sufficient_resolution() {
        assert_eq!(Resolution::for_target(32.0), Resolution::X1);
        assert_eq!(Resolution::for_target(40.0), Resolution::X1);
        assert_eq!(Resolution::for_target(64.0), Resolution::X2);
        assert_eq!(Resolution::for_target(96.0), Resolution::X3);
        assert_eq!(Resolution::for_target(200.0), Resolution::Full);
    }
}
