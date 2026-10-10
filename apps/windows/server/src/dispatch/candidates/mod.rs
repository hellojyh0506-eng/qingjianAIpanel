//! 候选窗口输出：Router 只产出 [`Frame`] 与光标矩形，交给 [`CandidateSink`] 去画；帧没变就不重画。

mod sink;

use qingjian_platform::protocol::{Frame, ScreenRect, SessionId};

pub use self::sink::{CandidateSink, NoopSink, RenderSettings};
use super::Router;

impl Router {
    /// 空帧收窗口；非空且已知光标矩形就重绘；还没收到矩形（组句刚起）先不显示，免得在旧位置闪一下。
    /// 纯提示（如快捷键切换、中英切换通知）在非组句态下允许回退至当前鼠标光标位置展示。
    pub(super) fn reconcile_candidates(&mut self, frame: &Frame) {
        if frame.is_empty() {
            self.engine.note_displayed(std::iter::empty());
            self.hide_candidate_window();
        } else if let Some(rect) = self.last_rect {
            let unchanged = matches!(&self.last_shown, Some((f, r)) if f == frame && *r == rect);
            if !unchanged {
                // 词汇记录的「看到轮次」按真正显示的页算，与 macOS 壳对齐。
                self.engine.note_displayed(frame.candidates.items.iter());
                self.candidates.show(frame.clone(), rect);
                self.last_shown = Some((frame.clone(), rect));
            }
        }
    }

    /// 获取有效的显示矩形：若已有宿主上报光标则用之，否则回退到鼠标当前坐标
    pub(super) fn effective_display_rect(&self) -> Option<ScreenRect> {
        if let Some(rect) = self.last_rect {
            return Some(rect);
        }
        let mut pt = windows::Win32::Foundation::POINT::default();
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_ok() && (pt.x != 0 || pt.y != 0) {
            Some(ScreenRect {
                left: pt.x,
                top: pt.y + 20,
                right: pt.x + 1,
                bottom: pt.y + 20,
            })
        } else {
            Some(ScreenRect {
                left: 100,
                top: 100,
                right: 101,
                bottom: 120,
            })
        }
    }

    pub(super) fn hide_candidate_window(&mut self) {
        self.last_rect = None;
        self.last_shown = None;
        self.candidates.hide();
    }

    pub(super) fn position_candidates(&mut self, session: SessionId, rect: ScreenRect) {
        if self.focused != Some(session) {
            return;
        }
        self.last_rect = Some(rect);
        // 自绘窗吃未降级的帧（降级只作用于发给 DLL 的那份）
        let frame = self.self_drawn_frame();
        self.reconcile_candidates(&frame);
    }
}
