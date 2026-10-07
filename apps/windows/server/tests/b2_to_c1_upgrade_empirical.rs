//! B2 -> C1 Expression Upgrade System Empirical Test Suite
//!
//! Validates:
//! 1. All 30 authentic B2 -> C1 scenarios:
//!    - Accurate B2 baseline detection
//!    - Accurate C1 recommendation
//!    - Rigorous Pragmatic Nuance and Register styles
//! 2. Anti-Overcorrection (Correctness != Sophistication):
//!    - Valid B2 sentences are NEVER classified as grammar errors or Chinglish.
//!    - Four-dimensional assessment evaluates correctness = true, naturalness_score, sophistication = B2.
//! 3. Direct C1 / Register match:
//!    - When user provides C1 expression or valid colloquial variant, recognized as ExactMatch.
//! 4. LearningVault & Profile integration:
//!    - ValidB2Upgrade counts toward c1_upgrades_count in UserEnglishProfile.
//! 5. Full Protocol serialization roundtrip:
//!    - Seamless serialization/deserialization across IPC protocol for upgrade structs.

use std::path::PathBuf;

use qingjian_core::{
    ComparisonStatus, Register, Sophistication,
};
use qingjian_learning::{
    RecallBook, get_upgrade_database, find_upgrade, evaluate_b2_upgrade, LearningVault,
};
use qingjian_platform::protocol::{
    LearningCardFrame as ProtoLearningCardFrame,
    LearningState as ProtoLearningState,
};

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("qj-upgrade-{}-{}-{}", name, std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_01_all_30_scenarios_exist_and_are_complete() {
    let db = get_upgrade_database();
    assert!(db.len() >= 30, "Database must contain at least 30 scenarios, found {}", db.len());

    for (idx, record) in db.iter().enumerate() {
        assert!(!record.prompt.is_empty(), "Scenario {} prompt is empty", idx);
        assert!(!record.b2_baseline.is_empty(), "Scenario {} b2_baseline is empty", idx);
        assert!(!record.natural_expression.is_empty(), "Scenario {} natural_expression is empty", idx);
        assert!(!record.c1_expression.is_empty(), "Scenario {} c1_expression is empty", idx);
        assert!(!record.why_better.is_empty(), "Scenario {} why_better is empty", idx);
        assert!(!record.nuance_explanation.is_empty(), "Scenario {} nuance_explanation is empty", idx);
        assert!(
            record.register_variants.len() >= 2,
            "Scenario {} ({}) must have at least 2 register variants, found {}",
            idx,
            record.prompt,
            record.register_variants.len()
        );

        for variant in &record.register_variants {
            assert!(!variant.expression.is_empty(), "Scenario {} variant expression is empty", idx);
            assert!(!variant.scenario.is_empty(), "Scenario {} variant scenario is empty", idx);
            assert!(!variant.feeling.is_empty(), "Scenario {} variant feeling is empty", idx);
        }
    }
}

#[test]
fn test_02_anti_overcorrection_simple_yet_correct_b2_never_penalized() {
    // 1. "觉得这事很难" -> User inputs grammatically correct B2: "I think this is very difficult."
    let attempt1 = RecallBook::evaluate_attempt(
        "觉得这事很难",
        "I think this is very difficult.",
        "I think this could be quite challenging.",
    );
    assert_eq!(
        attempt1.comparison.status,
        Some(ComparisonStatus::ValidB2Upgrade),
        "Correct B2 input must be classified as ValidB2Upgrade, not GrammarIssue"
    );
    assert!(
        !attempt1.error_categories.contains(&"grammar".to_string()),
        "Must not contain grammar error"
    );
    assert!(
        !attempt1.error_categories.contains(&"chinglish".to_string()),
        "Must not contain chinglish error"
    );
    let dims1 = attempt1.comparison.dimensions.as_ref().expect("Dimensions must be present");
    assert!(dims1.correctness, "Correctness must be true for valid B2");
    assert_eq!(dims1.sophistication, Sophistication::B2);

    let up1 = attempt1.comparison.upgrade.as_ref().expect("Upgrade must be attached");
    assert_eq!(up1.c1_expression, "I think this could be quite challenging.");
    assert!(up1.why_better.contains("challenging"));

    // 2. "你错了" -> User inputs B2: "You are wrong."
    let attempt2 = RecallBook::evaluate_attempt(
        "你错了",
        "You are wrong.",
        "I'm not sure that's entirely accurate.",
    );
    assert_eq!(attempt2.comparison.status, Some(ComparisonStatus::ValidB2Upgrade));
    assert!(attempt2.comparison.dimensions.as_ref().unwrap().correctness);

    // 3. "我们需要解决这个问题" -> User inputs B2: "We need to solve this problem."
    let attempt3 = RecallBook::evaluate_attempt(
        "我们需要解决这个问题",
        "We need to solve this problem.",
        "We need to tackle this issue.",
    );
    assert_eq!(attempt3.comparison.status, Some(ComparisonStatus::ValidB2Upgrade));
    assert!(attempt3.comparison.dimensions.as_ref().unwrap().correctness);
}

#[test]
fn test_03_c1_or_variant_input_recognized_as_exact_match() {
    // When user already uses C1 target: "I think this could be quite challenging."
    let attempt_c1 = RecallBook::evaluate_attempt(
        "觉得这事很难",
        "I think this could be quite challenging.",
        "I think this could be quite challenging.",
    );
    assert_eq!(attempt_c1.comparison.status, Some(ComparisonStatus::ExactMatch));
    assert!(attempt_c1.comparison.feedback_items.is_empty());
    assert!(attempt_c1.comparison.upgrade.is_some(), "Upgrade info still accessible on exact match");

    // When user uses a diplomatic variant: "It might require a bit more effort than we initially anticipated."
    let attempt_variant = RecallBook::evaluate_attempt(
        "觉得这事很难",
        "It might require a bit more effort than we initially anticipated.",
        "I think this could be quite challenging.",
    );
    assert_eq!(attempt_variant.comparison.status, Some(ComparisonStatus::ExactMatch));
    let dims = attempt_variant.comparison.dimensions.as_ref().expect("Dimensions present for variant");
    assert_eq!(dims.detected_register, Register::Diplomatic);
    assert_eq!(dims.sophistication, Sophistication::C1);
}

#[test]
fn test_04_vault_records_b2_upgrade_and_accumulates_profile() {
    let temp = TempDir::new("upgrade-test");
    let mut vault = LearningVault::open(&temp.0);

    // Record a B2 upgrade attempt
    let attempt_upgrade = RecallBook::evaluate_attempt(
        "我们需要解决这个问题",
        "We need to solve this problem.",
        "We need to tackle this issue.",
    );
    assert_eq!(attempt_upgrade.comparison.status, Some(ComparisonStatus::ValidB2Upgrade));

    vault.record_attempt(&attempt_upgrade);

    let profile = vault.calculate_profile();
    assert_eq!(profile.c1_upgrades_count, 1, "Profile must count C1 upgrades");
    assert_eq!(profile.total_attempts, 1);

    let ascii_report = profile.format_ascii_bars();
    assert!(ascii_report.contains("B2→C1 地道表达升级: 1 处"));
}

#[test]
fn test_05_pragmatic_nuances_distinguish_register_feelings() {
    let rec = find_upgrade("我们需要解决这个问题", "We need to tackle this issue.")
        .expect("Record must be found");

    assert_eq!(rec.c1_expression, "We need to tackle this issue.");
    assert!(rec.nuance_explanation.contains("tackle"));

    // Verify diplomatic variant exists
    let diplomatic = rec.register_variants.iter().find(|v| v.register == Register::Diplomatic);
    assert!(diplomatic.is_some(), "Must have diplomatic variant");
    let d = diplomatic.unwrap();
    assert!(d.scenario.contains("协调"));

    // Verify casual variant exists
    let casual = rec.register_variants.iter().find(|v| v.register == Register::Casual);
    assert!(casual.is_some(), "Must have casual variant");
}

#[test]
fn test_06_verify_all_30_scenarios_evaluate_consistently() {
    let db = get_upgrade_database();
    assert_eq!(db.len(), 30);

    for record in &db {
        // Test B2 baseline input
        let res_b2 = evaluate_b2_upgrade(&record.prompt, &record.b2_baseline, &record.c1_expression)
            .expect(&format!("Should evaluate B2 upgrade for {}", record.prompt));
        assert_eq!(
            res_b2.status,
            Some(ComparisonStatus::ValidB2Upgrade),
            "Failed on scenario prompt: {}, b2: {}, c1: {}",
            record.prompt,
            record.b2_baseline,
            record.c1_expression
        );
        let dims_b2 = res_b2.dimensions.expect("Dimensions must be present");
        assert!(dims_b2.correctness);
        assert_eq!(dims_b2.sophistication, Sophistication::B2);

        // Test C1 expression input
        let res_c1 = evaluate_b2_upgrade(&record.prompt, &record.c1_expression, &record.c1_expression)
            .expect(&format!("Should evaluate C1 for {}", record.prompt));
        assert_eq!(res_c1.status, Some(ComparisonStatus::ExactMatch));
        let dims_c1 = res_c1.dimensions.expect("Dimensions must be present");
        assert_eq!(dims_c1.sophistication, Sophistication::C1);
    }
}

#[test]
fn test_07_protocol_frame_serialization_roundtrip() {
    let up = find_upgrade("觉得这事很难", "I think this could be quite challenging.").unwrap();
    let frame = ProtoLearningCardFrame {
        target_text: "觉得这事很难".to_string(),
        state: ProtoLearningState::Ready,
        primary: None,
        alternatives: Vec::new(),
        expanded: true,
        selected_index: None,
        drill_stage: None,
        drill_input: String::new(),
        drill_attempt: None,
        drill_notice: None,
        today_due_count: 3,
        review_current_index: 1,
        review_total_count: 3,
        upgrade: Some(up),
    };

    let json = serde_json::to_string(&frame).expect("Must serialize LearningCardFrame");
    assert!(json.contains("I think this could be quite challenging."));
    assert!(json.contains("b2_baseline"));
    assert!(json.contains("register_variants"));

    let restored: ProtoLearningCardFrame = serde_json::from_str(&json).expect("Must deserialize LearningCardFrame");
    assert_eq!(restored.target_text, "觉得这事很难");
    assert_eq!(restored.today_due_count, 3);
    let restored_up = restored.upgrade.expect("Upgrade must be preserved");
    assert_eq!(restored_up.c1_expression, "I think this could be quite challenging.");
    assert_eq!(restored_up.b2_baseline, "I think this is very difficult.");
    assert!(restored_up.register_variants.len() >= 2);
}
