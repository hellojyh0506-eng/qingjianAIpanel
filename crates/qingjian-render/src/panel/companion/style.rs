//! 学习助手面板的版式常量与文字样式（点），量自 `docs/design/reference/strict_*.png`，偏差用 `tools/ui-diff` 对照修正。

use crate::color::Color;
use crate::panel::{Area, PanelText, tokens};

/// 窗口高度（固定）。
pub(super) const WINDOW_HEIGHT: f32 = 560.0;

/// 标题栏高度；收起时窗口就只剩它。
pub(super) const HEADER_HEIGHT: f32 = 36.0;

/// 英文大字与特点标签行、标签栏的位置。
pub(super) const HERO_ZH_Y: f32 = 42.0;
pub(super) const HERO_EN_Y: f32 = 59.0;
pub(super) const CHIP_ROW_Y: f32 = 91.0;
pub(super) const TAB_BAR_Y: f32 = 123.0;
pub(super) const TAB_BAR_HEIGHT: f32 = 31.0;

/// 可滚动内容区（视口）：上沿到底部分割线。
pub(super) const VIEWPORT_TOP: f32 = 159.0;
pub(super) const FOOTER_Y: f32 = 506.0;

/// 底部按钮。
pub(super) const BUTTON_Y: f32 = 517.0;
pub(super) const BUTTON_HEIGHT: f32 = 34.0;

/// 左右边距：标签栏、底部按钮 / 卡片。
pub(super) const MARGIN_BAR: f32 = 9.5;
pub(super) const MARGIN_CARD: f32 = 12.0;

/// 视口里第一块内容离视口上沿的距离。
pub(super) const CONTENT_GAP: f32 = 5.5;

/// 卡片之间的间距。
pub(super) const CARD_GAP: f32 = 6.0;

/// 青绿强调色：「更自然地说」标签选中时用（严格参考图里这一页是青绿的）。
pub(super) const TEAL: Color = Color::hex(0x0D9488);
pub(super) const TEAL_BG: Color = Color::hex(0xE6F7F5);

/// 深蓝小标题（「自然例句」）。
pub(super) const NAVY: Color = Color::hex(0x1E3A8A);

/// 正文里的次级深灰（语气 / 区别说明）。
pub(super) const TEXT_MID: Color = Color::hex(0x4B5563);

/// 骨架条底色。
pub(super) const SKELETON: Color = Color::hex(0xE5EAF1);

pub(super) const BODY: PanelText = PanelText::new(13.0, 18.0, tokens::TEXT);
pub(super) const BODY_BOLD: PanelText = PanelText::new(13.0, 18.0, tokens::TEXT).bolded();
pub(super) const SMALL: PanelText = PanelText::new(12.0, 16.0, TEXT_MID);
pub(super) const SMALL_BOLD: PanelText = PanelText::new(12.0, 16.0, tokens::TEXT).bolded();
pub(super) const CAPTION: PanelText = PanelText::new(11.0, 15.0, tokens::TEXT_2);

/// 窗口宽度。
pub(super) const WIDTH: f32 = tokens::WINDOW_WIDTH;

/// 卡片横向范围。
pub(super) fn card_area(y: f32, height: f32) -> Area {
    Area::new(MARGIN_CARD, y, WIDTH - 2.0 * MARGIN_CARD, height)
}
