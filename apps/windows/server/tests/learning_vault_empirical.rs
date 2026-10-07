use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{
    ComparisonResult, ComparisonStatus, ErrorMajorCategory, FeedbackItem, MasteryLevel,
    Prediction, PredictionPolicy, PredictionRequest, Predictor, RecallAttempt,
};
use qingjian_learning::LearningVault;
use qingjian_platform::LearningMode;
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, KeyOutcome, ServerMessage, SessionId,
};
use qingjian_windows_server::dispatch::{Router, RouterConfig};
use qingjian_windows_server::{AssemblySpec, assembly};

const SESSION: SessionId = SessionId(201);

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("qj-vault-{}-{}-{}", name, std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct MockVaultPredictor {
    pub requests: Arc<Mutex<Vec<PredictionRequest>>>,
    pub response: Arc<Mutex<Option<Prediction>>>,
}

impl Predictor for MockVaultPredictor {
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
    user_dir: PathBuf,
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
        user_dir: Some(user_dir),
        glossary: Some((Language::English, root.join("assets/sample/glossary-en.tsv"))),
        english: Some(root.join("assets/sample/english.tsv")),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble test engine");

    let mock = MockVaultPredictor {
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

fn send_char(router: &mut Router, c: char) -> Option<ServerMessage> {
    let event = KeyEvent::new(c as u32, Some(c), KeyModifiers::default());
    router.handle(ClientMessage::Key {
        session: SESSION,
        event,
    })
}

fn send_vk(router: &mut Router, vk: u32) -> Option<ServerMessage> {
    let char_opt = if vk == 0x20 { Some(' ') } else { None };
    let event = KeyEvent::new(vk, char_opt, KeyModifiers::default());
    router.handle(ClientMessage::Key {
        session: SESSION,
        event,
    })
}

/// [EMPIRICAL TEST 1]: Daily 模式隐私安全隔离 —— 正常中文打字绝不向 LearningVault 写入任何数据
#[test]
fn test_daily_mode_privacy_zero_vault_writes() {
    let temp = TempDir::new("privacy-daily");
    let user_dir = temp.path().to_path_buf();

    let (mut router, _, _) = create_test_router(LearningMode::Daily, user_dir.clone());

    // 正常输入拼音 "nihao"
    for c in "nihao".chars() {
        let msg = send_char(&mut router, c).expect("ServerMessage");
        match msg {
            ServerMessage::KeyResult { outcome, .. } => {
                assert_eq!(outcome, KeyOutcome::Consumed);
            }
            other => panic!("expected KeyResult, got {:?}", other),
        }
    }

    // 按空格上屏
    let msg = send_vk(&mut router, 0x20).expect("ServerMessage");
    match msg {
        ServerMessage::KeyResult { commit, .. } => {
            assert!(commit.is_some(), "expected commit for Chinese candidates");
        }
        other => panic!("expected KeyResult, got {:?}", other),
    }

    // 严苛验证：学习账本的所有 5 个文件绝对不能被静默创建或写入！
    assert!(
        !user_dir.join("user-saved-expressions.tsv").exists(),
        "Daily mode MUST NOT create user-saved-expressions.tsv"
    );
    assert!(
        !user_dir.join("user-learning-attempts.jsonl").exists(),
        "Daily mode MUST NOT create user-learning-attempts.jsonl"
    );
    assert!(
        !user_dir.join("user-errors.tsv").exists(),
        "Daily mode MUST NOT create user-errors.tsv"
    );
    assert!(
        !user_dir.join("user-mastery.tsv").exists(),
        "Daily mode MUST NOT create user-mastery.tsv"
    );
    assert!(
        !user_dir.join("user-learning-profile.json").exists(),
        "Daily mode MUST NOT create user-learning-profile.json"
    );
}

/// [EMPIRICAL TEST 2]: Learn 模式主动保存表达持久化与完整 Roundtrip
#[test]
fn test_learn_mode_explicit_save_persistence() {
    let temp = TempDir::new("explicit-save");
    let dir = temp.path().to_path_buf();

    {
        let mut vault = LearningVault::open(&dir);
        assert!(vault.is_empty());

        // 用户显式收藏 2 条口语表达
        vault.save_expression(
            "我最近太忙了",
            "I've been swamped lately",
            Some("Casual"),
            Some("美式口语高频表达"),
        );
        vault.save_expression(
            "这个项目需要解耦",
            "We need to decouple this module",
            Some("Professional"),
            Some("工程架构规范术语"),
        );

        vault.flush().expect("flush vault");
    }

    // 验证物理文件落盘
    let saved_path = dir.join("user-saved-expressions.tsv");
    let mastery_path = dir.join("user-mastery.tsv");
    assert!(saved_path.exists(), "user-saved-expressions.tsv should exist");
    assert!(mastery_path.exists(), "user-mastery.tsv should exist");

    let saved_content = std::fs::read_to_string(&saved_path).unwrap();
    assert!(saved_content.contains("I've been swamped lately"));
    assert!(saved_content.contains("We need to decouple this module"));
    assert!(saved_content.contains("Casual"));
    assert!(saved_content.contains("Professional"));

    // 重启加载 (Round-trip)
    {
        let vault = LearningVault::open(&dir);
        assert_eq!(vault.saved_expressions().len(), 2);
        assert_eq!(vault.saved_expressions()[0].prompt, "我最近太忙了");
        assert_eq!(
            vault.saved_expressions()[0].expression,
            "I've been swamped lately"
        );
        assert_eq!(
            vault.saved_expressions()[0].mastery,
            MasteryLevel::Learning
        );
        assert_eq!(vault.saved_expressions()[1].prompt, "这个项目需要解耦");
    }
}

/// [EMPIRICAL TEST 3]: Drill 模式对比尝试记录与错题聚合 (Error Records)
#[test]
fn test_drill_mode_compare_attempt_and_error_aggregation() {
    let temp = TempDir::new("drill-errors");
    let dir = temp.path().to_path_buf();

    let attempt1 = RecallAttempt {
        prompt: "我非常喜欢这个想法".to_string(),
        user_attempt: "I very like this idea".to_string(),
        reference_expression: "I really like this idea".to_string(),
        timestamp: 1728345600,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::NaturalBetter),
            key_advice: "用 really like 代替 very like".to_string(),
            user_formatted: "I very like this idea".to_string(),
            natural_expression: "I really like this idea".to_string(),
            primary_category: Some(ErrorMajorCategory::Collocation),
            feedback_items: vec![FeedbackItem {
                major_category: ErrorMajorCategory::Collocation,
                subcategory: qingjian_core::ErrorSubcategory::UnnaturalCollocation,
                label: "搭配不自然".to_string(),
                explanation: "very 不可直接修饰动词 like，应使用 really like".to_string(),
                suggestion: Some("really like".to_string()),
            }],
            upgrade: None,
            dimensions: None,
        },
        error_categories: vec!["collocation:unnatural".to_string()],
    };

    let attempt2 = RecallAttempt {
        prompt: "我们讨论一下这个问题".to_string(),
        user_attempt: "Let's discuss about this issue".to_string(),
        reference_expression: "Let's discuss this issue".to_string(),
        timestamp: 1728345700,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::GrammarIssue),
            key_advice: "discuss 为及物动词，不加 about".to_string(),
            user_formatted: "Let's discuss about this issue".to_string(),
            natural_expression: "Let's discuss this issue".to_string(),
            primary_category: Some(ErrorMajorCategory::Grammar),
            feedback_items: vec![FeedbackItem {
                major_category: ErrorMajorCategory::Grammar,
                subcategory: qingjian_core::ErrorSubcategory::Preposition,
                label: "介词冗余".to_string(),
                explanation: "discuss 后面直接接宾语，不可接 about".to_string(),
                suggestion: Some("discuss this issue".to_string()),
            }],
            upgrade: None,
            dimensions: None,
        },
        error_categories: vec!["grammar:preposition".to_string()],
    };

    {
        let mut vault = LearningVault::open(&dir);
        vault.record_attempt(&attempt1);
        vault.record_attempt(&attempt2);
        vault.flush().expect("flush vault");
    }

    // 检查 JSONL 与 TSV 落盘格式
    let attempts_path = dir.join("user-learning-attempts.jsonl");
    let errors_path = dir.join("user-errors.tsv");
    assert!(attempts_path.exists());
    assert!(errors_path.exists());

    let jsonl_content = std::fs::read_to_string(&attempts_path).unwrap();
    let lines: Vec<&str> = jsonl_content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("I very like this idea"));
    assert!(lines[1].contains("discuss about"));

    let errors_content = std::fs::read_to_string(&errors_path).unwrap();
    assert!(errors_content.contains("collocation"));
    assert!(errors_content.contains("grammar"));

    // 记录同类错误，验证 occurrence_count 累加
    {
        let mut vault = LearningVault::open(&dir);
        assert_eq!(vault.attempts().len(), 2);
        assert_eq!(vault.error_records().len(), 2);

        // 重复一次 discuss about 错误
        vault.record_attempt(&attempt2);
        vault.flush().expect("flush vault");
    }

    {
        let vault = LearningVault::open(&dir);
        assert_eq!(vault.attempts().len(), 3);
        let errs = vault.error_records();
        let discuss_err = errs
            .iter()
            .find(|e| e.user_fragment.contains("discuss"))
            .expect("discuss error record");
        assert_eq!(discuss_err.occurrence_count, 2);
    }
}

/// [EMPIRICAL TEST 4]: 个人学习画像动态计算、弱项诊断与 ASCII 柱状图生成
#[test]
fn test_profile_accumulation_and_weakness_derivation() {
    let temp = TempDir::new("profile-accumulation");
    let dir = temp.path().to_path_buf();

    let mut vault = LearningVault::open(&dir);

    // 1. 成功测试：Vocabulary
    let vocab_success = RecallAttempt {
        prompt: "解耦".to_string(),
        user_attempt: "decouple".to_string(),
        reference_expression: "decouple".to_string(),
        timestamp: 1000,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::ExactMatch),
            ..Default::default()
        },
        error_categories: vec![],
    };
    vault.record_attempt(&vocab_success);

    // 2. 失败测试：多次 Collocation 与 PhrasalVerbs 错误
    for i in 0..3 {
        let col_fail = RecallAttempt {
            prompt: format!("搭配测试 {}", i),
            user_attempt: "make a photo".to_string(),
            reference_expression: "take a photo".to_string(),
            timestamp: 1000 + i,
            comparison: ComparisonResult {
                status: Some(ComparisonStatus::NaturalBetter),
                primary_category: Some(ErrorMajorCategory::Collocation),
                feedback_items: vec![FeedbackItem {
                    major_category: ErrorMajorCategory::Collocation,
                    subcategory: qingjian_core::ErrorSubcategory::IncorrectVerbNounPairing,
                    label: "动宾搭配".to_string(),
                    explanation: "照相用 take a photo，不用 make".to_string(),
                    suggestion: Some("take a photo".to_string()),
                }],
                ..Default::default()
            },
            error_categories: vec!["collocation:verb_noun".to_string()],
        };
        vault.record_attempt(&col_fail);
    }

    let profile = vault.calculate_profile();
    assert_eq!(profile.total_attempts, 4);

    // Vocabulary 成功应获得加分 (>= 70)
    assert!(profile.vocabulary.score >= 70);

    // Collocation 发生 3 次错误，应受到惩罚 (< 70)
    assert!(profile.collocation.score < 70);

    // 验证薄弱环节正确提取出 Collocations
    assert!(
        profile.top_weaknesses.contains(&"Collocations".to_string()),
        "top_weaknesses should identify Collocations"
    );

    // 验证 ASCII 柱状图格式
    let ascii_bars = profile.format_ascii_bars();
    assert!(ascii_bars.contains("Your English Profile:"));
    assert!(ascii_bars.contains("Vocabulary"));
    assert!(ascii_bars.contains("Collocations"));
    assert!(ascii_bars.contains("█"));
    assert!(ascii_bars.contains("你的主要提升机会：Collocations"));
}

/// [EMPIRICAL TEST 5]: 掌握度状态机升级与降级闭环 (Unseen -> Learning -> Familiar -> Mastered -> Failure Reset)
#[test]
fn test_mastery_progression_and_failure_demotion() {
    let temp = TempDir::new("mastery-fsm");
    let dir = temp.path().to_path_buf();

    let mut vault = LearningVault::open(&dir);

    // 主动保存表达 -> 初始状态为 Learning
    vault.save_expression("习惯于", "be used to", Some("Common"), None);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Learning
    );

    let success_attempt = RecallAttempt {
        prompt: "习惯于".to_string(),
        user_attempt: "be used to".to_string(),
        reference_expression: "be used to".to_string(),
        timestamp: 2000,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::ExactMatch),
            ..Default::default()
        },
        error_categories: vec![],
    };

    let fail_attempt = RecallAttempt {
        prompt: "习惯于".to_string(),
        user_attempt: "used to".to_string(),
        reference_expression: "be used to".to_string(),
        timestamp: 3000,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::GrammarIssue),
            primary_category: Some(ErrorMajorCategory::Grammar),
            ..Default::default()
        },
        error_categories: vec!["grammar:tense".to_string()],
    };

    // 1st success -> 仍然是 Learning (1 次成功)
    vault.record_attempt(&success_attempt);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Learning
    );

    // 2nd consecutive success -> 晋级为 Familiar (2 次连续成功)
    vault.record_attempt(&success_attempt);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Familiar
    );

    // 3rd consecutive success -> 保持 Familiar
    vault.record_attempt(&success_attempt);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Familiar
    );

    // 4th consecutive success -> 晋级为 Mastered (4 次连续成功)
    vault.record_attempt(&success_attempt);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Mastered
    );

    // 1 次失败 -> 立即重置降级为 Learning
    vault.record_attempt(&fail_attempt);
    assert_eq!(
        vault.saved_expressions()[0].mastery,
        MasteryLevel::Learning
    );
}

/// [EMPIRICAL TEST 6]: 隐私安全全量擦除 (Clear All Privacy Purge)
#[test]
fn test_privacy_clear_all_purge() {
    let temp = TempDir::new("privacy-purge");
    let dir = temp.path().to_path_buf();

    let mut vault = LearningVault::open(&dir);
    vault.save_expression("测试", "test", None, None);
    vault.record_attempt(&RecallAttempt {
        prompt: "测试".to_string(),
        user_attempt: "test".to_string(),
        reference_expression: "test".to_string(),
        timestamp: 100,
        comparison: ComparisonResult {
            status: Some(ComparisonStatus::ExactMatch),
            ..Default::default()
        },
        error_categories: vec![],
    });
    vault.flush().expect("flush vault");

    // 确认落盘文件存在
    assert!(dir.join("user-saved-expressions.tsv").exists());
    assert!(dir.join("user-learning-attempts.jsonl").exists());
    assert!(dir.join("user-mastery.tsv").exists());
    assert!(dir.join("user-learning-profile.json").exists());

    // 执行彻底清除
    vault.clear_all().expect("clear_all");

    // 验证内存清空
    assert!(vault.is_empty());
    assert_eq!(vault.saved_expressions().len(), 0);
    assert_eq!(vault.attempts().len(), 0);
    assert_eq!(vault.error_records().len(), 0);

    // 验证物理磁盘文件全部彻底删除
    assert!(!dir.join("user-saved-expressions.tsv").exists());
    assert!(!dir.join("user-learning-attempts.jsonl").exists());
    assert!(!dir.join("user-errors.tsv").exists());
    assert!(!dir.join("user-mastery.tsv").exists());
    assert!(!dir.join("user-learning-profile.json").exists());

    // 再次重新打开确认为空
    let vault2 = LearningVault::open(&dir);
    assert!(vault2.is_empty());
}
