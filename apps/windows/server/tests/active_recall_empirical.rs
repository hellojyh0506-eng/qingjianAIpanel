use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{
    ComparisonStatus, Prediction, PredictionPolicy, PredictionRequest, Predictor,
};
use qingjian_learning::RecallBook;
use qingjian_platform::LearningMode;
use qingjian_platform::protocol::{
    ClientMessage, DrillStage, KeyEvent, KeyModifiers, KeyOutcome, ServerMessage, SessionId,
};
use qingjian_windows_server::dispatch::{Router, RouterConfig};
use qingjian_windows_server::{AssemblySpec, assembly};

const SESSION: SessionId = SessionId(101);

struct MockRecallPredictor {
    pub requests: Arc<Mutex<Vec<PredictionRequest>>>,
    pub response: Arc<Mutex<Option<Prediction>>>,
}

impl Predictor for MockRecallPredictor {
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

    let mock = MockRecallPredictor {
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

/// Scenario 1: Exact Match (User reproduces the natural target expression)
#[test]
fn test_scenario_01_exact_match() {
    let attempt = RecallBook::evaluate_attempt(
        "我最近忙得焦头烂额。",
        "I've been swamped lately.",
        "I've been swamped lately.",
    );

    assert_eq!(attempt.comparison.status, Some(ComparisonStatus::ExactMatch));
    assert_eq!(attempt.user_attempt, "I've been swamped lately.");
    assert_eq!(attempt.reference_expression, "I've been swamped lately.");
    assert!(attempt.error_categories.is_empty());
    assert!(attempt.comparison.key_advice.contains("非常地道准确"));
}

/// Scenario 2: Grammar Issue Detection (Tense clash or unnatural verb pattern)
#[test]
fn test_scenario_02_grammar_issue() {
    // 2.1 Tense conflict with yesterday
    let attempt_tense = RecallBook::evaluate_attempt(
        "我昨天非常忙。",
        "I am very busy yesterday.",
        "I was swamped yesterday.",
    );
    assert_eq!(
        attempt_tense.comparison.status,
        Some(ComparisonStatus::GrammarIssue)
    );
    assert!(attempt_tense.error_categories.contains(&"grammar".to_string()));
    assert!(attempt_tense.comparison.key_advice.contains("时态冲突"));

    // 2.2 Unnatural "very like" instead of "really like"
    let attempt_like = RecallBook::evaluate_attempt(
        "我非常喜欢这个方案。",
        "I very like this proposal.",
        "I really like this proposal.",
    );
    assert_eq!(
        attempt_like.comparison.status,
        Some(ComparisonStatus::GrammarIssue)
    );
    assert!(attempt_like.error_categories.contains(&"grammar".to_string()));
    assert!(attempt_like.comparison.key_advice.contains("very like"));
}

/// Scenario 3: Chinglish / Unidiomatic Phrasing Detection
#[test]
fn test_scenario_03_chinglish_detection() {
    // 3.1 Literal translation: "give you some color to see see"
    let attempt_color = RecallBook::evaluate_attempt(
        "给你点颜色瞧瞧。",
        "give you some color to see see",
        "teach you a lesson",
    );
    assert_eq!(
        attempt_color.comparison.status,
        Some(ComparisonStatus::Chinglish)
    );
    assert!(attempt_color.error_categories.contains(&"chinglish".to_string()));
    assert!(attempt_color.comparison.key_advice.contains("字面直译中式表达"));

    // 3.2 "busy like dog"
    let attempt_dog = RecallBook::evaluate_attempt(
        "我最近忙得像条狗。",
        "I am busy like dog recently.",
        "I've been swamped lately.",
    );
    assert_eq!(attempt_dog.comparison.status, Some(ComparisonStatus::Chinglish));
    assert!(attempt_dog.error_categories.contains(&"chinglish".to_string()));
}

/// Scenario 4: Meaning Correct But Less Natural (B2 -> C1 upgrade feedback)
#[test]
fn test_scenario_04_meaning_correct_less_natural() {
    let attempt = RecallBook::evaluate_attempt(
        "我最近忙得焦头烂额。",
        "I've been very busy recently.",
        "I've been swamped lately.",
    );

    assert_eq!(
        attempt.comparison.status,
        Some(ComparisonStatus::NaturalBetter)
    );
    assert_eq!(attempt.user_attempt, "I've been very busy recently.");
    assert_eq!(attempt.reference_expression, "I've been swamped lately.");
    assert!(
        attempt
            .comparison
            .key_advice
            .contains("swamped")
            || attempt.comparison.key_advice.contains("表达基本正确")
    );
}

/// Scenario 5: Empty User Input Rejection (Zero fake comparisons)
#[test]
fn test_scenario_05_empty_input_rejection_no_fake_comparison() {
    let (mut router, _, _) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "mang");

    // Manually trigger drill mode via Ctrl+Alt+D
    let drill_key = KeyEvent::new(
        'D' as u32,
        Some('D'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    let reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: drill_key,
    });

    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        let card = frame.learning_card.expect("Card must exist in Learn mode");
        assert_eq!(card.drill_stage, Some(DrillStage::Input));
        assert_eq!(card.drill_input, "");

        // User hits Enter with empty input
        let enter_key = KeyEvent::new(0x0D, None, KeyModifiers::default());
        let reply_enter = router.handle(ClientMessage::Key {
            session: SESSION,
            event: enter_key,
        });

        if let Some(ServerMessage::KeyResult {
            frame: enter_frame,
            outcome: enter_outcome,
            ..
        }) = reply_enter {
            assert_eq!(enter_outcome, KeyOutcome::Consumed);
            let updated_card = enter_frame.learning_card.expect("Card must exist");
            // Must stay in Input stage!
            assert_eq!(
                updated_card.drill_stage,
                Some(DrillStage::Input),
                "Empty input must NOT transition to Comparing or Compared!"
            );
            // Must show rejection notice!
            assert_eq!(
                updated_card.drill_notice,
                Some("请输入英文表达进行对比".to_string()),
                "Empty input must display explicit prompt notice"
            );
            assert!(
                updated_card.drill_attempt.is_none(),
                "Empty input must NEVER create a fake comparison attempt"
            );
        } else {
            panic!("Expected KeyResult for Enter key");
        }
    } else {
        panic!("Expected KeyResult for drill trigger");
    }
}

/// Scenario 6: Deterministic Offline Fallback Safety
#[test]
fn test_scenario_06_deterministic_offline_fallback_safety() {
    // When LLM network request times out or is completely offline,
    // evaluate_attempt executes deterministically and instantly in 0ms.
    let start = std::time::Instant::now();
    let attempt = RecallBook::evaluate_attempt(
        "我需要解决这个问题。",
        "I need to solve the problem.",
        "I need to tackle this issue.",
    );
    let duration = start.elapsed();

    assert!(duration.as_millis() < 50, "Deterministic fallback must complete in <50ms");
    assert!(!attempt.comparison.key_advice.is_empty());
    assert_eq!(attempt.user_attempt, "I need to solve the problem.");
    assert_eq!(attempt.reference_expression, "I need to tackle this issue.");
}

/// Scenario 7: Composition Isolation during Drill Input Mode
#[test]
fn test_scenario_07_drill_composition_isolation() {
    let (mut router, _, _) = create_test_router(LearningMode::Learn);

    type_letters(&mut router, "mang");

    // Enter drill mode
    let drill_key = KeyEvent::new(
        'D' as u32,
        Some('D'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: drill_key,
    });

    // Check preedit before typing English in drill mode
    let pre_msg = router.handle(ClientMessage::Poll { session: SESSION });
    let original_preedit = match pre_msg {
        Some(ServerMessage::Update { frame, .. }) => frame.preedit.clone(),
        _ => Vec::new(),
    };

    // Type English into drill input: "busy"
    for c in "busy".chars() {
        let key = KeyEvent::new(c as u32, Some(c), KeyModifiers::default());
        let reply = router.handle(ClientMessage::Key {
            session: SESSION,
            event: key,
        });
        if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = reply {
            assert_eq!(outcome, KeyOutcome::Consumed);
            let card = frame.learning_card.expect("card must exist");
            assert_eq!(card.drill_stage, Some(DrillStage::Input));
            // Chinese preedit must be 100% isolated and unchanged!
            assert_eq!(
                frame.preedit, original_preedit,
                "Typing in Recall mode must NOT mutate Chinese preedit!"
            );
        }
    }

    // Verify accumulated drill input
    let poll = router.handle(ClientMessage::Poll { session: SESSION });
    if let Some(ServerMessage::Update { frame, .. }) = poll {
        let card = frame.learning_card.expect("card must exist");
        assert_eq!(card.drill_input, "busy");
    }

    // Backspace test
    let bs = KeyEvent::new(0x08, None, KeyModifiers::default());
    let bs_reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: bs,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = bs_reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        let card = frame.learning_card.expect("card must exist");
        assert_eq!(card.drill_input, "bus");
    }

    // Escape test: Exits drill stage back to None
    let esc = KeyEvent::new(0x1B, None, KeyModifiers::default());
    let esc_reply = router.handle(ClientMessage::Key {
        session: SESSION,
        event: esc,
    });
    if let Some(ServerMessage::KeyResult { frame, outcome, .. }) = esc_reply {
        assert_eq!(outcome, KeyOutcome::Consumed);
        let card = frame.learning_card.expect("card must exist");
        assert_eq!(
            card.drill_stage, None,
            "Escape must exit drill stage cleanly"
        );
    }
}
