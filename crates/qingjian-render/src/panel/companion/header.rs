//! 标题栏：logo、名称、BETA、钉住 / 收起 / 关闭；空白处可拖动。

use super::hit::CompanionHit;
use super::style::{HEADER_HEIGHT, WIDTH};
use super::view::CompanionView;
use super::widgets::{Surface, icon_button};
use crate::panel::{Area, Icon, PanelText, tokens};

const NAME: PanelText = PanelText::new(12.0, 18.0, tokens::TEXT).bolded();
const BETA: PanelText = PanelText::new(9.0, 12.0, tokens::TAG_BLUE.fg).bolded();
const LOGO: crate::color::Color = crate::color::Color::hex(0x0D9488);

/// 窗口上沿的小按钮边长。
const BUTTON: f32 = 28.0;

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView) {
    s.hit(
        Area::new(0.0, 0.0, WIDTH, HEADER_HEIGHT),
        CompanionHit::Drag,
    );
    s.circle(26.0, HEADER_HEIGHT / 2.0, 8.0, LOGO);
    s.icon(
        Icon::Sparkles,
        21.5,
        HEADER_HEIGHT / 2.0 - 4.5,
        9.0,
        tokens::WINDOW_BG,
    );
    let name = "青简 · 英语学习助手";
    let width = s.text(40.0, 9.0, name, &NAME);
    let beta_x = 40.0 + width + 7.0;
    let beta_w = s.measure("BETA", &BETA) + 12.0;
    s.fill(
        Area::new(beta_x, 11.0, beta_w, 14.0),
        4.0,
        tokens::TAG_BLUE.bg,
    );
    s.text(beta_x + 6.0, 12.0, "BETA", &BETA);

    let y = (HEADER_HEIGHT - BUTTON) / 2.0;
    let close = Area::new(WIDTH - 10.0 - BUTTON, y, BUTTON, BUTTON);
    let minimize = Area::new(close.x - BUTTON, y, BUTTON, BUTTON);
    let pin = Area::new(minimize.x - BUTTON, y, BUTTON, BUTTON);
    let pin_color = if view.pinned {
        tokens::PRIMARY
    } else {
        tokens::TEXT_2
    };
    icon_button(s, pin, Icon::Pin, 14.0, pin_color, CompanionHit::Pin);
    icon_button(
        s,
        minimize,
        Icon::Minus,
        14.0,
        tokens::TEXT_2,
        CompanionHit::Minimize,
    );
    icon_button(s, close, Icon::X, 14.0, tokens::TEXT_2, CompanionHit::Close);
    if !view.minimized {
        s.hline(1.0, HEADER_HEIGHT - 1.0, WIDTH - 2.0, tokens::BORDER);
    }
}
