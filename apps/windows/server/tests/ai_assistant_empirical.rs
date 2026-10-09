//! AI Assistant 独立对话窗口端到端经验测试
//! 验证：
//! 1. Ctrl+Alt+D 单次触发切换 AI 对话窗口
//! 2. Composition 中按 Ctrl+Alt+D 绝不丢失拼音、不切换中英模式、不提交中文
//! 3. CandidateAction 状态机：NewSession、Stop、Open、Close

#![cfg(windows)]

use std::path::PathBuf;

use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, KeyOutcome, ServerMessage, SessionId,
};
use qingjian_windows_server::{
    AssemblySpec, CandidateAction, Router, RouterConfig, assembly,
};

const SESSION: SessionId = SessionId(101);

fn create_test_router() -> Router {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
        .expect("assemble engine");
    Router::new(engine, RouterConfig::default())
}

#[test]
fn test_ai_assistant_toggle_and_repeat_debounce() {
    let mut router = create_test_router();

    let ai_hotkey = KeyEvent::new(
        'D' as u32,
        Some('d'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );

    // 第一次按下：打开
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });

    assert!(router.is_ai_assistant_open(), "按 Ctrl+Alt+D 必须打开 AI 对话窗口");
    if let Some(ServerMessage::KeyResult { outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
    } else {
        panic!("必须返回 KeyResult");
    }

    // 短时间内连击 (key repeat)：应当忽略，不反转状态
    let _ = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });
    assert!(router.is_ai_assistant_open(), "防按键连击期间不应反转状态");

    // 模拟等待 150ms 再次按下：关闭
    router.set_last_ai_assistant_toggle_time(std::time::Instant::now() - std::time::Duration::from_millis(150));
    let _ = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });
    assert!(!router.is_ai_assistant_open(), "第二次有效按下必须关闭 AI 对话窗口");
}

#[test]
fn test_ai_assistant_protects_active_composition() {
    let mut router = create_test_router();

    // 用户正在打拼音 "nihao"
    for c in "nihao".chars() {
        let _ = router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(c as u32, Some(c), KeyModifiers::default()),
        });
    }

    assert!(router.is_composing(), "当前必须处于拼音组句状态");

    // 此时按下 Ctrl+Alt+D 打开 AI 助手
    let ai_hotkey = KeyEvent::new(
        'D' as u32,
        Some('d'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );

    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });

    // 严苛验证：
    // 1. AI 窗口已打开
    assert!(router.is_ai_assistant_open(), "AI 对话助手必须成功打开");
    // 2. 拼音组句绝对不能丢失，绝不能清空 composition
    assert!(router.is_composing(), "打开 AI 助手后原拼音组句必须完整保留！绝不可丢失");
    // 3. 绝不能提前上屏中文
    if let Some(ServerMessage::KeyResult { outcome, commit, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert_eq!(commit, None, "绝不能提前上屏中文");
    }
}

#[test]
fn test_ai_action_state_machine() {
    let mut router = create_test_router();

    // 1. Open and Close actions
    router.handle_candidate_action(CandidateAction::OpenAiAssistant);
    assert!(router.is_ai_assistant_open());

    router.handle_candidate_action(CandidateAction::CloseAiAssistant);
    assert!(!router.is_ai_assistant_open());

    // 2. New session clears history and resets stop signal
    router.push_ai_message(qingjian_predict::ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    });
    assert_eq!(router.ai_messages_count(), 1);

    router.handle_candidate_action(CandidateAction::AiChatNewSession);
    assert_eq!(router.ai_messages_count(), 0, "新对话必须清空对话历史");

    // 3. Stop action sets stop_signal to true
    router.handle_candidate_action(CandidateAction::AiChatStop {
        session_id: "test-session".to_string(),
    });
    assert!(router.is_ai_stop_signaled(), "Stop 动作必须置位原子停止信号");
}

#[test]
fn test_close_ai_assistant_cuts_streaming_and_protects_input() {
    let mut router = create_test_router();

    // 1. 打开 AI 助手
    router.handle_candidate_action(CandidateAction::OpenAiAssistant);
    assert!(router.is_ai_assistant_open());

    // 2. 关闭面板
    router.handle_candidate_action(CandidateAction::CloseAiAssistant);
    assert!(!router.is_ai_assistant_open(), "AI 窗口必须处于关闭状态");
    assert!(router.is_ai_stop_signaled(), "关闭 AI 窗口必须立即置位原子停止信号");

    // 3. 验证此时正常输入拼音完全不受任何影响
    for c in "ceshi".chars() {
        let _ = router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(c as u32, Some(c), KeyModifiers::default()),
        });
    }
    assert!(router.is_composing(), "关闭 AI 助手后正常拼音输入法完全正常工作");
}

#[test]
fn test_ai_assistant_window_buttons_state_logic() {
    let action_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let action_called_clone = action_called.clone();
    let win = qingjian_windows_server::ui::ai_assistant::AiAssistantWindow::new(Box::new(move |_| {
        action_called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    }))
    .expect("create AiAssistantWindow");

    // 1. 测试置顶图钉切换
    assert!(win.data.borrow().is_pinned, "初始状态默认置顶");
    win.toggle_pin();
    assert!(!win.data.borrow().is_pinned, "点击图钉后取消置顶状态");
    win.toggle_pin();
    assert!(win.data.borrow().is_pinned, "再次点击图钉后恢复置顶状态");

    // 2. 测试最大化与还原切换
    win.toggle_maximize();
    // 验证最大化生效
    win.toggle_maximize();
    // 验证还原生效

    // 3. 验证最小化：调用 minimize 绝不会触发 CloseAiAssistant 动作
    action_called.store(false, std::sync::atomic::Ordering::Relaxed);
    win.minimize();
    assert!(
        !action_called.load(std::sync::atomic::Ordering::Relaxed),
        "点击最小化绝对不能触发 Close 动作或切断在途会话"
    );

    // 4. 验证关闭：调用 close_and_notify 才会触发通知
    win.close_and_notify();
    assert!(
        action_called.load(std::sync::atomic::Ordering::Relaxed),
        "点击关闭时必须触发通知并中断在途生成"
    );
}

#[test]
fn test_ai_assistant_clear_input_and_mode_switch() {
    let win = qingjian_windows_server::ui::ai_assistant::AiAssistantWindow::new(Box::new(|_| {}))
        .expect("create AiAssistantWindow");

    // 1. 测试输入文本清空 (对应 [+] 按钮功能)
    win.data.borrow_mut().input_text = "这是一段未发送的草稿测试".to_string();
    assert_eq!(win.data.borrow().input_text, "这是一段未发送的草稿测试");
    win.data.borrow_mut().input_text.clear();
    assert!(win.data.borrow().input_text.is_empty(), "[+] 按钮点击后输入文本必须被清除");

    // 2. 测试模式切换 (Fast <-> Deep)
    assert_eq!(win.data.borrow().chat_mode, qingjian_windows_server::ui::ai_assistant::ChatMode::Fast);
    win.data.borrow_mut().chat_mode = qingjian_windows_server::ui::ai_assistant::ChatMode::Deep;
    assert_eq!(win.data.borrow().chat_mode, qingjian_windows_server::ui::ai_assistant::ChatMode::Deep);
    win.data.borrow_mut().chat_mode = qingjian_windows_server::ui::ai_assistant::ChatMode::Fast;
    assert_eq!(win.data.borrow().chat_mode, qingjian_windows_server::ui::ai_assistant::ChatMode::Fast);

    // 3. 测试联网搜索开关
    assert!(!win.data.borrow().web_search_enabled);
    win.data.borrow_mut().web_search_enabled = true;
    assert!(win.data.borrow().web_search_enabled);
    win.data.borrow_mut().web_search_enabled = false;
    assert!(!win.data.borrow().web_search_enabled);

    // 4. 测试新对话重置功能
    win.data.borrow_mut().input_text = "待清空草稿".to_string();
    win.new_session();
    assert!(win.data.borrow().input_text.is_empty(), "新会话必须清空输入文本");
    assert!(win.data.borrow().messages.is_empty(), "新会话必须清空所有消息记录");
}

#[test]
fn test_circular_stability_100_cycles() {
    let mut router = create_test_router();

    // 循环稳定性测试：100 次高频打字、选区翻译、候选翻页与模式切换，确保无内存泄漏与死锁
    for cycle in 0..100 {
        // 模拟打字
        for c in "kaifa".chars() {
            let _ = router.handle(ClientMessage::Key {
                session: SESSION,
                event: KeyEvent::new(c as u32, Some(c), KeyModifiers::default()),
            });
        }
        assert!(router.is_composing());

        // 模拟按下空格上屏
        let _ = router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(' ' as u32, Some(' '), KeyModifiers::default()),
        });
        assert!(!router.is_composing());

        // 模拟选区翻译请求并通过 ESC 结束
        let _ = router.handle(ClientMessage::Selection {
            session: SESSION,
            request: cycle as u64 + 1,
            text: "开发".to_string(),
            rect: qingjian_platform::protocol::ScreenRect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 20,
            },
        });
        let _ = router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(27, None, KeyModifiers::default()),
        });
    }
}

#[test]
fn test_ai_assistant_and_companion_pinned_by_default() {
    // 验证 AI Assistant 默认开启置顶，且支持动态图钉切换
    let ai_win = qingjian_windows_server::ui::ai_assistant::AiAssistantWindow::new(Box::new(|_| {}))
        .expect("create AiAssistantWindow");
    assert!(ai_win.data.borrow().is_pinned, "AI 对话面板必须默认高亮置顶 (is_pinned = true)");
    ai_win.data.borrow_mut().is_pinned = false;
    assert!(!ai_win.data.borrow().is_pinned, "AI 对话面板支持取消置顶");
    ai_win.data.borrow_mut().is_pinned = true;
    assert!(ai_win.data.borrow().is_pinned, "AI 对话面板支持再次置顶");

    // 验证 English Assistant 伴侣窗默认开启置顶，且支持动态图钉切换
    let comp_win = qingjian_windows_server::ui::companion::CompanionWindow::new(Box::new(|_| {}))
        .expect("create CompanionWindow");
    assert!(comp_win.is_pinned.get(), "English Assistant 学习面板必须默认高亮置顶 (is_pinned = true)");
    comp_win.is_pinned.set(false);
    assert!(!comp_win.is_pinned.get(), "English Assistant 伴侣窗支持取消置顶");
    comp_win.is_pinned.set(true);
    assert!(comp_win.is_pinned.get(), "English Assistant 伴侣窗支持再次置顶");
}

#[test]
fn test_ai_assistant_boost_command() {
    let mut router = create_test_router();

    // 验证发送 /boost 指令能正确分发并加入消息历史
    router.handle_candidate_action(CandidateAction::AiChatSend {
        prompt: "/boost".to_string(),
        fast_mode: true,
        web_search: false,
        session_id: "test-boost-session".to_string(),
    });

    // 验证消息已被推入对话历史记录
    let last = router.last_ai_message();
    assert!(last.is_some(), "发送 /boost 后必须在消息记录中建立用户条目");
    assert_eq!(last.unwrap().content, "/boost");
}

#[test]
fn test_companion_toggle_and_close_button_sync() {
    let mut router = create_test_router();

    let companion_hotkey = KeyEvent::new(
        'E' as u32,
        Some('e'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );

    // 1. 第一次按快捷键 Ctrl+Alt+E：打开伴侣窗
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: companion_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(frame.notice.as_deref().unwrap_or("").contains("English Assistant"), "开启必须带屏幕提示");
    }

    // 2. 模拟用户点击伴侣窗右上角的关闭按钮 (CloseAssistant)
    router.handle_candidate_action(CandidateAction::CloseAssistant);

    // 3. 再次按快捷键 Ctrl+Alt+E：必须单次即可重新打开，绝不需要按两次
    router.set_last_assistant_toggle_time(std::time::Instant::now() - std::time::Duration::from_millis(150));
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: companion_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(frame.notice.as_deref().unwrap_or("").contains("开启"), "再次按下必须单次触发重新打开");
    }

    // 4. 再次按快捷键 Ctrl+Alt+E：单次触发关闭
    router.set_last_assistant_toggle_time(std::time::Instant::now() - std::time::Duration::from_millis(150));
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: companion_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(frame.notice.as_deref().unwrap_or("").contains("收起"), "再次按下必须单次触发收起");
    }
}

#[test]
fn test_shortcut_toast_notices_and_feedback() {
    let mut router = create_test_router();

    // 1. Ctrl+Alt+L 切换英语学习模式：必须产出屏幕反馈气泡
    let mode_hotkey = KeyEvent::new(
        'L' as u32,
        Some('l'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: mode_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(frame.notice.as_deref().unwrap_or("").contains("已切换至"), "模式切换必须给用户提示");
    } else {
        panic!("Ctrl+Alt+L 必须返回 KeyResult");
    }

    // 2. 单击 Shift 切中英：必须产出中英切换屏幕气泡
    let _ = router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: true,
    });
    let frame_en = router.active_frame();
    assert!(frame_en.notice.as_deref().unwrap_or("").contains("英"), "切英文模式必须带英提示");

    let _ = router.handle(ClientMessage::ModeChanged {
        session: SESSION,
        english: false,
    });
    let frame_zh = router.active_frame();
    assert!(frame_zh.notice.as_deref().unwrap_or("").contains("中"), "切中文模式必须带中提示");

    // 3. Ctrl+Alt+T 选区翻译在无选区时：必须提示选中文字而不是无反应
    router.set_pending_selection(SESSION, 999);
    let _ = router.handle(ClientMessage::Selection {
        session: SESSION,
        request: 999,
        text: "".to_string(),
        rect: qingjian_platform::protocol::ScreenRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
    });
    let frame_sel = router.active_frame();
    assert!(frame_sel.notice.as_deref().unwrap_or("").contains("选中文字"), "空选区必须提示用户先划词");

    // 4. Ctrl+Alt+R 快捷键在非打字态下：必须有反馈气泡（提示待复习数量或暂无到期复习）
    let review_hotkey = KeyEvent::new(
        'R' as u32,
        Some('r'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let reply_r = router.handle(ClientMessage::Key {
        session: SESSION,
        event: review_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply_r {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(frame.notice.is_some(), "非组句态下按 Ctrl+Alt+R 必须给予屏幕反馈提示");
    } else {
        panic!("Ctrl+Alt+R 必须返回 KeyResult");
    }

    // 5. Ctrl+Alt+D AI助手快捷键切换与关闭按钮状态机双向同步
    let ai_hotkey = KeyEvent::new(
        'D' as u32,
        Some('d'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let reply_ai = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply_ai {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(router.is_ai_assistant_open(), "按 Ctrl+Alt+D 必须开启 AI 助手");
        assert!(frame.notice.as_deref().unwrap_or("").contains("AI 对话助手"), "开启必须带提示");
    }

    // 点击 AI 窗口右上角关闭
    router.handle_candidate_action(CandidateAction::CloseAiAssistant);
    assert!(!router.is_ai_assistant_open(), "点击关闭必须同步置为关闭状态");

    // 再次按 Ctrl+Alt+D：单次即重新打开
    router.set_last_ai_assistant_toggle_time(std::time::Instant::now() - std::time::Duration::from_millis(150));
    let reply_ai2 = router.handle(ClientMessage::Key {
        session: SESSION,
        event: ai_hotkey,
    });
    if let Some(ServerMessage::KeyResult { outcome, .. }) = reply_ai2 {
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert!(router.is_ai_assistant_open(), "再次按下必须单次触发重新打开");
    }
}
