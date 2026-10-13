//! Lucide 图标：24 × 24 视口的描边 path，运行时解析成 `tiny_skia::Path` 后按目标尺寸与统一线宽描边。
//!
//! 图标数据在 `data.rs`（脚本生成），来源与许可见同目录的 `LICENSE-lucide`。

mod data;
mod svg_path;

use tiny_skia::{Path, PathBuilder};

pub use data::Icon;

/// Lucide 图标的设计视口边长。
pub(super) const VIEWBOX: f32 = 24.0;

impl Icon {
    /// 把组成图标的所有笔画并成一条 path（24 × 24 坐标系）；解析失败返回 `None`。
    pub(super) fn path(self) -> Option<Path> {
        let mut builder = PathBuilder::new();
        for d in self.paths() {
            svg_path::append(&mut builder, d)?;
        }
        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses_and_stays_inside_the_viewbox() {
        for &icon in Icon::ALL {
            let path = icon
                .path()
                .unwrap_or_else(|| panic!("图标 {} 解析失败", icon.name()));
            let b = path.bounds();
            assert!(
                b.left() >= -0.5
                    && b.top() >= -0.5
                    && b.right() <= VIEWBOX + 0.5
                    && b.bottom() <= VIEWBOX + 0.5,
                "{} 越出视口：{b:?}",
                icon.name()
            );
            assert!(b.width().max(b.height()) > 1.0, "{} 太小", icon.name());
        }
    }

    #[test]
    fn plan_icons_are_all_present() {
        let names: Vec<&str> = Icon::ALL.iter().map(|icon| icon.name()).collect();
        for needed in [
            "pin",
            "minus",
            "x",
            "volume-2",
            "copy",
            "star",
            "mic",
            "message-square",
            "sparkles",
            "file-text",
            "scan-text",
            "target",
            "lightbulb",
            "chevron-down",
            "chevron-right",
            "plus",
            "settings",
            "paperclip",
            "image",
            "send",
            "refresh-cw",
            "circle-alert",
            "loader-circle",
            "circle-check",
            "ellipsis",
            "book-plus",
            "message-circle",
        ] {
            assert!(names.contains(&needed), "缺图标 {needed}");
        }
    }
}
