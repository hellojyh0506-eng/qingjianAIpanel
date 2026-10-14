//! AI 问答面板的窗口过程：鼠标、滚轮、键盘输入、焦点、拖动与定时器。

use std::panic::{AssertUnwindSafe, catch_unwind};

use qingjian_render::chat::{ChatHit, ChatView, Phase, SIDEBAR_WIDTH};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowRect, HTCLIENT, IsIconic, KillTimer, MA_ACTIVATE, WM_CHAR, WM_CLOSE,
    WM_DESTROY, WM_EXITSIZEMOVE, WM_IME_COMPOSITION, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL,
    WM_MOVE, WM_NCHITTEST, WM_SETFOCUS, WM_SIZE, WM_TIMER,
};

use super::{
    AiAssistantWindow, SPIN_STEP, TIMER_CARET, TIMER_FLUSH, TIMER_NOTICE, TIMER_SPIN, WHEEL_STEP,
    WINDOWS, input, window_of,
};
use crate::ui::clipboard;

pub(super) unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // 面板出错不能带倒宿主的 UI 线程（候选窗、状态条都在这条线程上）。
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        wndproc_inner(hwnd, msg, wparam, lparam)
    })) {
        Ok(result) => result,
        Err(_) => {
            tracing::error!("AI 问答窗口过程 panic，已拦下以保护宿主 UI 线程");
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

unsafe fn wndproc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if let Some(window) = window_of(hwnd) {
                let (x, y) = point_of(lparam);
                match window.hit_at(x, y) {
                    Some(ChatHit::Drag) if msg == WM_LBUTTONDBLCLK => window.toggle_maximize(),
                    Some(hit) => window.on_hit(hit),
                    None => {}
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(window) = window_of(hwnd) {
                let notches = (wparam.0 >> 16) as i16 as f32 / 120.0;
                let (screen_x, _) = point_of(lparam);
                window.wheel(screen_x, -notches * WHEEL_STEP);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(window) = window_of(hwnd) {
                window.on_char(wparam.0 as u16);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => match window_of(hwnd) {
            Some(window) if window.on_key(wparam.0 as u16) => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        },
        WM_IME_SETCONTEXT | WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION => {
            if let Some(window) = window_of(hwnd) {
                window.anchor_ime();
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETFOCUS => {
            if let Some(window) = window_of(hwnd) {
                window.on_focus_changed(true);
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            if let Some(window) = window_of(hwnd) {
                window.on_focus_changed(false);
            }
            LRESULT(0)
        }
        WM_MOVE | WM_EXITSIZEMOVE => {
            if let Some(window) = window_of(hwnd) {
                window.remember_position();
            }
            if msg == WM_EXITSIZEMOVE {
                crate::ui::wake_ui_thread();
            }
            LRESULT(0)
        }
        WM_SIZE => {
            // 从最小化还原：补画最小化期间攒下的内容。
            if let Some(window) = window_of(hwnd)
                && window.is_dirty()
            {
                window.redraw();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(window) = window_of(hwnd) {
                window.on_timer(wparam.0);
            }
            LRESULT(0)
        }
        // 任务栏 / Alt+F4 关闭：和面板上的关闭键一样，通知业务层打断回答。
        WM_CLOSE => {
            if let Some(window) = window_of(hwnd) {
                window.close_and_notify();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            WINDOWS.with(|map| map.borrow_mut().remove(&(hwnd.0 as isize)));
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// `lparam` 里打包的 `(x, y)`，各 16 位有符号。
fn point_of(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xffff) as i16 as i32,
        ((lparam.0 >> 16) & 0xffff) as i16 as i32,
    )
}

fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 }
}

impl AiAssistantWindow {
    fn is_dirty(&self) -> bool {
        self.state.try_borrow().is_ok_and(|state| state.dirty)
    }

    /// WM_CHAR：一个 UTF-16 码元。
    fn on_char(&self, unit: u16) {
        match unit {
            0x08 => self.edit(|view| input::backspace(&mut view.input, &mut view.cursor)),
            0x0D => {
                if shift_down() {
                    self.insert("\n");
                } else {
                    self.submit_input();
                }
            }
            0x1B => self.close_and_notify(),
            // Ctrl+V
            0x16 => {
                if let Some(text) = clipboard::paste_text() {
                    self.insert(&text);
                }
            }
            0..0x20 => {}
            _ => {
                if let Some(text) = self.decode(unit) {
                    self.insert(&text);
                }
            }
        }
    }

    /// 把 UTF-16 码元拼成字符；代理项对（emoji）分两次到，先记住前半。
    fn decode(&self, unit: u16) -> Option<String> {
        let mut state = self.state.borrow_mut();
        let units = match (state.high_surrogate.take(), unit) {
            (Some(high), low) if (0xDC00..0xE000).contains(&low) => vec![high, low],
            (_, high) if (0xD800..0xDC00).contains(&high) => {
                state.high_surrogate = Some(high);
                return None;
            }
            (_, other) => vec![other],
        };
        String::from_utf16(&units).ok()
    }

    fn insert(&self, text: &str) {
        self.edit(|view| {
            let before = view.input.len();
            input::insert(&mut view.input, &mut view.cursor, text);
            view.input.len() != before
        });
    }

    /// 改输入框；改动成功就让光标亮起并重画。
    fn edit(&self, change: impl FnOnce(&mut ChatView) -> bool) {
        let changed = change(&mut self.state.borrow_mut().view);
        if changed {
            self.restart_caret_blink();
            self.redraw();
        }
    }

    /// WM_KEYDOWN：光标移动与 Delete；处理了返回 `true`。
    fn on_key(&self, key: u16) -> bool {
        match key {
            k if k == VK_LEFT.0 => self.edit_cursor(input::left),
            k if k == VK_RIGHT.0 => self.edit_cursor(input::right),
            k if k == VK_HOME.0 => self.edit_cursor(input::home),
            k if k == VK_END.0 => self.edit_cursor(input::end),
            k if k == VK_DELETE.0 => {
                self.edit(|view| input::delete(&mut view.input, view.cursor));
                true
            }
            _ => false,
        }
    }

    fn edit_cursor(&self, step: fn(&str, usize) -> usize) -> bool {
        self.edit(|view| {
            let next = step(&view.input, view.cursor);
            let moved = next != view.cursor;
            view.cursor = next;
            moved
        });
        true
    }

    /// 滚轮：落在侧栏就滚会话列表，否则滚消息区。`screen_x` 是屏幕物理像素。
    fn wheel(&self, screen_x: i32, delta: f32) {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(self.hwnd, &mut rect) }.is_err() {
            return;
        }
        let changed = {
            let mut state = self.state.borrow_mut();
            let Some(placed) = state.placed.as_ref() else {
                return;
            };
            let x = (screen_x - rect.left - placed.margin) as f32 / placed.scale;
            if x < SIDEBAR_WIDTH {
                let max = placed.sidebar_max_scroll;
                let next = (state.view.sidebar_scroll + delta).clamp(0.0, max);
                let changed = (next - state.view.sidebar_scroll).abs() > f32::EPSILON;
                state.view.sidebar_scroll = next;
                changed
            } else {
                let max = placed.max_scroll;
                let next = (state.view.scroll + delta).clamp(0.0, max);
                let changed = (next - state.view.scroll).abs() > f32::EPSILON;
                state.view.scroll = next;
                // 往上滚就不再自动贴底，滚回底部才恢复。
                state.view.follow_bottom = next >= max - 0.5;
                changed
            }
        };
        if changed {
            self.redraw();
        }
    }

    /// 拖动结束：记下新位置，之后每次打开都停在这里。系统贴图时同步发来的 WM_MOVE 重入就跳过。
    fn remember_position(&self) {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return;
        };
        if !state.visible || state.view.maximized || unsafe { IsIconic(self.hwnd) }.as_bool() {
            return;
        }
        let Some(margin) = state.placed.as_ref().map(|placed| placed.margin) else {
            return;
        };
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(self.hwnd, &mut rect) }.is_err() {
            return;
        }
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
                let waiting = self.state.borrow().view.phase == Phase::Waiting;
                if waiting {
                    self.update_view(|view| view.spin = (view.spin + SPIN_STEP) % 360.0);
                } else {
                    unsafe {
                        let _ = KillTimer(Some(self.hwnd), TIMER_SPIN);
                    }
                    self.state.borrow_mut().spinning = false;
                }
            }
            TIMER_CARET => self.blink_caret(),
            TIMER_FLUSH => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_FLUSH);
                }
                if self.is_dirty() {
                    self.redraw();
                }
            }
            _ => {}
        }
    }
}
