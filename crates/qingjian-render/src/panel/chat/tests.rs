//! 整帧渲染的测试：点击区登记、滚动、排版缓存、输入框光标。没有系统字体的环境跳过。

use super::*;
use crate::fonts::FontLibrary;
use crate::panel::PanelRenderer;

fn renderer() -> Option<PanelRenderer> {
    let mut renderer = PanelRenderer::new(FontLibrary::system("zh-CN").ok()?);
    renderer.set_scale(2.0);
    Some(renderer)
}

fn view() -> ChatView {
    ChatView {
        topic: Some("Do your best".into()),
        sessions: (0..3)
            .map(|i| SessionItem {
                title: format!("会话 {i}"),
                subtitle: "3条消息 · 10/24".into(),
                selected: i == 0,
            })
            .collect(),
        suggestions: vec!["什么时候用？".into(), "请给我 IELTS 口语题".into()],
        ..ChatView::default()
    }
}

fn long_conversation(view: &mut ChatView) {
    for i in 0..6 {
        view.messages
            .push(ChatMessage::new(Role::User, format!("第 {i} 个问题")));
        let mut answer = ChatMessage::new(
            Role::Assistant,
            format!("第 {i} 个回答：\n- 要点一\n- 要点二\n```\ncode {i}\n```"),
        );
        answer.push_reasoning("想了想");
        view.messages.push(answer);
    }
}

fn find(frame: &ChatFrame, wanted: ChatHit) -> Option<crate::panel::Area> {
    frame
        .frame
        .hits
        .entries()
        .iter()
        .find(|(_, hit)| *hit == wanted)
        .map(|(area, _)| *area)
}

fn pixel(frame: &ChatFrame, x: f32, y: f32) -> (u8, u8, u8) {
    let p = frame
        .frame
        .pixmap
        .pixel(
            ((x + frame.frame.margin) * 2.0) as u32,
            ((y + frame.frame.margin) * 2.0) as u32,
        )
        .unwrap();
    (p.red(), p.green(), p.blue())
}

#[test]
fn fixed_controls_are_registered_where_they_are_painted() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    view.input = "hello".into();
    view.cursor = 5;
    let frame = render(&mut renderer, &mut view, None).unwrap();
    for hit in [
        ChatHit::Close,
        ChatHit::Pin,
        ChatHit::Maximize,
        ChatHit::ClearChat,
        ChatHit::NewChat,
        ChatHit::Settings,
        ChatHit::Session(0),
        ChatHit::Session(2),
        ChatHit::Suggestion(0),
        ChatHit::Suggestion(1),
        ChatHit::Input,
        ChatHit::ToggleWeb,
        ChatHit::ToggleDeep,
        ChatHit::Send,
    ] {
        let area = find(&frame, hit).unwrap_or_else(|| panic!("没登记 {hit:?}"));
        assert_eq!(
            frame
                .frame
                .hits
                .hit(area.x + area.width / 2.0, area.y + area.height / 2.0),
            Some(&hit)
        );
    }
    assert!(find(&frame, ChatHit::Stop).is_none());
    // 空白处是拖动区
    let drag = find(&frame, ChatHit::Drag).unwrap();
    assert_eq!(
        frame.frame.hits.hit(drag.x + 40.0, 6.0),
        Some(&ChatHit::Drag)
    );
    // 发送键是主色实心，选中的会话有选中底色
    let send = find(&frame, ChatHit::Send).unwrap();
    assert_eq!(
        pixel(&frame, send.x + 4.0, send.y + 18.0),
        (0x25, 0x63, 0xEB)
    );
    let row = find(&frame, ChatHit::Session(0)).unwrap();
    assert_eq!(
        pixel(&frame, row.x + 30.0, row.y + 33.0),
        (0xEB, 0xF2, 0xFF)
    );
}

#[test]
fn send_is_only_clickable_with_text_and_becomes_stop_while_busy() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let empty = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&empty, ChatHit::Send).is_none() && find(&empty, ChatHit::Stop).is_none());
    view.phase = Phase::Streaming;
    let busy = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&busy, ChatHit::Stop).is_some() && find(&busy, ChatHit::Send).is_none());
    view.phase = Phase::Idle;
    view.composition = "ni".into();
    let composing = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&composing, ChatHit::Send).is_some());
}

#[test]
fn toggles_are_highlighted_when_on() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let off = render(&mut renderer, &mut view, None).unwrap();
    view.deep = true;
    let on = render(&mut renderer, &mut view, None).unwrap();
    let area = find(&on, ChatHit::ToggleDeep).unwrap();
    let corner = (area.x + 2.5, area.y + 13.0);
    assert_ne!(
        pixel(&off, corner.0, corner.1),
        pixel(&on, corner.0, corner.1)
    );
    assert_eq!(pixel(&on, corner.0, corner.1), (0xEB, 0xF2, 0xFF));
}

#[test]
fn suggestions_only_show_in_an_empty_conversation() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let empty = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&empty, ChatHit::Suggestion(0)).is_some());
    view.messages.push(ChatMessage::new(Role::User, "hi"));
    let started = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&started, ChatHit::Suggestion(0)).is_none());
}

#[test]
fn follow_bottom_sticks_and_manual_scroll_is_clamped() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    long_conversation(&mut view);
    let bottom = render(&mut renderer, &mut view, None).unwrap();
    assert!(bottom.max_scroll > 100.0);
    assert_eq!(bottom.scroll, bottom.max_scroll);

    view.follow_bottom = false;
    view.scroll = 40.0;
    let middle = render(&mut renderer, &mut view, None).unwrap();
    assert_eq!(middle.scroll, 40.0);
    view.scroll = 1.0e6;
    let clamped = render(&mut renderer, &mut view, None).unwrap();
    assert_eq!(clamped.scroll, clamped.max_scroll);
    view.scroll = -5.0;
    assert_eq!(render(&mut renderer, &mut view, None).unwrap().scroll, 0.0);
}

#[test]
fn scrolled_out_message_hits_are_not_clickable() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    long_conversation(&mut view);
    view.follow_bottom = false;
    view.scroll = 0.0;
    let frame = render(&mut renderer, &mut view, None).unwrap();
    let copies: Vec<_> = frame
        .frame
        .hits
        .entries()
        .iter()
        .filter(|(_, hit)| matches!(hit, ChatHit::CopyMessage(_)))
        .collect();
    assert!(
        !copies.is_empty() && copies.len() < 6,
        "只有视口里的复制键可点"
    );
    for (area, _) in copies {
        assert!(area.y >= 32.0 && area.bottom() <= frame.input.y, "{area:?}");
    }
}

#[test]
fn layout_is_cached_and_only_the_appended_bubble_is_relaid() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    long_conversation(&mut view);
    render(&mut renderer, &mut view, None).unwrap();
    assert!(view.messages.iter().all(|m| m.layout.is_some()));
    let before = view.messages[0].layout.clone();

    view.phase = Phase::Streaming;
    view.last_assistant_mut().unwrap().push_text("\n再补一句");
    let last = view.messages.len() - 1;
    assert!(view.messages[last].layout.is_none());
    assert!(view.messages[..last].iter().all(|m| m.layout.is_some()));
    render(&mut renderer, &mut view, None).unwrap();
    assert!(view.messages[last].layout.is_some());
    assert_eq!(view.messages[0].layout, before);

    // 流式中不画复制键，结束后才有
    let streaming = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&streaming, ChatHit::CopyMessage(last)).is_none());
    view.phase = Phase::Idle;
    let done = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&done, ChatHit::CopyMessage(last)).is_some());
}

#[test]
fn reasoning_row_toggles_and_expands_the_bubble() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    long_conversation(&mut view);
    let closed = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&closed, ChatHit::ToggleReasoning(11)).is_some());
    let closed_height = view.messages[11].layout.as_ref().unwrap().height;
    view.messages[11].set_reasoning_open(true);
    let open = render(&mut renderer, &mut view, None).unwrap();
    assert!(view.messages[11].layout.as_ref().unwrap().height > closed_height + 10.0);
    assert!(open.max_scroll > closed.max_scroll);
}

#[test]
fn waiting_phase_adds_a_placeholder_below_the_last_message() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    view.messages.push(ChatMessage::new(Role::User, "hi"));
    let idle = render(&mut renderer, &mut view, None).unwrap();
    view.phase = Phase::Waiting;
    let waiting = render(&mut renderer, &mut view, None).unwrap();
    assert!(waiting.max_scroll >= idle.max_scroll);
    assert!(find(&waiting, ChatHit::Stop).is_some());
}

#[test]
fn caret_follows_text_composition_and_newlines() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let empty = render(&mut renderer, &mut view, None).unwrap();
    view.input = "hello".into();
    view.cursor = 5;
    let typed = render(&mut renderer, &mut view, None).unwrap();
    assert!(typed.caret.0 > empty.caret.0 + 20.0);
    assert_eq!(typed.caret.1, empty.caret.1);

    view.cursor = 2;
    let middle = render(&mut renderer, &mut view, None).unwrap();
    assert!(middle.caret.0 > empty.caret.0 && middle.caret.0 < typed.caret.0);

    view.cursor = 5;
    view.composition = "你好".into();
    let composing = render(&mut renderer, &mut view, None).unwrap();
    assert!(composing.caret.0 > typed.caret.0 + 20.0);

    view.composition.clear();
    view.input = "a\nb".into();
    view.cursor = 3;
    let second = render(&mut renderer, &mut view, None).unwrap();
    assert!(second.caret.1 > empty.caret.1 - 0.5);
    assert!(second.input.height > empty.input.height);
    // 光标在输入框里
    assert!(second.input.contains(second.caret.0, second.caret.1 + 1.0));
}

#[test]
fn caret_is_drawn_only_when_asked() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    view.input = "hello".into();
    view.cursor = 5;
    let off = render(&mut renderer, &mut view, None).unwrap();
    view.show_caret = true;
    let on = render(&mut renderer, &mut view, None).unwrap();
    assert_eq!(off.caret, on.caret);
    let pixel = |frame: &ChatFrame| {
        let (x, y, h) = frame.caret;
        let scale = 2.0;
        let px = ((x + 0.75) * scale) as u32;
        let py = ((y + h / 2.0) * scale) as u32;
        frame.frame.pixmap.pixel(px, py)
    };
    assert_ne!(pixel(&off), pixel(&on));
}

#[test]
fn input_grows_with_lines_and_stops_at_four() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let height = |lines: usize, renderer: &mut PanelRenderer, view: &mut ChatView| {
        view.input = vec!["行"; lines].join("\n");
        view.cursor = view.input.len();
        render(renderer, view, None).unwrap().input.height
    };
    let one = height(1, &mut renderer, &mut view);
    let two = height(2, &mut renderer, &mut view);
    let four = height(4, &mut renderer, &mut view);
    let nine = height(9, &mut renderer, &mut view);
    assert_eq!(one, 35.0);
    assert_eq!(two, one + 18.0);
    assert_eq!(four, nine);
}

#[test]
fn window_size_follows_the_view_and_maximize_icon_swaps() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    let normal = render(&mut renderer, &mut view, None).unwrap();
    assert_eq!(
        (normal.frame.pixmap.width(), normal.frame.pixmap.height()),
        (1440, 1040)
    );
    view.width = 1000.0;
    view.height = 600.0;
    view.maximized = true;
    let big = render(&mut renderer, &mut view, None).unwrap();
    assert_eq!(
        (big.frame.pixmap.width(), big.frame.pixmap.height()),
        (2000, 1200)
    );
    let close = find(&big, ChatHit::Close).unwrap();
    assert!(close.right() <= 1000.0 && close.x > 900.0);
}

#[test]
fn sidebar_scrolls_when_sessions_overflow() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = view();
    view.sessions = (0..20)
        .map(|i| SessionItem {
            title: format!("会话 {i}"),
            subtitle: String::new(),
            selected: false,
        })
        .collect();
    let top = render(&mut renderer, &mut view, None).unwrap();
    assert!(top.sidebar_max_scroll > 100.0);
    assert!(find(&top, ChatHit::Session(19)).is_none());
    view.sidebar_scroll = 1.0e6;
    let bottom = render(&mut renderer, &mut view, None).unwrap();
    assert!(find(&bottom, ChatHit::Session(19)).is_some());
    assert!(find(&bottom, ChatHit::Session(0)).is_none());
}
