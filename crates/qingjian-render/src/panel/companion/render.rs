//! 学习助手面板的整帧渲染：固定的标题栏、英雄区、标签栏、底部按钮，中间一块可滚动的内容区。

use super::footer;
use super::frame::CompanionFrame;
use super::header;
use super::hero;
use super::style::{
    BODY, CONTENT_GAP, FOOTER_Y, HEADER_HEIGHT, VIEWPORT_TOP, WIDTH, WINDOW_HEIGHT,
};
use super::tab_alternatives;
use super::tab_bar;
use super::tab_dialogue;
use super::tab_nuance;
use super::view::CompanionView;
use super::widgets::Surface;
use crate::error::RenderError;
use crate::panel::{Area, PanelRenderer, tokens};
use crate::shadow::Shadow;

/// 底部提示条的位置（离底部分割线）。
const NOTICE_OFFSET: f32 = 40.0;

/// 滚动条宽度。
const SCROLLBAR: f32 = 3.0;

/// 画一帧。滚动量超出内容时夹回去并重画一次（只在内容变短的那一帧发生）。
pub fn render(
    renderer: &mut PanelRenderer,
    view: &CompanionView,
    shadow: Option<Shadow>,
) -> Result<CompanionFrame, RenderError> {
    let first = draw(renderer, view, view.scroll.max(0.0), shadow)?;
    if view.scroll <= first.max_scroll + 0.5 || view.minimized {
        return Ok(first);
    }
    draw(renderer, view, first.max_scroll, shadow)
}

fn draw(
    renderer: &mut PanelRenderer,
    view: &CompanionView,
    scroll: f32,
    shadow: Option<Shadow>,
) -> Result<CompanionFrame, RenderError> {
    let height = if view.minimized {
        HEADER_HEIGHT
    } else {
        WINDOW_HEIGHT
    };
    let mut s = renderer.begin(WIDTH, height, shadow)?;
    header::draw(&mut s, view);
    if view.minimized {
        return Ok(CompanionFrame {
            frame: s.finish(),
            max_scroll: 0.0,
            scroll: 0.0,
            height,
        });
    }
    let viewport = Area::new(0.0, VIEWPORT_TOP, WIDTH, FOOTER_Y - VIEWPORT_TOP);
    s.set_clip(Some(viewport));
    let top = VIEWPORT_TOP + CONTENT_GAP;
    let end = draw_tab(&mut s, view, top - scroll);
    s.set_clip(None);
    let content_height = end + scroll - VIEWPORT_TOP;
    let max_scroll = (content_height - viewport.height).max(0.0);
    let scroll = scroll.min(max_scroll);

    hero::draw(&mut s, view);
    tab_bar::draw(&mut s, view);
    footer::draw(&mut s, view);
    if max_scroll > 0.0 {
        scrollbar(&mut s, viewport, scroll, max_scroll);
    }
    if let Some(text) = &view.notice {
        notice(&mut s, text);
    }
    Ok(CompanionFrame {
        frame: s.finish(),
        max_scroll,
        scroll,
        height,
    })
}

fn draw_tab(s: &mut Surface<'_>, view: &CompanionView, y: f32) -> f32 {
    match view.tab {
        0 => tab_nuance::draw(s, view, y),
        1 => tab_alternatives::draw(s, view, y),
        _ => tab_dialogue::draw(s, view, y),
    }
}

/// 右边缘的细滚动条：只显示位置，不可拖。
fn scrollbar(s: &mut Surface<'_>, viewport: Area, scroll: f32, max_scroll: f32) {
    let content = viewport.height + max_scroll;
    let thumb = (viewport.height * viewport.height / content).max(24.0);
    let y = viewport.y + 4.0 + (viewport.height - 8.0 - thumb) * (scroll / max_scroll);
    s.fill(
        Area::new(WIDTH - SCROLLBAR - 3.0, y, SCROLLBAR, thumb),
        SCROLLBAR / 2.0,
        crate::color::Color::hex(0xCBD5E1),
    );
}

/// 底部居中的短暂提示（「已复制」「已收藏」）。
fn notice(s: &mut Surface<'_>, text: &str) {
    let style = BODY.colored(tokens::WINDOW_BG);
    let width = s.measure(text, &style) + 28.0;
    let area = Area::new((WIDTH - width) / 2.0, FOOTER_Y - NOTICE_OFFSET, width, 28.0);
    s.fill(area, 14.0, tokens::TEXT);
    s.text(area.x + 14.0, area.y + 5.0, text, &style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontLibrary;
    use crate::panel::Avatar;
    use crate::panel::companion::{
        Alternative, CompanionHit, DialogueContent, Example, NuanceContent, TabStatus, Turn,
    };

    /// 没有系统字体的环境返回 `None`，测试跳过。
    fn renderer() -> Option<PanelRenderer> {
        let mut renderer = PanelRenderer::new(FontLibrary::system("zh-CN").ok()?);
        renderer.set_scale(2.0);
        Some(renderer)
    }

    fn example(en: &str) -> Example {
        Example {
            en: en.into(),
            zh: "例句中文".into(),
            hl: "best".into(),
        }
    }

    fn view() -> CompanionView {
        let mut view = CompanionView::new("尽量", "Do your best");
        view.tags = vec!["自然口语".into()];
        view.level = "B1-B2".into();
        view.nuance = NuanceContent {
            meaning: "尽你所能。".into(),
            tone: "鼓励、支持。".into(),
            usage: "用于安慰或鼓励别人。".into(),
            example: Some(example("Just do your best.")),
            details: Vec::new(),
        };
        view.nuance_status = TabStatus::Done;
        view.alternatives = (0..3)
            .map(|i| Alternative {
                expression: format!("Give it your best shot {i}"),
                zh: "尽力一试。".into(),
                tag: "日常口语".into(),
                tone: "鼓励、轻松".into(),
                diff: "强调尝试，不确定能否成功时更自然。".into(),
                example: example("You should give it your best shot."),
            })
            .collect();
        view.alternatives_status = TabStatus::Done;
        view.dialogue = DialogueContent {
            title: "朋友聊天".into(),
            turns: [Avatar::Boy, Avatar::Girl, Avatar::Boy]
                .into_iter()
                .map(|speaker| Turn {
                    speaker,
                    en: "You'll be fine. Just do your best.".into(),
                    zh: "你会没问题的。".into(),
                    hl: "do your best".into(),
                })
                .collect(),
            points: vec!["自然。".into(), "友好。".into()],
        };
        view.dialogue_status = TabStatus::Done;
        view
    }

    fn pixel(frame: &CompanionFrame, x: f32, y: f32) -> (u8, u8, u8) {
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
        let frame = render(&mut renderer, &view(), None).unwrap();
        let hits = &frame.frame.hits;
        let find = |wanted: &CompanionHit| {
            hits.entries()
                .iter()
                .find(|(_, hit)| hit == wanted)
                .map(|(area, _)| *area)
                .unwrap_or_else(|| panic!("没登记 {wanted:?}"))
        };
        for hit in [
            CompanionHit::Close,
            CompanionHit::Pin,
            CompanionHit::Minimize,
            CompanionHit::Save,
            CompanionHit::Practice,
            CompanionHit::Tab(0),
            CompanionHit::Tab(2),
            CompanionHit::Speak("Do your best".into()),
        ] {
            let area = find(&hit);
            assert_eq!(
                hits.hit(area.x + area.width / 2.0, area.y + area.height / 2.0),
                Some(&hit)
            );
        }
        // 「问 AI」是实心主色按钮：点击区左侧内缘的像素就是主色
        let ask = find(&CompanionHit::AskAi);
        assert_eq!(
            pixel(&frame, ask.x + 5.0, ask.y + ask.height / 2.0),
            (0x25, 0x63, 0xEB)
        );
        // 选中的标签有选中底色
        let tab = find(&CompanionHit::Tab(0));
        assert_eq!(pixel(&frame, tab.x + 8.0, tab.y + 6.0), (0xEB, 0xF2, 0xFF));
        // 标题栏空白处是拖动区，按钮盖在它上面
        let drag = find(&CompanionHit::Drag);
        assert_eq!(
            hits.hit(drag.x + 100.0, drag.y + 10.0),
            Some(&CompanionHit::Drag)
        );
    }

    #[test]
    fn scroll_is_clamped_and_scrolled_out_hits_are_not_clickable() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut view = view();
        view.tab = 1;
        let top = render(&mut renderer, &view, None).unwrap();
        assert!(top.max_scroll > 20.0, "三张替换表达卡应该超出视口");
        view.scroll = 10_000.0;
        let bottom = render(&mut renderer, &view, None).unwrap();
        assert_eq!(bottom.scroll, bottom.max_scroll);
        assert_eq!(bottom.max_scroll, top.max_scroll);
        for frame in [&top, &bottom] {
            for (area, hit) in frame.frame.hits.entries() {
                if matches!(hit, CompanionHit::Copy(_) | CompanionHit::Speak(_)) && area.y > 150.0 {
                    assert!(
                        area.y >= VIEWPORT_TOP && area.bottom() <= FOOTER_Y,
                        "{hit:?} 的点击区 {area:?} 越出了滚动视口"
                    );
                }
            }
        }
        // 滚到底以后，最后一张卡的复制按钮在视口里
        assert!(bottom.frame.hits.entries().iter().any(
            |(_, hit)| *hit == CompanionHit::Copy("You should give it your best shot.".into())
        ));
    }

    #[test]
    fn longer_text_makes_the_content_taller() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut view = view();
        let short = render(&mut renderer, &view, None).unwrap().max_scroll;
        view.nuance.meaning = "这是一段很长的释义，".repeat(30);
        let long = render(&mut renderer, &view, None).unwrap().max_scroll;
        assert!(long > short + 100.0, "short={short} long={long}");
    }

    #[test]
    fn every_status_of_every_tab_renders() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let statuses = [
            TabStatus::Idle,
            TabStatus::Loading,
            TabStatus::Streaming,
            TabStatus::Done,
            TabStatus::Failed("网络连接超时".into()),
        ];
        for tab in 0..3 {
            for status in &statuses {
                for empty in [false, true] {
                    let mut view = view();
                    view.tab = tab;
                    if empty {
                        view.nuance = NuanceContent::default();
                        view.alternatives.clear();
                        view.dialogue = DialogueContent::default();
                    }
                    view.nuance_status = status.clone();
                    view.alternatives_status = status.clone();
                    view.dialogue_status = status.clone();
                    let frame = render(&mut renderer, &view, None).unwrap();
                    assert_eq!(frame.height, 560.0);
                    let retry = frame
                        .frame
                        .hits
                        .entries()
                        .iter()
                        .any(|(_, hit)| *hit == CompanionHit::Retry);
                    assert_eq!(
                        retry,
                        matches!(status, TabStatus::Failed(_)),
                        "tab={tab} status={status:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn minimized_window_is_only_the_title_bar() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut view = view();
        view.minimized = true;
        let frame = render(&mut renderer, &view, None).unwrap();
        assert_eq!(frame.height, HEADER_HEIGHT);
        assert_eq!(frame.frame.pixmap.height(), (HEADER_HEIGHT * 2.0) as u32);
        assert!(frame.frame.hits.entries().iter().all(|(_, hit)| matches!(
            hit,
            CompanionHit::Drag | CompanionHit::Pin | CompanionHit::Minimize | CompanionHit::Close
        )));
    }

    #[test]
    fn scene_menu_items_win_and_everything_else_dismisses() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut view = view();
        view.tab = 2;
        view.scene_menu_open = true;
        let frame = render(&mut renderer, &view, None).unwrap();
        let hits = &frame.frame.hits;
        let (area, _) = hits
            .entries()
            .iter()
            .find(|(_, hit)| *hit == CompanionHit::Scene(4))
            .expect("菜单里有旅行出行");
        assert_eq!(
            hits.hit(area.x + 10.0, area.y + 10.0),
            Some(&CompanionHit::Scene(4))
        );
        assert_eq!(hits.hit(20.0, 300.0), Some(&CompanionHit::DismissMenu));
        view.scene_menu_open = false;
        let closed = render(&mut renderer, &view, None).unwrap();
        assert!(
            !closed
                .frame
                .hits
                .entries()
                .iter()
                .any(|(_, hit)| *hit == CompanionHit::DismissMenu)
        );
    }
}
