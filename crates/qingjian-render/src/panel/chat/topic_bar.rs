//! 主区顶栏：当前主题、BETA、清空会话与窗口按钮；空白处可拖动。

use super::hit::ChatHit;
use super::style::{BETA, CONTENT_INSET, HEADER_HEIGHT, PILL_BG, SIDEBAR_WIDTH, SMALL, SMALL_BOLD};
use super::view::ChatView;
use super::widgets::{Surface, fit_text, icon_button};
use crate::panel::{Area, Icon, tokens};

/// 窗口按钮边长与图标大小。
const BUTTON: f32 = 24.0;
const ICON: f32 = 14.0;

pub(super) fn draw(s: &mut Surface<'_>, view: &ChatView) {
    let width = view.width;
    s.hit(
        Area::new(SIDEBAR_WIDTH, 0.0, width - SIDEBAR_WIDTH, HEADER_HEIGHT),
        ChatHit::Drag,
    );

    let y = (HEADER_HEIGHT - BUTTON) / 2.0;
    let close = Area::new(width - 8.0 - BUTTON, y, BUTTON, BUTTON);
    let maximize = Area::new(close.x - BUTTON, y, BUTTON, BUTTON);
    let pin = Area::new(maximize.x - BUTTON, y, BUTTON, BUTTON);
    let pin_color = if view.pinned {
        tokens::PRIMARY
    } else {
        tokens::TEXT_2
    };
    icon_button(s, pin, Icon::Pin, ICON, pin_color, ChatHit::Pin);
    let maximize_icon = if view.maximized {
        Icon::Minimize2
    } else {
        Icon::Maximize2
    };
    icon_button(
        s,
        maximize,
        maximize_icon,
        ICON,
        tokens::TEXT_2,
        ChatHit::Maximize,
    );
    icon_button(s, close, Icon::X, ICON, tokens::TEXT_2, ChatHit::Close);

    let label = "清空会话";
    let pill_width = 12.0 + 13.0 + 5.0 + s.measure(label, &SMALL) + 12.0;
    let pill = Area::new(pin.x - 6.0 - pill_width, y, pill_width, BUTTON);
    s.fill(pill, tokens::RADIUS_BUTTON, PILL_BG);
    s.icon(Icon::Trash, pill.x + 12.0, y + 5.5, 13.0, tokens::TEXT_2);
    s.text(
        pill.x + 30.0,
        y + (BUTTON - SMALL.line_height) / 2.0,
        label,
        &SMALL,
    );
    s.hit(pill, ChatHit::ClearChat);

    let x = SIDEBAR_WIDTH + CONTENT_INSET;
    let text_y = (HEADER_HEIGHT - SMALL.line_height) / 2.0;
    let prefix = "当前主题：";
    let prefix_width = s.text(x, text_y, prefix, &SMALL.colored(tokens::TEXT_2));
    let beta_width = s.measure("BETA", &BETA) + 12.0;
    let topic = view.topic.as_deref().unwrap_or("自由提问");
    let room = pill.x - 12.0 - x - prefix_width - beta_width - 8.0;
    let topic = fit_text(s, topic, &SMALL_BOLD, room.max(40.0));
    let topic_width = s.text(x + prefix_width, text_y, &topic, &SMALL_BOLD);
    let beta_x = x + prefix_width + topic_width + 8.0;
    s.fill(
        Area::new(beta_x, (HEADER_HEIGHT - 14.0) / 2.0, beta_width, 14.0),
        4.0,
        tokens::TAG_BLUE.bg,
    );
    s.text(beta_x + 6.0, (HEADER_HEIGHT - 12.0) / 2.0, "BETA", &BETA);
}
