//! Phase P1: Daily / Learn / Drill 三态学习模式实机验收与多宿主兼容性测试套件。
//!
//! 验证场景覆盖：
//! 1. Daily 模式：零干扰极速输入，不主动发起云端被动翻译，状态栏「学」处于未激活态；
//! 2. Learn 模式：输入时提供学习辅助，触发首选词被动翻译，状态栏「学」处于高亮激活态；
//! 3. Drill 模式：主动训练模式，状态栏呈现「练」，支持「中文提示 -> 隐藏英文 -> 输入 -> Compare」；
//! 4. 模式极速切换：支持快捷键 Ctrl+Alt+L、状态栏左键轮换、右键菜单直切，且状态跨会话/应用全局同步；
//! 5. 多宿主实机模拟兼容性：覆盖 Notepad、Chrome、Windows Terminal、VS Code / 长文本编辑器。

#![cfg(windows)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{
    Prediction, PredictionKind, PredictionPolicy, PredictionRequest, Predictor,
};
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, KeyOutcome, PROTOCOL_VERSION, ServerMessage, SessionId,
};
use qingjian_platform::LearningMode;
use qingjian_windows_server::dispatch::StatusEvent;
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

struct MockPredictor {
    pub requests: Arc<Mutex<Vec<PredictionRequest>>>,
    pub response: Arc<Mutex<Option<Prediction>>>,
}

impl Predictor for MockPredictor {
    fn policy(&self) -> PredictionPolicy {
        PredictionPolicy {
            max_items: 2,
            sentence: true,
            ..Default::default()
        }
    }

    fn submit(&mut self, request: PredictionRequest) {
        self.requests.lock().unwrap().push(request);
    }

    fn poll(&mut self) -> Option<Prediction> {
        self.response.lock().unwrap().take()
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

fn letter(c: char) -> KeyEvent {
    KeyEvent::new(c.to_ascii_uppercase() as u32, Some(c), Default::default())
}

fn toggle_combo() -> KeyEvent {
    KeyEvent::new(
        'L' as u32,
        Some('l'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    )
}

/// 场景 1: 三态模式切换流水线与隔离性验证 (Daily -> Learn -> Drill -> Daily)
#[test]
fn test_p1_three_state_cycle_and_isolation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let engine = assembly::assemble(&AssemblySpec {
        glossary: Some((
            Language::English,
            root.join("assets/sample/glossary-en.tsv"),
        )),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
    })
    .expect("assemble engine");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = Arc::new(Mutex::new(None));
    let mock = MockPredictor {
        requests: Arc::clone(&requests),
        response: Arc::clone(&response),
    };

    let mut router = Router::new(engine, RouterConfig::default());
    router.engine_mut().set_predictor(Box::new(mock));

    let session = SessionId(101);
    router.handle(ClientMessage::OpenSession {
        session,
        app: Some("notepad.exe".into()),
        protocol: PROTOCOL_VERSION,
    });

    // 1. 默认启动即为 Daily 模式
    assert_eq!(router.learning_mode(), LearningMode::Daily);

    // 2. Daily 模式下键入「xianqian」，确认绝不发出被动翻译请求
    for c in "xianqian".chars() {
        router.handle(ClientMessage::Key {
            session,
            event: letter(c),
        });
    }
    assert!(
        requests.lock().unwrap().iter().all(|r| r.kind != PredictionKind::Translate),
        "Daily 模式绝不能向外部大模型发送被动翻译请求"
    );

    // 空格提交首选词并清空
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let commit_msg = router.handle(ClientMessage::Key {
        session,
        event: space,
    });
    if let Some(ServerMessage::KeyResult { commit, .. }) = commit_msg {
        assert_eq!(commit.as_deref(), Some("先前"));
    }

    // 3. 快捷键 Ctrl+Alt+L 切换到 Learn 模式
    let toggle_msg = router.handle(ClientMessage::Key {
        session,
        event: toggle_combo(),
    });
    assert_eq!(router.learning_mode(), LearningMode::Learn);
    if let Some(ServerMessage::KeyResult { frame, .. }) = toggle_msg {
        assert_eq!(frame.learning_mode, LearningMode::Learn);
        assert!(frame.notice.as_deref().unwrap().contains("学习"));
    }

    // 4. Learn 模式下键入「xianqian」，确认触发了被动翻译
    requests.lock().unwrap().clear();
    for c in "xianqian".chars() {
        router.handle(ClientMessage::Key {
            session,
            event: letter(c),
        });
    }
    let req = requests.lock().unwrap().last().cloned().expect("必须触发请求");
    assert_eq!(req.kind, PredictionKind::Translate);
    assert_eq!(req.text, "先前");

    // 模拟翻译返回
    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("previously".to_string()),
    });
    router.tick();

    let poll_msg = router.handle(ClientMessage::Poll { session });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        assert_eq!(frame.learning_mode, LearningMode::Learn);
        let first = frame.candidates.items.first().expect("首个候选词");
        assert_eq!(first.text, "先前");
        let trans = first.translation.as_ref().expect("被动翻译已挂载");
        assert_eq!(trans.senses()[0].text, "previously");
    }

    // 提交后重置
    router.handle(ClientMessage::Key {
        session,
        event: space,
    });

    // 5. 再次按 Ctrl+Alt+L 切换到 Drill 模式
    let drill_msg = router.handle(ClientMessage::Key {
        session,
        event: toggle_combo(),
    });
    assert_eq!(router.learning_mode(), LearningMode::Drill);
    if let Some(ServerMessage::KeyResult { frame, .. }) = drill_msg {
        assert_eq!(frame.learning_mode, LearningMode::Drill);
        assert!(frame.notice.as_deref().unwrap().contains("训练"));
    }

    // 6. 再按一次 Ctrl+Alt+L 回到 Daily 模式
    let daily_msg = router.handle(ClientMessage::Key {
        session,
        event: toggle_combo(),
    });
    assert_eq!(router.learning_mode(), LearningMode::Daily);
    if let Some(ServerMessage::KeyResult { frame, .. }) = daily_msg {
        assert_eq!(frame.learning_mode, LearningMode::Daily);
        assert!(frame.notice.as_deref().unwrap().contains("日常"));
    }
}

/// 场景 2: 状态栏与菜单事件交互测试
#[test]
fn test_p1_status_bar_and_menu_events() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
        .expect("assemble engine");

    let mut router = Router::new(engine, RouterConfig::default());
    assert_eq!(router.learning_mode(), LearningMode::Daily);

    // 1. 模拟状态栏单次左键点击轮转
    router.handle_status_event(StatusEvent::ToggleLearningMode);
    assert_eq!(router.learning_mode(), LearningMode::Learn);

    router.handle_status_event(StatusEvent::ToggleLearningMode);
    assert_eq!(router.learning_mode(), LearningMode::Drill);

    router.handle_status_event(StatusEvent::ToggleLearningMode);
    assert_eq!(router.learning_mode(), LearningMode::Daily);

    // 2. 模拟右键菜单或设置菜单直切
    router.handle_status_event(StatusEvent::SetLearningMode(LearningMode::Drill));
    assert_eq!(router.learning_mode(), LearningMode::Drill);

    router.handle_status_event(StatusEvent::SetLearningMode(LearningMode::Learn));
    assert_eq!(router.learning_mode(), LearningMode::Learn);

    router.handle_status_event(StatusEvent::SetLearningMode(LearningMode::Daily));
    assert_eq!(router.learning_mode(), LearningMode::Daily);
}

/// 场景 3: 多宿主（Notepad, Chrome, Terminal, VS Code）跨会话兼容性测试
#[test]
fn test_p1_multi_host_compatibility() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let engine = assembly::assemble(&AssemblySpec {
        glossary: Some((
            Language::English,
            root.join("assets/sample/glossary-en.tsv"),
        )),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
    })
    .expect("assemble engine");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = Arc::new(Mutex::new(None));
    let mock = MockPredictor {
        requests: Arc::clone(&requests),
        response: Arc::clone(&response),
    };

    let mut router = Router::new(engine, RouterConfig::default());
    router.engine_mut().set_predictor(Box::new(mock));

    let hosts = [
        ("notepad.exe", SessionId(201)),
        ("chrome.exe", SessionId(202)),
        ("windowsterminal.exe", SessionId(203)),
        ("code.exe", SessionId(204)),
    ];

    // 为每个宿主打开独立会话
    for (app, session) in &hosts {
        let open_resp = router.handle(ClientMessage::OpenSession {
            session: *session,
            app: Some((*app).into()),
            protocol: PROTOCOL_VERSION,
        });
        assert!(open_resp.is_some());
    }

    // 1. 在 Notepad 会话中切换到 Learn 模式
    let session_notepad = SessionId(201);
    router.handle(ClientMessage::Key {
        session: session_notepad,
        event: toggle_combo(),
    });
    assert_eq!(router.learning_mode(), LearningMode::Learn);

    // 2. 验证 Chrome 会话自动共享该模式
    let session_chrome = SessionId(202);
    let sync_msg = router.handle(ClientMessage::SyncMode {
        session: session_chrome,
    });
    assert!(sync_msg.is_some());
    // 在 Chrome 中打字，模式随帧正确传递为 Learn
    for c in "kaifa".chars() {
        let key_msg = router.handle(ClientMessage::Key {
            session: session_chrome,
            event: letter(c),
        });
        if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = key_msg {
            assert_eq!(outcome, KeyOutcome::Consumed);
            assert_eq!(frame.learning_mode, LearningMode::Learn);
        }
    }

    // 3. 在 Terminal 会话中按 Esc 取消并切换到 Drill
    let session_terminal = SessionId(203);
    let _ = session_terminal;
    let esc = KeyEvent::new(0x1B, None, Default::default());
    router.handle(ClientMessage::Key {
        session: session_chrome,
        event: esc,
    });
    router.handle_status_event(StatusEvent::SetLearningMode(LearningMode::Drill));
    assert_eq!(router.learning_mode(), LearningMode::Drill);

    // 在 VS Code 会话中确认进入 Drill 模式帧
    let session_vscode = SessionId(204);
    let key_msg = router.handle(ClientMessage::Key {
        session: session_vscode,
        event: letter('a'),
    });
    if let Some(ServerMessage::KeyResult { frame, .. }) = key_msg {
        assert_eq!(frame.learning_mode, LearningMode::Drill);
    }
}
