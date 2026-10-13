//! 消息区：欢迎卡、建议问题、气泡与「正在思考」占位，在视口里滚动。

use super::bubble;
use super::phase::Phase;
use super::role::Role;
use super::style::{BUBBLE_GAP, CONTENT_INSET, SIDEBAR_WIDTH, SMALL};
use super::view::ChatView;
use super::welcome;
use super::widgets::Surface;
use crate::color::Color;
use crate::panel::{Area, Icon, tokens};

/// 视口第一块内容离上沿、最后一块离下沿的距离。
const TOP_GAP: f32 = 8.0;
const BOTTOM_GAP: f32 = 12.0;

/// 右边留白；用户气泡最多占内容宽度的比例。
const RIGHT_INSET: f32 = 20.0;
const USER_WIDTH_RATIO: f32 = 0.78;

/// 「正在思考」占位气泡高度。
const WAITING_HEIGHT: f32 = 34.0;

const SCROLLBAR: f32 = 3.0;

/// 画消息区，返回 `(最大滚动量, 实际滚动量)`（点）。
pub(super) fn draw(
    s: &mut Surface<'_>,
    view: &mut ChatView,
    scale: f32,
    viewport: Area,
) -> (f32, f32) {
    let left = SIDEBAR_WIDTH + CONTENT_INSET;
    let right = view.width - RIGHT_INSET;
    let content_width = right - left;

    let welcome_size = welcome::measure(s, view.topic.as_deref(), content_width);
    let show_chips = view.messages.is_empty() && view.phase == Phase::Idle;
    let (chips, chips_height) = if show_chips {
        welcome::measure_suggestions(s, &view.suggestions, content_width)
    } else {
        (Vec::new(), 0.0)
    };

    let last = view.messages.len().saturating_sub(1);
    let streaming = view.phase == Phase::Streaming;
    let mut offsets = Vec::with_capacity(view.messages.len());
    let mut y = welcome_size.1 + chips_height;
    for (index, message) in view.messages.iter_mut().enumerate() {
        let max_width = match message.role {
            Role::User => content_width * USER_WIDTH_RATIO,
            Role::Assistant | Role::Error => content_width,
        };
        let live = streaming && index == last && message.role == Role::Assistant;
        bubble::ensure(s, message, max_width, scale, live);
        y += BUBBLE_GAP;
        offsets.push(y);
        y += message.layout.as_ref().map_or(0.0, |layout| layout.height);
    }
    let waiting_y = y + BUBBLE_GAP;
    if view.phase == Phase::Waiting {
        y = waiting_y + WAITING_HEIGHT;
    }
    let content_height = TOP_GAP + y + BOTTOM_GAP;

    let max_scroll = (content_height - viewport.height).max(0.0);
    let scroll = if view.follow_bottom {
        max_scroll
    } else {
        view.scroll.clamp(0.0, max_scroll)
    };

    s.set_clip(Some(viewport));
    let top = viewport.y + TOP_GAP - scroll;
    let visible = |y: f32, height: f32| y + height > viewport.y && y < viewport.bottom();
    if visible(top, welcome_size.1) {
        welcome::draw(s, view.topic.as_deref(), (left, top), welcome_size);
    }
    if show_chips {
        welcome::draw_suggestions(s, &view.suggestions, &chips, (left, top + welcome_size.1));
    }
    for (index, message) in view.messages.iter().enumerate() {
        let Some(layout) = &message.layout else {
            continue;
        };
        let y = top + offsets[index];
        if !visible(y, layout.height) {
            continue;
        }
        let x = match message.role {
            Role::User => right - layout.width,
            Role::Assistant | Role::Error => left,
        };
        let live = streaming && index == last && message.role == Role::Assistant;
        bubble::draw(s, message, index, (x, y), live);
    }
    if view.phase == Phase::Waiting {
        waiting(s, view.spin, (left, top + waiting_y));
    }
    s.set_clip(None);

    if max_scroll > 0.0 {
        scrollbar(s, viewport, scroll, max_scroll);
    }
    (max_scroll, scroll)
}

fn waiting(s: &mut Surface<'_>, spin: f32, (x, y): (f32, f32)) {
    let label = "正在思考…";
    let width = 14.0 + 14.0 + 7.0 + s.measure(label, &SMALL) + 14.0;
    s.fill(
        Area::new(x, y, width, WAITING_HEIGHT),
        super::style::BUBBLE_RADIUS,
        tokens::CARD_BLUE,
    );
    s.icon_rotated(
        Icon::LoaderCircle,
        x + 14.0,
        y + 10.0,
        14.0,
        tokens::TEXT_2,
        spin,
    );
    s.text(
        x + 35.0,
        y + (WAITING_HEIGHT - SMALL.line_height) / 2.0,
        label,
        &SMALL,
    );
}

/// 右边缘的细滚动条：只显示位置，不可拖。
fn scrollbar(s: &mut Surface<'_>, viewport: Area, scroll: f32, max_scroll: f32) {
    let content = viewport.height + max_scroll;
    let thumb = (viewport.height * viewport.height / content).max(24.0);
    let y = viewport.y + 4.0 + (viewport.height - 8.0 - thumb) * (scroll / max_scroll);
    s.fill(
        Area::new(viewport.right() - SCROLLBAR - 3.0, y, SCROLLBAR, thumb),
        SCROLLBAR / 2.0,
        Color::hex(0xCBD5E1),
    );
}
