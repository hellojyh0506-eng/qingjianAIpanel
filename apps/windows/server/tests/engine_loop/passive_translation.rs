use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{
    Prediction, PredictionKind, PredictionPolicy, PredictionRequest, Predictor,
};
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyOutcome, ServerMessage,
};

use crate::support::*;

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

fn router_with_mock_predictor() -> (Router, Arc<Mutex<Vec<PredictionRequest>>>, Arc<Mutex<Option<Prediction>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = Arc::new(Mutex::new(None));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let glossary = root.join("assets/sample/glossary-en.tsv");
    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, glossary)),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine from sample data");

    let mock = MockPredictor {
        requests: Arc::clone(&requests),
        response: Arc::clone(&response),
    };
    engine.set_predictor(Box::new(mock));

    let mut router = Router::new(
        engine,
        RouterConfig {
            learning_mode: qingjian_platform::LearningMode::Learn,
            ..RouterConfig::default()
        },
    );
    open_session(&mut router, SESSION, None);
    (router, requests, response)
}

#[test]
fn test_local_glossary_passive_translation_space_commits_chinese_only() {
    let mut router = router();
    let (outcome, commit, frame) = type_letters(&mut router, "kaifa");

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert_eq!(preedit(&frame), "kai'fa");

    let first = frame.candidates.items.first().expect("有首选");
    assert_eq!(first.text, "开发");
    let trans = first.translation.as_ref().expect("开发有本地释义");
    assert!(
        trans.senses().iter().any(|s| s.text.contains("develop")),
        "开发释义应含 develop: {:?}",
        trans.senses()
    );

    // Space 按键必须提交中文候选「开发」，绝不能提交英文释义
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("开发"), "Space 必须只上屏中文候选");
    assert!(after.is_empty(), "上屏后候选框收起");
}

#[test]
fn test_local_glossary_enter_commits_raw_pinyin_never_english() {
    let mut router = router();
    type_letters(&mut router, "kaifa");

    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));

    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("kaifa"), "Enter 必须提交生键，绝不能提交英文");
    assert!(after.is_empty());
}

#[test]
fn test_cloud_passive_translation_when_no_local_gloss_displayed_and_space_commits_chinese() {
    let (mut router, requests, response) = router_with_mock_predictor();

    // 敲「xianqian」，首选「先前」在 sample 词库中，但不在 sample glossary-en.tsv 中
    let (_, _, frame) = type_letters(&mut router, "xianqian");
    let first = frame.candidates.items.first().expect("有首选");
    assert_eq!(first.text, "先前");
    assert!(first.translation.is_none(), "无本地释义");

    // 检查是否发出了 Translate 类型的请求
    let last_req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    assert_eq!(last_req.kind, PredictionKind::Translate);
    assert_eq!(last_req.text, "先前");
    assert_eq!(last_req.target_language, "en");

    // 模拟云端返回被动翻译
    *response.lock().unwrap() = Some(Prediction {
        sequence: last_req.sequence,
        words: Vec::new(),
        sentence: Some("previously".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });

    // 触发 tick 轮询
    router.tick();

    // 检查轮询后候选帧：首选带上了被动翻译 annotation
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    let frame = match poll_msg {
        Some(ServerMessage::Update { frame, .. }) => frame,
        other => panic!("expected Update with frame, got {other:?}"),
    };
    let first = frame.candidates.items.first().expect("有首选");
    let trans = first.translation.as_ref().expect("被动翻译已挂上");
    assert_eq!(trans.senses()[0].text, "previously");

    // Space 依然只提交中文「先前」，绝不提交「previously」
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("先前"), "Space 提交中文");
    assert!(after.is_empty());
}

#[test]
fn test_ctrl_1_commits_passive_translation() {
    let (mut router, requests, response) = router_with_mock_predictor();

    type_letters(&mut router, "xianqian");
    let last_req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    *response.lock().unwrap() = Some(Prediction {
        sequence: last_req.sequence,
        words: Vec::new(),
        sentence: Some("beforehand".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    // 轮询收到被动翻译响应
    let _ = router.handle(ClientMessage::Poll { session: SESSION });

    // 按 Ctrl+1（缺省 TRANSLATE 快捷键）：主动选择上屏被动翻译
    let (outcome, commit, after) = press(&mut router, digit_with(1, TRANSLATE));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("beforehand"), "Ctrl+1 上屏被动翻译");
    assert!(after.is_empty());
}

#[test]
fn test_stale_response_discarded_on_new_typing() {
    let (mut router, requests, response) = router_with_mock_predictor();

    type_letters(&mut router, "xian");
    let old_req = requests.lock().unwrap().last().cloned().expect("第1次请求");

    // 用户继续输入「q」
    type_letters(&mut router, "q");
    let new_req = requests.lock().unwrap().last().cloned().expect("第2次请求");
    assert!(new_req.sequence > old_req.sequence);

    // 云端返回了旧请求的响应
    *response.lock().unwrap() = Some(Prediction {
        sequence: old_req.sequence,
        words: Vec::new(),
        sentence: Some("stale translation".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();

    // 验证旧结果被静默丢弃，当前帧没有挂上 stale translation
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        if let Some(first) = frame.candidates.items.first() {
            if let Some(trans) = &first.translation {
                assert_ne!(trans.senses()[0].text, "stale translation");
            }
        }
    }
}

#[test]
fn test_candidate_shift_discards_stale_target_translation() {
    let (mut router, requests, response) = router_with_mock_predictor();

    // 1. 用户输入 "xian"
    type_letters(&mut router, "xian");
    let req1 = requests.lock().unwrap().last().cloned().expect("第1次请求");

    // 2. 用户继续输入 "qian"，首选变成 "先前"
    type_letters(&mut router, "qian");
    let req2 = requests.lock().unwrap().last().cloned().expect("第2次请求");
    assert_ne!(req1.sequence, req2.sequence);

    // 3. 云端返回针对第1个请求的结果 (比如对 "先" 的翻译 "first")
    *response.lock().unwrap() = Some(Prediction {
        sequence: req1.sequence,
        words: Vec::new(),
        sentence: Some("first".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();

    // 4. Poll 之后验证当前候选 "先前" 绝不会被挂上 "first"
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        if let Some(first) = frame.candidates.items.first() {
            assert_eq!(first.text, "先前");
            if let Some(trans) = &first.translation {
                assert_ne!(
                    trans.senses()[0].text,
                    "first",
                    "当前首选词「先前」绝不能被旧候选「先」的翻译污染"
                );
            }
        }
    }
}

#[test]
fn test_learning_language_off_disables_all_passive_translation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let engine = assembly::assemble(&AssemblySpec::new(dict))
        .expect("assemble engine without glossary");

    let mut router = Router::new(
        engine,
        RouterConfig {
            learning_language_off: true,
            ..RouterConfig::default()
        },
    );
    open_session(&mut router, SESSION, None);

    let (_, _, frame) = type_letters(&mut router, "kaifa");
    let first = frame.candidates.items.first().expect("有首选");
    assert_eq!(first.text, "开发");
    assert!(first.translation.is_none(), "learning_language_off 时无释义");
}

#[test]
fn test_privacy_mode_disables_passive_translation() {
    let (mut router, requests, _) = router_with_mock_predictor();

    // 进入私密/密码模式
    router.handle(ClientMessage::Privacy {
        session: SESSION,
        private: true,
    });
    requests.lock().unwrap().clear();

    type_letters(&mut router, "xianqian");
    // 验证私密模式下不发出任何翻译请求
    assert!(
        requests.lock().unwrap().is_empty(),
        "私密输入期间不发任何云端翻译请求"
    );
}

#[test]
fn test_backspace_and_esc_clears_passive_translation() {
    let (mut router, requests, response) = router_with_mock_predictor();

    type_letters(&mut router, "xianqian");
    let req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("previously".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();

    // 退格
    let back = KeyEvent::new(0x08, None, Default::default());
    let (outcome, _, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: back,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(preedit(&after), "xian'qia");

    // Esc 取消
    let esc = KeyEvent::new(0x1B, None, Default::default());
    let (outcome, commit, after) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: esc,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(after.is_empty(), "Esc 清空组句");
}

#[test]
fn test_english_direct_mode_no_regression() {
    let mut router = router_with(RouterConfig {
        english_direct: true,
        english_candidates: false,
        ..RouterConfig::default()
    });

    // 验证 Server 经 SessionOpened 下发 english_direct = true
    let reply = router.handle(ClientMessage::OpenSession {
        session: SessionId(10),
        app: None,
        protocol: PROTOCOL_VERSION,
    });
    match reply {
        Some(ServerMessage::SessionOpened { input, .. }) => {
            assert!(input.english_direct, "english_direct = true 必须下发给 TSF DLL");
        }
        other => panic!("expected SessionOpened, got {other:?}"),
    }

    // 验证 ModeSync 也携带 english_direct = true
    let reply = router.handle(ClientMessage::SyncMode {
        session: SessionId(10),
    });
    match reply {
        Some(ServerMessage::ModeSync { input, .. }) => {
            assert!(input.english_direct, "ModeSync 必须下发 english_direct = true");
        }
        other => panic!("expected ModeSync, got {other:?}"),
    }
}

#[test]
fn test_chongddren_passive_translation_strictly_binds_to_candidate_text_never_surrounding_context() {
    let (mut router, requests, response) = router_with_mock_predictor();

    // 1. 模拟宿主应用（如 ChatGPT 输入框）向输入法汇报大量历史前文
    let historic_context = "他充电了吗？我不知道他有没有优化。你能提示她出来告诉我她在上面做了什么吗？翻译是什么来着？";
    router.handle(ClientMessage::Surrounding {
        session: SESSION,
        text: historic_context.to_string(),
    });

    // 2. 用户键入 "chongddren"
    type_letters(&mut router, "chongddren");

    // 3. 验证发出的云端翻译请求：
    //    - 目标文本必须是当前候选词 "充电的人"
    //    - 严格隔离：before 必须为空字符串，绝对不能包含历史前文
    let req = requests.lock().unwrap().last().cloned().expect("必须触发被动翻译请求");
    assert_eq!(req.kind, qingjian_core::PredictionKind::Translate);
    assert_eq!(req.text, "充电的人");
    assert!(
        req.before.is_empty(),
        "被动翻译请求的 before 必须完全为空，严防泄漏宿主历史上下文"
    );

    // 4. 模拟模型正常返回 "the person charging up"
    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("the person charging up".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();

    // 5. 验证展示给用户的候选窗口：
    //    - 首选为 "充电的人"
    //    - 标注为 "the person charging up"
    //    - 严禁出现历史长句
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        let first = frame.candidates.items.first().expect("首个候选词存在");
        assert_eq!(first.text, "充电的人");
        let trans = first.translation.as_ref().expect("必须挂载被动翻译");
        assert_eq!(trans.senses()[0].text, "the person charging up");
        assert!(
            !trans.senses()[0].text.contains("Did he finish charging"),
            "候选翻译严禁被历史上下文污染"
        );
    } else {
        panic!("expected Update message with frame");
    }

    // 6. 防御性测试：若外部模型由于某种原因返回了 > 100 字符的冗长串译句子，系统必须坚决丢弃
    let rogue_long_sentence = "Did he finish charging? I'm not sure if he optimized it. Can you give her a hint to come out and tell me what she did up there? What's the translation again?";
    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some(rogue_long_sentence.to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();
    // 再次 Poll，验证候选框绝不会挂上该异常长句
    let poll_rogue = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_rogue {
        if let Some(first) = frame.candidates.items.first() {
            if let Some(trans) = &first.translation {
                assert_ne!(
                    trans.senses()[0].text,
                    rogue_long_sentence,
                    "异常发散/串译长句必须被防发散规则丢弃"
                );
            }
        }
    }
}

#[test]
fn test_three_state_learning_mode_daily_learn_drill_switching_and_isolation() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = Arc::new(Mutex::new(None));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let glossary = root.join("assets/sample/glossary-en.tsv");
    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, glossary)),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine");

    let mock = MockPredictor {
        requests: Arc::clone(&requests),
        response: Arc::clone(&response),
    };
    engine.set_predictor(Box::new(mock));

    // 1. 默认 Daily 模式启动
    let mut router = Router::new(engine, RouterConfig::default());
    open_session(&mut router, SESSION, None);
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Daily);

    // 2. 在 Daily 模式下输入「xianqian」，确认触发快速地道英文单句翻译
    let (_, _, frame) = type_letters(&mut router, "xianqian");
    assert_eq!(frame.learning_mode, qingjian_platform::LearningMode::Daily);
    assert!(
        requests.lock().unwrap().iter().any(|r| r.kind == PredictionKind::Translate),
        "Daily 模式必须支持当前首选地道口语翻译"
    );

    // 3. 清空缓冲区
    router.handle(ClientMessage::Commit { session: SESSION });

    // 4. 使用快捷键 Ctrl+Alt+L 切换到 Learn 模式
    let toggle_key = KeyEvent::new(
        'L' as u32,
        Some('l'),
        qingjian_platform::protocol::KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let msg = router.handle(ClientMessage::Key {
        session: SESSION,
        event: toggle_key,
    });
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Learn);
    if let Some(ServerMessage::KeyResult { frame, .. }) = msg {
        assert_eq!(frame.learning_mode, qingjian_platform::LearningMode::Learn);
    }

    // 5. 在 Learn 模式下输入「xianqian」，确认触发了被动翻译请求
    requests.lock().unwrap().clear();
    let (_, _, frame) = type_letters(&mut router, "xianqian");
    assert_eq!(frame.learning_mode, qingjian_platform::LearningMode::Learn);
    assert!(
        requests.lock().unwrap().iter().any(|r| r.kind == PredictionKind::Translate),
        "Learn 模式必须触发当前首选被动翻译"
    );

    // 6. 清空缓冲区，再次按 Ctrl+Alt+L 切换到 Drill 模式
    router.handle(ClientMessage::Commit { session: SESSION });
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: toggle_key,
    });
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Drill);

    // 7. 再按一次 Ctrl+Alt+L 回到 Daily 模式
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: toggle_key,
    });
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Daily);

    // 8. 测试状态栏事件切换
    router.handle_status_event(qingjian_windows_server::dispatch::StatusEvent::ToggleLearningMode);
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Learn);

    router.handle_status_event(qingjian_windows_server::dispatch::StatusEvent::SetLearningMode(
        qingjian_platform::LearningMode::Drill,
    ));
    assert_eq!(router.learning_mode(), qingjian_platform::LearningMode::Drill);
}


