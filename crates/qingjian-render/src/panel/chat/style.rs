//! AI 问答面板的版式常量与文字样式（点），量自 `docs/design/reference/strict_4_ai_chat.png`，偏差用 `tools/ui-diff` 对照修正。

use crate::color::Color;
use crate::panel::{PanelText, tokens};

/// 左侧栏宽度。
pub(super) const SIDEBAR_WIDTH: f32 = 168.0;

/// 侧栏标题行、主区顶栏的高度。
pub(super) const HEADER_HEIGHT: f32 = 32.0;

/// 侧栏会话行：高度、行距、第一行的位置。
pub(super) const ROW_HEIGHT: f32 = 38.0;
pub(super) const ROW_PITCH: f32 = 41.0;
pub(super) const ROW_TOP: f32 = 59.0;

/// 侧栏底部「设置」行高度（含上方分割线）。
pub(super) const SETTINGS_HEIGHT: f32 = 43.0;

/// 消息区内容离主区左边的距离。
pub(super) const CONTENT_INSET: f32 = 25.0;

/// 输入区：左右下边距、最小高度、行高、发送键边长、最多显示行数。
pub(super) const COMPOSER_SIDE: f32 = 11.0;
pub(super) const COMPOSER_BOTTOM: f32 = 8.0;
pub(super) const INPUT_MIN_HEIGHT: f32 = 35.0;
pub(super) const INPUT_LINE_HEIGHT: f32 = 18.0;
pub(super) const INPUT_MAX_LINES: usize = 4;
pub(super) const SEND_SIZE: f32 = 36.0;

/// 气泡内边距与块间距。
pub(super) const BUBBLE_PAD_X: f32 = 14.0;
pub(super) const BUBBLE_PAD_Y: f32 = 9.0;
pub(super) const BLOCK_GAP: f32 = 6.0;
pub(super) const BUBBLE_GAP: f32 = 12.0;
pub(super) const BUBBLE_RADIUS: f32 = 12.0;

/// 侧栏底色。
pub(super) const SIDEBAR_BG: Color = Color::hex(0xF8FAFC);

/// 选中会话的描边、输入框描边、建议胶囊描边。
pub(super) const SELECTED_BORDER: Color = Color::hex(0xBFD5FB);
pub(super) const SOFT_BORDER: Color = Color::hex(0xD9E4F5);

/// 建议问题胶囊底色。
pub(super) const CHIP_BG: Color = Color::hex(0xF3F7FE);

/// 按钮底色（清空会话）。
pub(super) const PILL_BG: Color = Color::hex(0xF1F5F9);

/// 引用竖线、代码块底色、行内代码色。
pub(super) const QUOTE_BAR: Color = Color::hex(0xCBD5E1);
pub(super) const CODE_BG: Color = Color::hex(0xE9EEF6);
pub(super) const CODE_INLINE: Color = Color::hex(0xB45309);

/// 侧栏 logo 底色（与学习助手面板一致）。
pub(super) const LOGO: Color = Color::hex(0x0D9488);

/// 次级深灰。
pub(super) const TEXT_MID: Color = Color::hex(0x4B5563);

pub(super) const BODY: PanelText = PanelText::new(13.0, 19.0, tokens::TEXT);
pub(super) const CODE: PanelText = PanelText::new(12.0, 17.0, tokens::TEXT);
pub(super) const SMALL: PanelText = PanelText::new(12.0, 16.0, TEXT_MID);
pub(super) const SMALL_BOLD: PanelText = PanelText::new(12.0, 16.0, tokens::TEXT).bolded();
pub(super) const CAPTION: PanelText = PanelText::new(11.0, 15.0, tokens::TEXT_2);
pub(super) const SIDE_TEXT: PanelText = PanelText::new(10.5, 15.0, tokens::TEXT);
pub(super) const CHIP_TEXT: PanelText = PanelText::new(11.0, 15.0, TEXT_MID);
pub(super) const TINY: PanelText = PanelText::new(10.0, 14.0, tokens::TEXT_2);
pub(super) const INPUT_TEXT: PanelText = PanelText::new(13.0, INPUT_LINE_HEIGHT, tokens::TEXT);
pub(super) const BETA: PanelText = PanelText::new(9.0, 12.0, tokens::TAG_BLUE.fg).bolded();
