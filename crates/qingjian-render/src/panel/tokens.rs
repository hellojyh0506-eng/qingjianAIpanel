//! 面板设计 token：色值、圆角、窗口宽度，取自 `docs/plan/ai-panel-rebuild.md` P7 的 token 表（标签与卡片色从《严格参考图》取样）。
//!
//! 尺寸单位是点（逻辑像素）；颜色一律不透明。面板里不要再写散落的色值，都从这里取。

use super::color_pair::ColorPair;
use crate::color::Color;

/// 面板窗口宽度。
pub const WINDOW_WIDTH: f32 = 420.0;

/// 窗口圆角。
pub const WINDOW_RADIUS: f32 = 12.0;

/// 卡片圆角。
pub const RADIUS_CARD: f32 = 10.0;

/// 标签与按钮圆角。
pub const RADIUS_BUTTON: f32 = 8.0;

/// 胶囊（等级、语气小标签）圆角。
pub const RADIUS_CHIP: f32 = 6.0;

/// 窗口底色。
pub const WINDOW_BG: Color = Color::hex(0xFFFFFF);

/// 主色：选中标签文字、主按钮。
pub const PRIMARY: Color = Color::hex(0x2563EB);

/// 选中标签背景（边框用 [`PRIMARY`] 1 px）。
pub const SELECTED_BG: Color = Color::hex(0xEBF2FF);

/// 未选中标签所在分段条的底色。
pub const TABBAR_BG: Color = Color::hex(0xEEF2F7);

/// 通用卡片底色。
pub const CARD_BG: Color = Color::hex(0xF8FAFC);

/// 「意思」卡片底色。
pub const CARD_BLUE: Color = Color::hex(0xF3F7FE);

/// 「使用场景」卡片底色。
pub const CARD_PURPLE: Color = Color::hex(0xF9F7FE);

/// 边框与分割线。
pub const BORDER: Color = Color::hex(0xE5E7EB);

/// 主文字。
pub const TEXT: Color = Color::hex(0x111827);

/// 次文字、中文翻译。
pub const TEXT_2: Color = Color::hex(0x6B7280);

/// 自然口语、更有力度。
pub const TAG_ORANGE: ColorPair = ColorPair::new(Color::hex(0xFEF3C7), Color::hex(0xB45309));

/// 鼓励支持、常用表达、日常口语。
pub const TAG_GREEN: ColorPair = ColorPair::new(Color::hex(0xDCFCE7), Color::hex(0x15803D));

/// 温和安慰、等级胶囊。
pub const TAG_BLUE: ColorPair = ColorPair::new(Color::hex(0xDBEAFE), Color::hex(0x1D4ED8));

/// 区块图标底：蓝。
pub const ICON_CHIP_BLUE: Color = Color::hex(0xDBEAFE);

/// 区块图标底：绿。
pub const ICON_CHIP_GREEN: Color = Color::hex(0xDCFCE7);

/// 区块图标底：紫。
pub const ICON_CHIP_PURPLE: Color = Color::hex(0xEDE9FE);

/// 区块标题字色：意思。
pub const TITLE_BLUE: Color = Color::hex(0x2563EB);

/// 区块标题字色：语气。
pub const TITLE_GREEN: Color = Color::hex(0x16A34A);

/// 区块标题字色：使用场景。
pub const TITLE_PURPLE: Color = Color::hex(0x7C3AED);

/// 替换表达序号圆点：1。
pub const BADGE_1: Color = Color::hex(0x2563EB);

/// 替换表达序号圆点：2。
pub const BADGE_2: Color = Color::hex(0x14B8A6);

/// 替换表达序号圆点：3。
pub const BADGE_3: Color = Color::hex(0xF59E0B);

/// 错误状态底色。
pub const ERROR_BG: Color = Color::hex(0xFEF2F2);

/// 错误状态边框。
pub const ERROR_BORDER: Color = Color::hex(0xFECACA);

/// 错误状态文字。
pub const ERROR_FG: Color = Color::hex(0xDC2626);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_plan_table() {
        assert_eq!(PRIMARY, Color::rgb(0x25, 0x63, 0xEB));
        assert_eq!(SELECTED_BG, Color::rgb(0xEB, 0xF2, 0xFF));
        assert_eq!(TABBAR_BG, Color::rgb(0xEE, 0xF2, 0xF7));
        assert_eq!(CARD_BG, Color::rgb(0xF8, 0xFA, 0xFC));
        assert_eq!(
            (CARD_BLUE, CARD_PURPLE),
            (Color::hex(0xF3F7FE), Color::hex(0xF9F7FE))
        );
        assert_eq!(
            (BORDER, TEXT, TEXT_2),
            (
                Color::hex(0xE5E7EB),
                Color::hex(0x111827),
                Color::hex(0x6B7280)
            )
        );
        assert_eq!(
            (TAG_ORANGE.bg, TAG_ORANGE.fg),
            (Color::hex(0xFEF3C7), Color::hex(0xB45309))
        );
        assert_eq!(
            (TAG_GREEN.bg, TAG_GREEN.fg),
            (Color::hex(0xDCFCE7), Color::hex(0x15803D))
        );
        assert_eq!(
            (TAG_BLUE.bg, TAG_BLUE.fg),
            (Color::hex(0xDBEAFE), Color::hex(0x1D4ED8))
        );
        assert_eq!(
            (ICON_CHIP_BLUE, ICON_CHIP_GREEN, ICON_CHIP_PURPLE),
            (
                Color::hex(0xDBEAFE),
                Color::hex(0xDCFCE7),
                Color::hex(0xEDE9FE)
            )
        );
        assert_eq!(
            (TITLE_BLUE, TITLE_GREEN, TITLE_PURPLE),
            (
                Color::hex(0x2563EB),
                Color::hex(0x16A34A),
                Color::hex(0x7C3AED)
            )
        );
        assert_eq!(
            (BADGE_1, BADGE_2, BADGE_3),
            (
                Color::hex(0x2563EB),
                Color::hex(0x14B8A6),
                Color::hex(0xF59E0B)
            )
        );
        assert_eq!(
            (ERROR_BG, ERROR_BORDER, ERROR_FG),
            (
                Color::hex(0xFEF2F2),
                Color::hex(0xFECACA),
                Color::hex(0xDC2626)
            )
        );
        assert_eq!((RADIUS_CARD, RADIUS_BUTTON, RADIUS_CHIP), (10.0, 8.0, 6.0));
        assert_eq!(WINDOW_WIDTH, 420.0);
    }
}
