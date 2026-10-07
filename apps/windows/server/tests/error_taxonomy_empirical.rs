//! Error Taxonomy Empirical Validation Suite for Phase P4
//!
//! Validates:
//! 1. All 6 Major Taxonomy Categories and their Subcategories:
//!    - Grammar (Tense, Preposition, Countability)
//!    - Vocabulary (WrongWord, VocabularyPrecision)
//!    - Collocation (UnnaturalCollocation, FixedExpression, IncorrectVerbNounPairing)
//!    - Naturalness (Chinglish, AwkwardPhrasing)
//!    - Phrasal / Idiomatic (MissingPhrasalVerb)
//!    - Pragmatics (WrongSocialTone, TooFormal)
//! 2. Quantity cap: Max 1~3 structured suggestions per comparison.
//! 3. Anti-Pedantry & Anti-Overcorrection: Valid American spoken expressions and contractions
//!    (gonna, wanna, I've been, we're good, no worries, etc.) must NEVER be flagged as errors.
//! 4. Full IPC & Rendering pipeline preservation of structured feedback items.

use qingjian_core::{
    ComparisonStatus, ErrorMajorCategory, ErrorSubcategory,
};
use qingjian_learning::{RecallBook, analyze_feedback_items};
use qingjian_platform::protocol::{
    ComparisonResult as ProtoComparisonResult,
    ErrorMajorCategory as ProtoMajorCategory,
    ErrorSubcategory as ProtoSubcategory,
    RecallAttempt as ProtoRecallAttempt,
};

#[test]
fn test_taxonomy_01_grammar_tense_preposition_countability() {
    // 1.1 Tense conflict
    let attempt_tense = RecallBook::evaluate_attempt(
        "我昨天非常忙。",
        "I am very busy yesterday.",
        "I was swamped yesterday.",
    );
    assert_eq!(attempt_tense.comparison.status, Some(ComparisonStatus::GrammarIssue));
    assert_eq!(attempt_tense.comparison.primary_category, Some(ErrorMajorCategory::Grammar));
    let tense_item = attempt_tense.comparison.feedback_items.iter().find(|i| i.major_category == ErrorMajorCategory::Grammar).unwrap();
    assert_eq!(tense_item.subcategory, ErrorSubcategory::Tense);
    assert!(tense_item.label.contains("时态"));
    assert!(tense_item.explanation.contains("yesterday"));

    // 1.2 Preposition: "depend of"
    let attempt_prep = RecallBook::evaluate_attempt(
        "这取决于具体情况。",
        "It depends of the situation.",
        "It depends on the situation.",
    );
    assert_eq!(attempt_prep.comparison.primary_category, Some(ErrorMajorCategory::Grammar));
    let prep_item = attempt_prep.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::Preposition).unwrap();
    assert!(prep_item.label.contains("介词"));
    assert!(prep_item.explanation.contains("depend on"));

    // 1.3 Countability: "an advice" / "advices"
    let attempt_count = RecallBook::evaluate_attempt(
        "他给了我一条建议。",
        "He gave me an advice.",
        "He gave me some advice.",
    );
    assert_eq!(attempt_count.comparison.primary_category, Some(ErrorMajorCategory::Grammar));
    let count_item = attempt_count.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::Countability).unwrap();
    assert!(count_item.label.contains("不可数"));
    assert!(count_item.explanation.contains("advice"));
}

#[test]
fn test_taxonomy_02_vocabulary_wrong_word_and_precision() {
    // 2.1 Wrong word: "look the tv" -> watch TV
    let attempt_word = RecallBook::evaluate_attempt(
        "我看电视。",
        "I look the tv.",
        "I watch TV.",
    );
    assert_eq!(attempt_word.comparison.primary_category, Some(ErrorMajorCategory::Vocabulary));
    let word_item = attempt_word.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::WrongWord).unwrap();
    assert!(word_item.label.contains("选词"));
    assert!(word_item.explanation.contains("watch"));

    // 2.2 Precision: "often customer" -> "regular"
    let attempt_prec = RecallBook::evaluate_attempt(
        "我是那里的常客。",
        "I am an often customer there.",
        "I am a regular there.",
    );
    assert_eq!(attempt_prec.comparison.primary_category, Some(ErrorMajorCategory::Vocabulary));
    let prec_item = attempt_prec.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::VocabularyPrecision).unwrap();
    assert!(prec_item.label.contains("精度"));
    assert!(prec_item.explanation.contains("regular"));
}

#[test]
fn test_taxonomy_03_collocation_unnatural_fixed_and_pairings() {
    // 3.1 Unnatural collocation: "very like"
    let attempt_like = RecallBook::evaluate_attempt(
        "我非常喜欢这个主意。",
        "I very like this idea.",
        "I really like this idea.",
    );
    assert!(attempt_like.comparison.feedback_items.iter().any(|i| i.major_category == ErrorMajorCategory::Collocation && i.subcategory == ErrorSubcategory::UnnaturalCollocation));

    // 3.2 Fixed expression / redundant prep: "discuss about"
    let attempt_discuss = RecallBook::evaluate_attempt(
        "我们来讨论一下方案。",
        "Let's discuss about the proposal.",
        "Let's discuss the proposal.",
    );
    assert_eq!(attempt_discuss.comparison.primary_category, Some(ErrorMajorCategory::Collocation));
    let discuss_item = attempt_discuss.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::FixedExpression).unwrap();
    assert!(discuss_item.label.contains("搭配冗余"));
    assert!(discuss_item.explanation.contains("discuss"));

    // 3.3 Incorrect verb-noun pairing: "make a photo"
    let attempt_photo = RecallBook::evaluate_attempt(
        "拍张照吧。",
        "Let's make a photo.",
        "Let's take a picture.",
    );
    assert_eq!(attempt_photo.comparison.primary_category, Some(ErrorMajorCategory::Collocation));
    let photo_item = attempt_photo.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::IncorrectVerbNounPairing).unwrap();
    assert!(photo_item.label.contains("动宾搭配"));
    assert!(photo_item.explanation.contains("take a picture"));
}

#[test]
fn test_taxonomy_04_naturalness_chinglish_and_awkward_phrasing() {
    // 4.1 Chinglish: literal translations
    let attempt_color = RecallBook::evaluate_attempt(
        "给你点颜色瞧瞧。",
        "give you some color to see see",
        "teach you a lesson",
    );
    assert_eq!(attempt_color.comparison.status, Some(ComparisonStatus::Chinglish));
    assert_eq!(attempt_color.comparison.primary_category, Some(ErrorMajorCategory::Naturalness));
    let color_item = attempt_color.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::Chinglish).unwrap();
    assert!(color_item.label.contains("字面直译"));

    let attempt_sea = RecallBook::evaluate_attempt(
        "人山人海。",
        "people mountain people sea",
        "it was packed",
    );
    assert_eq!(attempt_sea.comparison.primary_category, Some(ErrorMajorCategory::Naturalness));

    let attempt_dog = RecallBook::evaluate_attempt(
        "我忙成狗了。",
        "I am busy like dog.",
        "I've been swamped.",
    );
    assert_eq!(attempt_dog.comparison.primary_category, Some(ErrorMajorCategory::Naturalness));

    // 4.2 Awkward phrasing vs natural upgrade (swamped vs very busy)
    let attempt_awkward = RecallBook::evaluate_attempt(
        "我最近忙得焦头烂额。",
        "I am very busy.",
        "I've been swamped lately.",
    );
    assert_eq!(attempt_awkward.comparison.primary_category, Some(ErrorMajorCategory::Naturalness));
    let awk_item = attempt_awkward.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::AwkwardPhrasing).unwrap();
    assert!(awk_item.label.contains("地道表达升级"));
}

#[test]
fn test_taxonomy_05_phrasal_idiomatic_missing_phrasal_verb() {
    // 5.1 Missing phrasal verb: "cancel" vs "call off"
    let attempt_phrasal = RecallBook::evaluate_attempt(
        "他们取消了会议。",
        "They decided to cancel the meeting.",
        "They decided to call off the meeting.",
    );
    assert_eq!(attempt_phrasal.comparison.primary_category, Some(ErrorMajorCategory::PhrasalIdiomatic));
    let phrasal_item = attempt_phrasal.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::MissingPhrasalVerb).unwrap();
    assert!(phrasal_item.label.contains("短语动词"));
    assert!(phrasal_item.explanation.contains("call off"));
}

#[test]
fn test_taxonomy_06_pragmatics_tone_and_formality() {
    // 6.1 Wrong social tone: "according to me" -> in my opinion
    let attempt_tone = RecallBook::evaluate_attempt(
        "依我看，这不行。",
        "According to me, this won't work.",
        "In my opinion, this won't work.",
    );
    assert_eq!(attempt_tone.comparison.primary_category, Some(ErrorMajorCategory::Pragmatics));
    let tone_item = attempt_tone.comparison.feedback_items.iter().find(|i| i.subcategory == ErrorSubcategory::WrongSocialTone).unwrap();
    assert!(tone_item.label.contains("语用"));
    assert!(tone_item.explanation.contains("according to"));

    // 6.2 Too formal in casual context: "it is done" vs "we're good"
    let tips = analyze_feedback_items("it is done", "we're good");
    assert!(tips.iter().any(|i| i.major_category == ErrorMajorCategory::Pragmatics));
}

#[test]
fn test_quantity_cap_max_three_items() {
    // A sentence with multiple triggers: tense + prep + collocation + chinglish
    let messy_input = "Yesterday I am very like to discuss about it and busy like dog.";
    let items = analyze_feedback_items(messy_input, "I really liked discussing it.");
    // Must strictly be capped at 1~3 items to prevent pedantic info overload
    assert!(!items.is_empty());
    assert!(items.len() <= 3, "Actual items len: {}", items.len());
}

#[test]
fn test_anti_overcorrection_and_colloquial_contractions() {
    // Valid colloquial American contractions and phrases must NEVER trigger errors or false pedantic warnings
    let cases = [
        ("I'm gonna hit the gym.", "I'm going to hit the gym.", "去健身房"),
        ("I wanna grab a bite.", "I want to grab a bite.", "吃点东西"),
        ("I've been swamped lately.", "I've been swamped lately.", "最近很忙"),
        ("We're good.", "It is done.", "办妥了"),
        ("No worries.", "No problem.", "没事"),
        ("Got it.", "Got it / I understand.", "明白了"),
        ("All set.", "All set / Everything is ready.", "准备就绪"),
    ];

    for (user_input, target, prompt) in cases {
        let attempt = RecallBook::evaluate_attempt(prompt, user_input, target);
        assert_eq!(
            attempt.comparison.status,
            Some(ComparisonStatus::ExactMatch),
            "Expected ExactMatch for '{}', but got {:?}",
            user_input,
            attempt.comparison.status
        );
        assert!(
            attempt.comparison.feedback_items.is_empty(),
            "Expected 0 feedback items for valid colloquial '{}', but got: {:?}",
            user_input,
            attempt.comparison.feedback_items
        );
        assert!(
            attempt.error_categories.is_empty(),
            "Expected empty error_categories for '{}', but got: {:?}",
            user_input,
            attempt.error_categories
        );
    }
}

#[test]
fn test_pipeline_protocol_conversion_preserves_feedback_items() {
    let attempt = RecallBook::evaluate_attempt(
        "我昨天非常忙。",
        "I am very busy yesterday.",
        "I was swamped yesterday.",
    );

    let proto = ProtoRecallAttempt {
        prompt: attempt.prompt.clone(),
        user_attempt: attempt.user_attempt.clone(),
        reference_expression: attempt.reference_expression.clone(),
        timestamp: attempt.timestamp,
        comparison: ProtoComparisonResult {
            status: attempt.comparison.status.map(|s| match s {
                ComparisonStatus::ExactMatch => qingjian_platform::protocol::ComparisonStatus::ExactMatch,
                ComparisonStatus::NaturalBetter => qingjian_platform::protocol::ComparisonStatus::NaturalBetter,
                ComparisonStatus::GrammarIssue => qingjian_platform::protocol::ComparisonStatus::GrammarIssue,
                ComparisonStatus::Chinglish => qingjian_platform::protocol::ComparisonStatus::Chinglish,
                ComparisonStatus::EmptyInput => qingjian_platform::protocol::ComparisonStatus::EmptyInput,
                ComparisonStatus::ValidB2Upgrade => qingjian_platform::protocol::ComparisonStatus::ValidB2Upgrade,
            }),
            key_advice: attempt.comparison.key_advice.clone(),
            user_formatted: attempt.comparison.user_formatted.clone(),
            natural_expression: attempt.comparison.natural_expression.clone(),
            primary_category: attempt.comparison.primary_category.map(|c| match c {
                ErrorMajorCategory::Grammar => ProtoMajorCategory::Grammar,
                ErrorMajorCategory::Vocabulary => ProtoMajorCategory::Vocabulary,
                ErrorMajorCategory::Collocation => ProtoMajorCategory::Collocation,
                ErrorMajorCategory::Naturalness => ProtoMajorCategory::Naturalness,
                ErrorMajorCategory::PhrasalIdiomatic => ProtoMajorCategory::PhrasalIdiomatic,
                ErrorMajorCategory::Pragmatics => ProtoMajorCategory::Pragmatics,
            }),
            feedback_items: attempt.comparison.feedback_items.iter().map(|item| {
                qingjian_platform::protocol::FeedbackItem {
                    major_category: match item.major_category {
                        ErrorMajorCategory::Grammar => ProtoMajorCategory::Grammar,
                        ErrorMajorCategory::Vocabulary => ProtoMajorCategory::Vocabulary,
                        ErrorMajorCategory::Collocation => ProtoMajorCategory::Collocation,
                        ErrorMajorCategory::Naturalness => ProtoMajorCategory::Naturalness,
                        ErrorMajorCategory::PhrasalIdiomatic => ProtoMajorCategory::PhrasalIdiomatic,
                        ErrorMajorCategory::Pragmatics => ProtoMajorCategory::Pragmatics,
                    },
                    subcategory: match &item.subcategory {
                        ErrorSubcategory::Tense => ProtoSubcategory::Tense,
                        ErrorSubcategory::Preposition => ProtoSubcategory::Preposition,
                        ErrorSubcategory::Countability => ProtoSubcategory::Countability,
                        ErrorSubcategory::WrongWord => ProtoSubcategory::WrongWord,
                        ErrorSubcategory::VocabularyPrecision => ProtoSubcategory::VocabularyPrecision,
                        ErrorSubcategory::UnnaturalCollocation => ProtoSubcategory::UnnaturalCollocation,
                        ErrorSubcategory::FixedExpression => ProtoSubcategory::FixedExpression,
                        ErrorSubcategory::IncorrectVerbNounPairing => ProtoSubcategory::IncorrectVerbNounPairing,
                        ErrorSubcategory::Chinglish => ProtoSubcategory::Chinglish,
                        ErrorSubcategory::AwkwardPhrasing => ProtoSubcategory::AwkwardPhrasing,
                        ErrorSubcategory::MissingPhrasalVerb => ProtoSubcategory::MissingPhrasalVerb,
                        ErrorSubcategory::WrongSocialTone => ProtoSubcategory::WrongSocialTone,
                        ErrorSubcategory::TooFormal => ProtoSubcategory::TooFormal,
                        _ => ProtoSubcategory::Tense,
                    },
                    label: item.label.clone(),
                    explanation: item.explanation.clone(),
                    suggestion: item.suggestion.clone(),
                }
            }).collect(),
            upgrade: None,
            dimensions: None,
        },
        error_categories: attempt.error_categories.clone(),
    };

    assert_eq!(proto.comparison.primary_category, Some(ProtoMajorCategory::Grammar));
    assert!(!proto.comparison.feedback_items.is_empty());
    assert_eq!(proto.comparison.feedback_items[0].subcategory, ProtoSubcategory::Tense);
}
