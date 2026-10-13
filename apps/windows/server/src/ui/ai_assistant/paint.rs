//! 绘制：算尺寸与落点、调渲染器、贴到分层窗口，以及流式期间的重绘节流、转圈与光标闪烁定时器。

use std::time::{Duration, Instant};

use qingjian_render::chat::Phase;
use qingjian_render::{FontLibrary, PanelRenderer, Shadow};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, KillTimer, SetTimer};

use super::{
    AiAssistantWindow, CARET_BLINK_MS, FRAME_MS, Placed, SPIN_MS, State, TIMER_CARET, TIMER_FLUSH,
    TIMER_SPIN,
};
use crate::ui::{layered, monitor, placement};

/// 显示器工作区（物理像素）的宽高。
fn size_of(work: RECT) -> (i32, i32) {
    (work.right - work.left, work.bottom - work.top)
}

impl AiAssistantWindow {
    /// 立即画一帧并贴到窗口上；窗口没显示、最小化着、字体库加载失败都直接略过
    /// （最小化期间的变化记在 `dirty`，还原时补画）。
    pub(super) fn redraw(&self) {
        // 贴图会让系统同步发 WM_SIZE，消息过程里可能再来画；已经在画就不重入。
        let Ok(mut guard) = self.state.try_borrow_mut() else {
            return;
        };
        let state = &mut *guard;
        if !state.visible {
            return;
        }
        if unsafe { IsIconic(self.hwnd) }.as_bool() {
            state.dirty = true;
            return;
        }
        state.dirty = false;
        state.last_paint = Some(Instant::now());
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_FLUSH) };
        if let Err(message) = paint(self, state) {
            tracing::warn!(error = %message, "AI 问答面板绘制失败");
            return;
        }
        let waiting = state.view.phase == Phase::Waiting;
        let focused = state.focused;
        drop(guard);
        self.sync_spin(waiting);
        if focused {
            self.anchor_ime();
        }
    }

    /// 流式内容的重绘：距上一帧不足一帧时间就先记着，由补画定时器画出最后一块。
    pub(super) fn redraw_throttled(&self) {
        let wait = {
            let mut state = self.state.borrow_mut();
            state.dirty = true;
            let frame = Duration::from_millis(FRAME_MS as u64);
            match state.last_paint {
                Some(last) if last.elapsed() < frame => Some(frame - last.elapsed()),
                _ => None,
            }
        };
        match wait {
            None => self.redraw(),
            Some(wait) => unsafe {
                let _ = SetTimer(
                    Some(self.hwnd),
                    TIMER_FLUSH,
                    (wait.as_millis() as u32).max(1),
                    None,
                );
            },
        }
    }

    /// 等待回答时转圈，否则停表。
    fn sync_spin(&self, waiting: bool) {
        let mut state = self.state.borrow_mut();
        if waiting && !state.spinning {
            state.spinning = true;
            unsafe {
                let _ = SetTimer(Some(self.hwnd), TIMER_SPIN, SPIN_MS, None);
            }
        } else if !waiting && state.spinning {
            state.spinning = false;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_SPIN);
            }
        }
    }

    /// 焦点变化：得到焦点就开始闪光标，失去就停并收起光标。
    pub(super) fn on_focus_changed(&self, focused: bool) {
        {
            let mut state = self.state.borrow_mut();
            state.focused = focused;
            state.view.show_caret = focused;
        }
        unsafe {
            if focused {
                let _ = SetTimer(Some(self.hwnd), TIMER_CARET, CARET_BLINK_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_CARET);
            }
        }
        self.redraw();
    }

    /// 输入或移动光标之后：光标先亮起来，闪烁节拍重新计时。
    pub(super) fn restart_caret_blink(&self) {
        self.state.borrow_mut().view.show_caret = true;
        unsafe {
            let _ = SetTimer(Some(self.hwnd), TIMER_CARET, CARET_BLINK_MS, None);
        }
    }

    pub(super) fn blink_caret(&self) {
        let focused = {
            let mut state = self.state.borrow_mut();
            if state.focused {
                state.view.show_caret = !state.view.show_caret;
            }
            state.focused
        };
        if focused {
            self.redraw();
        }
    }
}

/// 画一帧：整理视图尺寸 → 渲染 → 定位 → 贴图 → 记下几何。
fn paint(window: &AiAssistantWindow, state: &mut State) -> Result<(), String> {
    let probe = state
        .content_pos
        .or(state.user_pos)
        .or(state.anchor.map(|anchor| (anchor.left, anchor.top)))
        .unwrap_or((0, 0));
    let probe_point = POINT {
        x: probe.0,
        y: probe.1,
    };
    let scale = monitor::dpi_near(probe_point).unwrap_or(96) as f32 / 96.0;
    let work = monitor::work_area_near(probe_point);
    let (work_w, work_h) = size_of(work);

    if state.renderer.is_none() {
        let library = FontLibrary::system("zh-CN").map_err(|error| error.to_string())?;
        state.renderer = Some(PanelRenderer::new(library));
    }
    let maximized = state.view.maximized;
    let shadow = (!maximized).then(Shadow::mac_panel);
    let renderer = state.renderer.as_mut().ok_or("没有渲染器")?;
    renderer.set_scale(scale);
    let margin_pt = shadow.map_or(0.0, |shadow| shadow.blur * 2.0 + shadow.offset_y);
    if maximized {
        state.view.width = work_w as f32 / scale;
        state.view.height = work_h as f32 / scale;
    } else {
        // 小屏幕上别让窗口超出工作区。
        let limit_w = (work_w as f32 / scale - 2.0 * margin_pt).max(480.0);
        let limit_h = (work_h as f32 / scale - 2.0 * margin_pt).max(360.0);
        state.view.width = state.normal_size.0.min(limit_w);
        state.view.height = state.normal_size.1.min(limit_h);
    }

    let frame = qingjian_render::chat::render(renderer, &mut state.view, shadow)
        .map_err(|error| error.to_string())?;
    state.view.scroll = frame.scroll;
    state.view.sidebar_scroll = state
        .view
        .sidebar_scroll
        .clamp(0.0, frame.sidebar_max_scroll);

    let margin = (frame.frame.margin * scale).round() as i32;
    let content = (
        frame.frame.pixmap.width() as i32 - 2 * margin,
        frame.frame.pixmap.height() as i32 - 2 * margin,
    );
    let pos = if maximized {
        (work.left, work.top)
    } else if let Some(pos) = state.content_pos {
        pos
    } else {
        let pos = match state.user_pos {
            Some(user) => placement::clamp_to_work_area(user, content, margin),
            None => centered(work, content),
        };
        state.content_pos = Some(pos);
        pos
    };
    if maximized {
        state.content_pos = Some(pos);
    }
    layered::present(
        window.hwnd,
        &frame.frame.pixmap,
        (pos.0 - margin, pos.1 - margin),
    )
    .map_err(|error| error.to_string())?;
    state.placed = Some(Placed {
        hits: frame.frame.hits,
        margin,
        scale,
        max_scroll: frame.max_scroll,
        sidebar_max_scroll: frame.sidebar_max_scroll,
        caret: frame.caret,
    });
    Ok(())
}

/// 内容在工作区正中。
fn centered(work: RECT, content: (i32, i32)) -> (i32, i32) {
    let (work_w, work_h) = size_of(work);
    (
        work.left + (work_w - content.0) / 2,
        work.top + (work_h - content.1) / 2,
    )
}
