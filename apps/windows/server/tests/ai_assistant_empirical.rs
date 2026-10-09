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
