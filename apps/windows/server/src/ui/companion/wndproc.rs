//! 学习助手面板的窗口过程与点击分派。

use qingjian_render::companion::CompanionHit;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowRect, HTCAPTION, HTCLIENT, KillTimer, MA_NOACTIVATE, SendMessageW,
    WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_TIMER,
};

use super::tts::{SpeakError, Speaker};
use super::{
    CompanionWindow, SPIN_STEP, TIMER_NOTICE, TIMER_SPIN, WHEEL_STEP, model, window_of,
};
use crate::dispatch::CandidateAction;
use crate::ui::clipboard;

pub(super) unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // 纯阅读学习，不抢宿主应用的输入焦点。
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_LBUTTONDOWN => {
            if let Some(window) = window_of(hwnd) {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if let Some(hit) = window.hit_at(x, y) {
                    window.on_hit(hit);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(window) = window_of(hwnd) {
                let notches = (wparam.0 >> 16) as i16 as f32 / 120.0;
                window.scroll_by(-notches * WHEEL_STEP);
            }
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            if let Some(window) = window_of(hwnd) {
                window.remember_dragged_position();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(window) = window_of(hwnd) {
                window.on_timer(wparam.0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

impl CompanionWindow {
    /// 窗口客户区像素坐标处点到了什么。
    fn hit_at(&self, x: i32, y: i32) -> Option<CompanionHit> {
        let state = self.state.borrow();
        let placed = state.placed.as_ref()?;
        let px = (x - placed.margin) as f32 / placed.scale;
        let py = (y - placed.margin) as f32 / placed.scale;
        placed.hits.hit(px, py).cloned()
    }

    fn on_hit(&self, hit: CompanionHit) {
        match hit {
            CompanionHit::Drag => unsafe {
                // 交给系统走原生拖动，拖完在 WM_EXITSIZEMOVE 里记位置。
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    self.hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
            },
            CompanionHit::Close => {
                self.hide();
                (self.action_cb)(CandidateAction::CloseAssistant);
            }
            CompanionHit::Pin => {
                self.set_pinned(!self.is_pinned());
            }
            CompanionHit::Minimize => {
                {
                    let mut state = self.state.borrow_mut();
                    state.view.minimized = !state.view.minimized;
                    state.view.scene_menu_open = false;
                }
                self.redraw();
            }
            CompanionHit::Speak(text) => self.speak(&text),
            CompanionHit::Copy(text) => {
                let notice = if clipboard::copy_text(&text) {
                    "已复制"
                } else {
                    "复制失败，剪贴板被占用"
                };
                self.set_notice(notice.to_string(), false);
            }
            CompanionHit::Tab(tab) => {
                let fresh = model::select_tab(&mut self.state.borrow_mut().view, tab);
                self.redraw();
                if fresh {
                    (self.action_cb)(CandidateAction::CompanionSelectTab(tab));
                }
            }
            CompanionHit::Scene(scene) => {
                let fresh = model::select_scene(&mut self.state.borrow_mut().view, scene);
                self.redraw();
                if fresh {
                    (self.action_cb)(CandidateAction::CompanionSelectScene(scene));
                }
            }
            CompanionHit::MoreScenes => {
                self.update_view(|view| view.scene_menu_open = !view.scene_menu_open)
            }
            CompanionHit::DismissMenu => self.update_view(|view| view.scene_menu_open = false),
            CompanionHit::ToggleDetails => {
                self.update_view(|view| view.details_open = !view.details_open)
            }
            CompanionHit::Retry => {
                let (tab, scene) = {
                    let mut state = self.state.borrow_mut();
                    let tab = state.view.tab;
                    model::retry(&mut state.view, tab);
                    (tab, state.view.scene)
                };
                self.redraw();
                (self.action_cb)(CandidateAction::CompanionRetry { tab, scene });
            }
            CompanionHit::Save => (self.action_cb)(CandidateAction::CompanionSave),
            CompanionHit::Practice => (self.action_cb)(CandidateAction::CompanionPractice),
            CompanionHit::AskAi => (self.action_cb)(CandidateAction::OpenAiAssistant),
        }
    }

    fn update_view(&self, change: impl FnOnce(&mut qingjian_render::companion::CompanionView)) {
        change(&mut self.state.borrow_mut().view);
        self.redraw();
    }

    /// 朗读英文；系统没有美音就提示，不回退到别的口音。
    fn speak(&self, text: &str) {
        if let Err(error) = self.try_speak(text) {
            self.set_notice(error.notice().to_string(), false);
        }
    }

    fn try_speak(&self, text: &str) -> Result<(), SpeakError> {
        let mut state = self.state.borrow_mut();
        if state.speaker.is_none() {
            state.speaker = Some(Speaker::new()?);
        }
        match &state.speaker {
            Some(speaker) => speaker.speak(text),
            None => Ok(()),
        }
    }

    /// 滚动内容区 `delta` 点（正数向下），夹在 `0..=max_scroll`。
    fn scroll_by(&self, delta: f32) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let Some(max) = state.placed.as_ref().map(|placed| placed.max_scroll) else {
                return;
            };
            let next = (state.view.scroll + delta).clamp(0.0, max);
            let changed = (next - state.view.scroll).abs() > f32::EPSILON;
            state.view.scroll = next;
            changed
        };
        if changed {
            self.redraw();
        }
    }

    /// 拖动结束：记下新位置，之后每次打开都停在这里。
    fn remember_dragged_position(&self) {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(self.hwnd, &mut rect) }.is_err() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let Some(margin) = state.placed.as_ref().map(|placed| placed.margin) else {
            return;
        };
        let pos = (rect.left + margin, rect.top + margin);
        state.content_pos = Some(pos);
        state.user_pos = Some(pos);
    }

    fn on_timer(&self, id: usize) {
        match id {
            TIMER_NOTICE => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_NOTICE);
                }
                self.update_view(|view| view.notice = None);
            }
            TIMER_SPIN => {
                if model::is_busy(&self.state.borrow().view) {
                    self.update_view(|view| view.spin = (view.spin + SPIN_STEP) % 360.0);
                } else {
                    self.sync_spin(false);
                }
            }
            _ => {}
        }
    }
}
