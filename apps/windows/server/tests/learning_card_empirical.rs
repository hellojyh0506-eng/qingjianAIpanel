use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{
    Prediction, PredictionAlternative, PredictionKind, PredictionPolicy, PredictionRequest,
    Predictor,
};
use qingjian_platform::LearningMode;
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, KeyOutcome, LearningState, ServerMessage, SessionId,
};
use qingjian_windows_server::dispatch::{Router, RouterConfig};
use qingjian_windows_server::{AssemblySpec, assembly};

const SESSION: SessionId = SessionId(1);

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

fn create_test_router(
    mode: LearningMode,
) -> (
    Router,
    Arc<Mutex<Vec<PredictionRequest>>>,
    Arc<Mutex<Option<Prediction>>>,
) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = Arc::new(Mutex::new(None));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let qj_dict = root.join("data/generated/dict.qj");
    let dict = if qj_dict.exists() {
        qj_dict
    } else {
        root.join("assets/lexicon/dict.tsv")
    };
    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, root.join("assets/sample/glossary-en.tsv"))),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine");

    let mock = MockPredictor {
        requests: Arc::clone(&requests),
        response: Arc::clone(&response),
    };
    engine.set_predictor(Box::new(mock));

    let mut router = Router::new(
        engine,
        RouterConfig {
            learning_mode: mode,
            ..RouterConfig::default()
        },
    );
    router.handle(ClientMessage::OpenSession {
        session: SESSION,
        app: None,
        protocol: qingjian_platform::protocol::PROTOCOL_VERSION,
    });
    (router, requests, response)
}

fn type_letters(router: &mut Router, letters: &str) {
    for c in letters.chars() {
        let event = KeyEvent::new(c as u32, Some(c), KeyModifiers::default());
        router.handle(ClientMessage::Key {
            session: SESSION,
            event,
        });
    }
}

/// Scenario 1: Standard Chinese
/// `ta'chong'dian'le'ma` -> 他充电了吗 -> Layer 3 Spoken American English + 2~5 alternatives
#[test]
fn test_scenario_01_standard_chinese_learning_card() {
    let (mut router, requests, response) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "tachongdianlema");

    let last_req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    assert_eq!(last_req.kind, PredictionKind::Translate);

    // 候选窗口此时处于 Loading 状态，Layer 1 & 2 中文候选完全正常
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        assert_eq!(frame.learning_mode, LearningMode::Learn);
        let card = frame.learning_card.expect("Learn 模式下必须挂载 learning_card");
        assert_eq!(card.state, LearningState::Loading);
        assert!(!card.target_text.is_empty());
    }

    // 模拟云端模型返回丰富口语备选
    *response.lock().unwrap() = Some(Prediction {
        sequence: last_req.sequence,
        words: Vec::new(),
        sentence: Some("Did he charge it?".to_string()),
        alternatives: vec![
            PredictionAlternative {
                text: "Did he charge it?".to_string(),
                tag: "Natural".to_string(),
                meaning: "他充电了吗".to_string(),
                tone: "Casual".to_string(),
                scenario: "Daily conversation".to_string(),
                commonality: "High".to_string(),
                why_natural: "Natural spoken English for checking charging state".to_string(),
            },
            PredictionAlternative {
                text: "Did he plug it in?".to_string(),
                tag: "Casual".to_string(),
                meaning: "他插上电充了吗".to_string(),
                tone: "Casual".to_string(),
                scenario: "Home or office".to_string(),
                commonality: "Very high".to_string(),
                why_natural: "Native speakers often say plug it in instead of charge".to_string(),
            },
            PredictionAlternative {
                text: "Is his phone charged?".to_string(),
                tag: "Idiomatic".to_string(),
                meaning: "他手机充好电了吗".to_string(),
                tone: "Casual".to_string(),
                scenario: "Asking about battery status".to_string(),
                commonality: "Common".to_string(),
                why_natural: "Focuses on result status rather than the action".to_string(),
            },
        ],
        ..Default::default()
    });
    router.tick();

    // 检查轮询后：状态变为 Ready，包含 Primary 与 3 个 alternatives
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        let card = frame.learning_card.expect("必须有 learning_card");
        assert_eq!(card.state, LearningState::Ready);
        let prim = card.primary.expect("必须有 primary 推荐");
        assert_eq!(prim.text, "Did he charge it?");
        assert_eq!(card.alternatives.len(), 3);
        assert_eq!(card.alternatives[1].tag, "Casual");
        assert_eq!(card.alternatives[1].text, "Did he plug it in?");
        assert!(!card.expanded, "默认未展开更多译法");
    }

    // Space 按键必须提交中文候选，绝不提交英文
    let space = KeyEvent::new(0x20, Some(' '), KeyModifiers::default());
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    });
    match reply {
        Some(ServerMessage::KeyResult { outcome, commit, .. }) => {
            assert_eq!(outcome, KeyOutcome::Consumed);
            let committed = commit.expect("必须上屏中文");
            assert!(
                !committed.chars().any(|c| c.is_ascii_alphabetic()),
                "Space 提交的必须是汉字，绝不能包含英文字符: {committed}"
            );
        }
        other => panic!("expected KeyResult, got {other:?}"),
    }
}

/// Scenario 2: Henan Dialect Spoken Input
/// `woritage` -> 我日他哥, `ni'er` -> 妮儿, `wo'lei'ge'dou` -> 我嘞个豆
#[test]
fn test_scenario_02_henan_dialect_spoken_learning_card() {
    let (mut router, requests, response) = create_test_router(LearningMode::Learn);

    // 1. 测试 "woritage"
    type_letters(&mut router, "woritage");
    let req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    assert_eq!(req.kind, PredictionKind::Translate);

    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("No freaking way!".to_string()),
        alternatives: vec![
            PredictionAlternative {
                text: "No freaking way!".to_string(),
                tag: "Slang".to_string(),
                meaning: "我日他哥/简直不可置信".to_string(),
                tone: "Emphatic".to_string(),
                scenario: "Emotional venting".to_string(),
                commonality: "High".to_string(),
                why_natural: "Natural Gen-Z emotional disbelief exclamation".to_string(),
            },
            PredictionAlternative {
                text: "Holy crap!".to_string(),
                tag: "Casual".to_string(),
                meaning: "我去/天呐".to_string(),
                tone: "Shocked".to_string(),
                scenario: "Extreme surprise".to_string(),
                commonality: "Very high".to_string(),
                why_natural: "Classic American colloquial exclamation".to_string(),
            },
        ],
        ..Default::default()
    });
    router.tick();

    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        let card = frame.learning_card.expect("必须有 learning_card");
        assert_eq!(card.state, LearningState::Ready);
        assert_eq!(card.primary.as_ref().unwrap().text, "No freaking way!");
        assert_eq!(card.alternatives[0].tag, "Slang");
    }
}

/// Scenario 3: Contextual Disambiguation Chinese
/// `jie'ou` -> 解耦, `w's'm` -> 为什么
#[test]
fn test_scenario_03_disambiguation_chinese_learning_card() {
    let (mut router, requests, response) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "jie'ou");
    let req = requests.lock().unwrap().last().cloned().expect("发出了请求");
    assert_eq!(req.kind, PredictionKind::Translate);
    assert_eq!(req.text, "解耦");

    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("decouple".to_string()),
        alternatives: vec![
            PredictionAlternative {
                text: "decouple".to_string(),
                tag: "Professional".to_string(),
                meaning: "解耦（架构设计）".to_string(),
                tone: "Technical".to_string(),
                scenario: "Software engineering discussion".to_string(),
                commonality: "Standard".to_string(),
                why_natural: "Standard industry technical terminology".to_string(),
            },
            PredictionAlternative {
                text: "disentangle".to_string(),
                tag: "Natural".to_string(),
                meaning: "理清/解开交缠".to_string(),
                tone: "Expressive".to_string(),
                scenario: "System complexity discussion".to_string(),
                commonality: "Moderate".to_string(),
                why_natural: "Vividly expresses separating tightly bound modules".to_string(),
            },
        ],
        ..Default::default()
    });
    router.tick();

    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        assert_eq!(frame.candidates.items[0].text, "解耦");
        let card = frame.learning_card.expect("必须有 learning_card");
        assert_eq!(card.target_text, "解耦");
        assert_eq!(card.primary.as_ref().unwrap().text, "decouple");
    }
}

/// Scenario 4: Highlight Shift & Stale Translation Rejection
#[test]
fn test_scenario_04_highlight_shift_stale_rejection() {
    let (mut router, requests, response) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "jie'ou");
    let req1 = requests.lock().unwrap().last().cloned().expect("第1个请求");
    assert_eq!(req1.text, "解耦");

    // 用户按方向键下移高亮到第 2 个候选词
    let down_arrow = KeyEvent::new(0x28, None, KeyModifiers::default());
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: down_arrow,
    });

    let poll_after_nav = router.handle(ClientMessage::Poll { session: SESSION });
    let current_highlight_target = if let Some(ServerMessage::Update { frame, .. }) = poll_after_nav {
        assert_eq!(frame.highlight, 1);
        let target = frame.candidates.items[frame.highlight].text.clone();
        assert_ne!(target, "解耦");
        target
    } else {
        panic!("expected Update");
    };

    // 云端延迟返回了旧候选词 "解耦" 的翻译
    *response.lock().unwrap() = Some(Prediction {
        sequence: req1.sequence,
        words: Vec::new(),
        sentence: Some("decouple".to_string()),
        alternatives: Vec::new(),
        ..Default::default()
    });
    router.tick();

    // 验证旧结果被静默丢弃，当前高亮词决不会错误挂上 "decouple"
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        if let Some(card) = &frame.learning_card {
            assert_ne!(card.target_text, "解耦", "旧候选词翻译已被作废丢弃");
            assert_eq!(card.target_text, current_highlight_target);
        }
    }
}

/// Scenario 5: Progressive Disclosure (`Ctrl+Alt+E`) and Breakdown Details
#[test]
fn test_scenario_05_progressive_disclosure_and_breakdown() {
    let (mut router, requests, response) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "xianqian");
    let req = requests.lock().unwrap().last().cloned().expect("发出了请求");

    *response.lock().unwrap() = Some(Prediction {
        sequence: req.sequence,
        words: Vec::new(),
        sentence: Some("beforehand".to_string()),
        alternatives: vec![
            PredictionAlternative {
                text: "beforehand".to_string(),
                tag: "Natural".to_string(),
                meaning: "预先/事先".to_string(),
                tone: "Casual".to_string(),
                scenario: "Daily planning".to_string(),
                commonality: "Very high".to_string(),
                why_natural: "Natural spoken adverb for doing things ahead of time".to_string(),
            },
            PredictionAlternative {
                text: "in advance".to_string(),
                tag: "Formal".to_string(),
                meaning: "提前/预先".to_string(),
                tone: "Polite".to_string(),
                scenario: "Work and business".to_string(),
                commonality: "High".to_string(),
                why_natural: "More formal and standard in workplace communication".to_string(),
            },
        ],
        ..Default::default()
    });
    router.tick();

    // 默认 expanded 为 false
    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        let card = frame.learning_card.expect("必须有 card");
        assert!(!card.expanded);
        assert_eq!(card.selected_index, None);
    }

    // 按快捷键 Ctrl+Alt+E 展开「更多译法」
    let expand_key = KeyEvent::new(
        'e' as u32,
        Some('e'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: expand_key,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        let card = frame.learning_card.expect("必须有 card");
        assert!(card.expanded, "Ctrl+Alt+E 成功展开更多译法");
        assert_eq!(card.alternatives.len(), 2);
    }

    // 再次按 Ctrl+Alt+E 收起
    std::thread::sleep(std::time::Duration::from_millis(150));
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: expand_key,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        let card = frame.learning_card.expect("必须有 card");
        assert!(!card.expanded, "再次按 Ctrl+Alt+E 成功收起更多译法");
    }
}

/// Scenario 6: Daily Mode Fast Translation
#[test]
fn test_scenario_06_daily_mode_zero_overhead_isolation() {
    let (mut router, requests, _) = create_test_router(LearningMode::Daily);

    type_letters(&mut router, "nihao");

    // Daily 模式下支持快速日常口语单句翻译
    let reqs = requests.lock().unwrap();
    assert!(
        reqs.iter().any(|r| r.kind == PredictionKind::Translate),
        "Daily 模式支持快速日常口语单句翻译"
    );

    let poll_msg = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll_msg {
        assert_eq!(frame.learning_mode, LearningMode::Daily);
    }
}
