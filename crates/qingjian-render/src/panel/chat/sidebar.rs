//! 左侧栏：标题与新建、会话列表（可滚动）、底部设置。

use super::hit::ChatHit;
use super::style::{
    HEADER_HEIGHT, LOGO, ROW_HEIGHT, ROW_PITCH, ROW_TOP, SELECTED_BORDER, SETTINGS_HEIGHT,
    SIDE_TEXT, SIDEBAR_BG, SIDEBAR_WIDTH, SMALL_BOLD, TINY,
};
use super::view::ChatView;
use super::widgets::{Surface, fit_text, icon_button};
use crate::panel::{Area, Icon, tokens};

/// 画侧栏，返回会话列表最多能滚多远（点）。
pub(super) fn draw(s: &mut Surface<'_>, view: &ChatView) -> f32 {
    let height = view.height;
    // 左侧两个圆角跟着窗口，右侧直角
    s.fill(
        Area::new(1.0, 1.0, SIDEBAR_WIDTH - 1.0, height - 2.0),
        tokens::WINDOW_RADIUS - 1.0,
        SIDEBAR_BG,
    );
    s.fill(
        Area::new(SIDEBAR_WIDTH - 40.0, 1.0, 40.0, height - 2.0),
        0.0,
        SIDEBAR_BG,
    );
    s.fill(
        Area::new(SIDEBAR_WIDTH, 1.0, 1.0, height - 2.0),
        0.0,
        tokens::BORDER,
    );

    s.circle(17.0, HEADER_HEIGHT / 2.0, 9.0, LOGO);
    s.icon(
        Icon::Sparkles,
        12.5,
        HEADER_HEIGHT / 2.0 - 4.5,
        9.0,
        tokens::WINDOW_BG,
    );
    s.text(
        32.0,
        (HEADER_HEIGHT - 16.0) / 2.0,
        "青简 AI 聊天",
        &SMALL_BOLD,
    );
    icon_button(
        s,
        Area::new(SIDEBAR_WIDTH - 32.0, 4.0, 24.0, 24.0),
        Icon::Plus,
        14.0,
        tokens::PRIMARY,
        ChatHit::NewChat,
    );

    let button = Area::new(8.0, 31.0, SIDEBAR_WIDTH - 16.0, 24.0);
    s.fill_bordered(
        button,
        tokens::RADIUS_BUTTON,
        tokens::WINDOW_BG,
        tokens::BORDER,
    );
    s.icon(
        Icon::Plus,
        button.x + 9.0,
        button.y + 5.0,
        14.0,
        tokens::TEXT_2,
    );
    s.text(button.x + 31.0, button.y + 4.5, "新对话", &SIDE_TEXT);
    s.hit(button, ChatHit::NewChat);

    let list = Area::new(
        0.0,
        ROW_TOP - 2.0,
        SIDEBAR_WIDTH,
        height - SETTINGS_HEIGHT - ROW_TOP + 2.0,
    );
    let content = view.sessions.len() as f32 * ROW_PITCH;
    let max_scroll = (content - list.height + 4.0).max(0.0);
    let scroll = view.sidebar_scroll.clamp(0.0, max_scroll);
    s.set_clip(Some(list));
    for (index, item) in view.sessions.iter().enumerate() {
        let row = Area::new(
            6.0,
            ROW_TOP + index as f32 * ROW_PITCH - scroll,
            SIDEBAR_WIDTH - 12.0,
            ROW_HEIGHT,
        );
        if row.bottom() < list.y || row.y > list.bottom() {
            continue;
        }
        let (title_color, icon_color) = if item.selected {
            s.fill_bordered(
                row,
                tokens::RADIUS_BUTTON,
                tokens::SELECTED_BG,
                SELECTED_BORDER,
            );
            (tokens::PRIMARY, tokens::PRIMARY)
        } else {
            (tokens::TEXT, tokens::TEXT)
        };
        s.icon(
            Icon::MessageSquare,
            row.x + 11.0,
            row.y + 12.0,
            14.0,
            icon_color,
        );
        let text_x = row.x + 34.0;
        let room = row.right() - 8.0 - text_x;
        let title = fit_text(s, &item.title, &SIDE_TEXT, room);
        s.text(text_x, row.y + 4.5, &title, &SIDE_TEXT.colored(title_color));
        let subtitle = fit_text(s, &item.subtitle, &TINY, room);
        s.text(text_x, row.y + 20.5, &subtitle, &TINY);
        s.hit(row, ChatHit::Session(index));
    }
    s.set_clip(None);

    let footer = Area::new(
        0.0,
        height - SETTINGS_HEIGHT,
        SIDEBAR_WIDTH,
        SETTINGS_HEIGHT - 1.0,
    );
    s.fill(
        Area::new(1.0, footer.y, SIDEBAR_WIDTH - 1.0, 1.0),
        0.0,
        tokens::BORDER,
    );
    s.icon(Icon::Settings, 17.0, footer.y + 14.0, 15.0, tokens::TEXT);
    s.text(40.0, footer.y + 14.5, "设置", &SIDE_TEXT);
    s.hit(footer, ChatHit::Settings);
    max_scroll
}
