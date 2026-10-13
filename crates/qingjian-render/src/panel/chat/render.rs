//! AI 问答面板的整帧渲染：侧栏、顶栏、可滚动的消息区、底部输入区。

use super::composer;
use super::frame::ChatFrame;
use super::messages;
use super::sidebar;
use super::style::{HEADER_HEIGHT, SIDEBAR_WIDTH, SMALL};
use super::topic_bar;
use super::view::ChatView;
use super::widgets::Surface;
use crate::error::RenderError;
use crate::panel::{Area, PanelRenderer, tokens};
use crate::shadow::Shadow;

/// 消息区下沿离输入框的距离。
const VIEWPORT_GAP: f32 = 6.0;

/// 画一帧。`view` 里每条消息的排版缓存会被更新，所以是可变引用。
pub fn render(
    renderer: &mut PanelRenderer,
    view: &mut ChatView,
    shadow: Option<Shadow>,
) -> Result<ChatFrame, RenderError> {
    let scale = renderer.scale();
    let mut s = renderer.begin(view.width, view.height, shadow)?;
    let sidebar_max_scroll = sidebar::draw(&mut s, view);
    topic_bar::draw(&mut s, view);

    let composer = composer::layout(&mut s, view);
    let viewport = Area::new(
        SIDEBAR_WIDTH + 1.0,
        HEADER_HEIGHT,
        view.width - SIDEBAR_WIDTH - 2.0,
        composer.input.y - VIEWPORT_GAP - HEADER_HEIGHT,
    );
    let (max_scroll, scroll) = messages::draw(&mut s, view, scale, viewport);
    let caret = composer::draw(&mut s, view, &composer);
    if let Some(text) = &view.notice {
        notice(&mut s, view.width, composer.input.y, text);
    }
    Ok(ChatFrame {
        input: composer.input,
        frame: s.finish(),
        max_scroll,
        scroll,
        sidebar_max_scroll,
        caret,
    })
}

/// 输入框上方居中的短暂提示（「已复制」）。
fn notice(s: &mut Surface<'_>, width: f32, input_top: f32, text: &str) {
    let style = SMALL.colored(tokens::WINDOW_BG);
    let box_width = s.measure(text, &style) + 28.0;
    let center = SIDEBAR_WIDTH + (width - SIDEBAR_WIDTH) / 2.0;
    let area = Area::new(center - box_width / 2.0, input_top - 40.0, box_width, 28.0);
    s.fill(area, 14.0, tokens::TEXT);
    s.text(area.x + 14.0, area.y + 6.0, text, &style);
}
