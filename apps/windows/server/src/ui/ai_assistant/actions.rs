//! 面板上的点击与发送：点到什么就做什么，需要业务层配合的经 `action_cb` 回给 Router。

use qingjian_render::chat::ChatHit;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, SendMessageW, WM_NCLBUTTONDOWN};

use super::{AiAssistantWindow, model};
use crate::dispatch::CandidateAction;
use crate::ui::clipboard;

impl AiAssistantWindow {
    /// 客户区像素坐标处点到了什么。
    pub(super) fn hit_at(&self, x: i32, y: i32) -> Option<ChatHit> {
        let state = self.state.borrow();
        let placed = state.placed.as_ref()?;
        let px = (x - placed.margin) as f32 / placed.scale;
        let py = (y - placed.margin) as f32 / placed.scale;
        placed.hits.hit(px, py).copied()
    }

    pub(super) fn on_hit(&self, hit: ChatHit) {
        match hit {
            ChatHit::Drag => {
                if !self.state.borrow().view.maximized {
                    // 交给系统走原生拖动，拖完在 WM_EXITSIZEMOVE 里记位置。
                    unsafe {
                        let _ = ReleaseCapture();
                        let _ = SendMessageW(
                            self.hwnd,
                            WM_NCLBUTTONDOWN,
                            Some(WPARAM(HTCAPTION as usize)),
                            Some(LPARAM(0)),
                        );
                    }
                }
            }
            ChatHit::Close => self.close_and_notify(),
            ChatHit::Pin => self.toggle_pin(),
            ChatHit::Maximize => self.toggle_maximize(),
            ChatHit::NewChat => self.new_session(),
            ChatHit::Session(index) => self.switch_session(index),
            ChatHit::ClearChat => self.clear_chat(),
            ChatHit::Settings => crate::ui::open_settings(),
            ChatHit::Suggestion(index) => {
                let prompt = self.state.borrow().view.suggestions.get(index).cloned();
                if let Some(prompt) = prompt {
                    self.send_prompt(prompt);
                }
            }
            ChatHit::Input => unsafe {
                let _ = SetFocus(Some(self.hwnd));
            },
            ChatHit::ToggleWeb => self.toggle_web(),
            ChatHit::ToggleDeep => self.toggle_deep(),
            ChatHit::Send => self.submit_input(),
            ChatHit::Stop => self.stop(),
            ChatHit::CopyMessage(index) => self.copy_message(index),
            ChatHit::ToggleReasoning(index) => self.update_view(|view| {
                if let Some(message) = view.messages.get_mut(index) {
                    let open = !message.reasoning_open;
                    message.set_reasoning_open(open);
                }
            }),
        }
    }

    /// 发送输入框里的文字（Enter / 发送键）；空的、还在回答时都不发。
    pub(super) fn submit_input(&self) {
        let prompt = {
            let mut state = self.state.borrow_mut();
            let prompt = state.view.input.trim().to_string();
            if prompt.is_empty() || state.busy() {
                return;
            }
            state.view.input.clear();
            state.view.cursor = 0;
            prompt
        };
        self.send_prompt(prompt);
    }

    /// 本地先放上用户气泡、进入等待，再把问题交给业务层。还在回答时忽略。
    pub(super) fn send_prompt(&self, prompt: String) {
        let (deep, web, session_id) = {
            let mut state = self.state.borrow_mut();
            if state.busy() {
                return;
            }
            model::push_user(&mut state.view, &prompt);
            (state.view.deep, state.view.web, state.current.id.clone())
        };
        self.redraw();
        (self.action_cb)(CandidateAction::AiChatSend {
            prompt,
            deep,
            web_search: web,
            session_id,
        });
    }

    /// 停止这一轮回答：已经收到的内容留在屏幕上。
    pub(super) fn stop(&self) {
        let session_id = {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            if !state.busy() {
                return;
            }
            model::finish(&mut state.view, &mut state.pending_sources);
            state.current.id.clone()
        };
        self.redraw();
        (self.action_cb)(CandidateAction::AiChatStop { session_id });
    }

    fn switch_session(&self, index: usize) {
        let id = {
            let state = self.state.borrow();
            match state.session_ids.get(index) {
                Some(id) if *id != state.current.id => id.clone(),
                _ => return,
            }
        };
        (self.action_cb)(CandidateAction::AiChatSwitch { session_id: id });
    }

    /// 清空当前会话：本地先清屏，业务层随后推来整面状态。
    fn clear_chat(&self) {
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            model::clear_messages(&mut state.view, &mut state.pending_sources);
        }
        self.redraw();
        (self.action_cb)(CandidateAction::AiChatClear);
    }

    fn copy_message(&self, index: usize) {
        let text = self
            .state
            .borrow()
            .view
            .messages
            .get(index)
            .map(|message| message.text().to_string());
        let Some(text) = text else {
            return;
        };
        let notice = if clipboard::copy_text(&text) {
            "已复制"
        } else {
            "复制失败，剪贴板被占用"
        };
        self.set_notice(notice);
    }
}
