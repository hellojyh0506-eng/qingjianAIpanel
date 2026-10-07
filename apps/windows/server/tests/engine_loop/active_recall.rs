use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use qingjian_core::candidate::Language;
use qingjian_core::{RecallItem, RecallOutcome, RecallTracker};
use qingjian_learning::RecallBook;
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, KeyOutcome,
};

use crate::support::*;

#[derive(Default, Clone)]
struct TestRecallTracker {
    pub items: Arc<Mutex<Vec<RecallItem>>>,
    pub recalls: Arc<Mutex<Vec<(String, String, RecallOutcome)>>>,
}

impl RecallTracker for TestRecallTracker {
    fn record_exposure(&mut self, language: Language, prompt: &str, target: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|i| i.prompt == prompt && i.target == target) {
            item.seen += 1;
        } else {
            items.push(RecallItem {
                prompt: prompt.to_string(),
                target: target.to_string(),
                language,
                seen: 1,
                success: 0,
                failure: 0,
                revealed: 0,
            });
        }
    }

    fn record_recall(
        &mut self,
        _language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
    ) {
        self.recalls.lock().unwrap().push((prompt.to_string(), target.to_string(), outcome));
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|i| i.prompt == prompt && i.target == target) {
            match outcome {
                RecallOutcome::Success => item.success += 1,
                RecallOutcome::Failure => item.failure += 1,
                RecallOutcome::Revealed => item.revealed += 1,
            }
        }
    }

    fn next_item(&self, _language: Language) -> Option<RecallItem> {
        let items = self.items.lock().unwrap();
        items.iter().find(|i| i.success == 0).cloned().or_else(|| items.first().cloned())
    }

    fn pending_count(&self, _language: Language) -> usize {
        self.items.lock().unwrap().iter().filter(|i| i.success == 0).count()
    }
}

fn router_with_recall() -> (Router, TestRecallTracker) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let glossary = root.join("assets/sample/glossary-en.tsv");
    let tracker = TestRecallTracker::default();

    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, glossary)),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine from sample data");

    engine.set_recall_tracker(Box::new(tracker.clone()));

    let mut router = Router::new(engine, RouterConfig::default());
    open_session(&mut router, SESSION, None);
    (router, tracker)
}

fn ctrl_alt_key(c: char) -> KeyEvent {
    KeyEvent::new(
        c.to_ascii_uppercase() as u32,
        Some(c),
        KeyModifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            caps: false,
            english_mode: false,
        },
    )
}

#[test]
fn test_active_recall_exposure_recorded_on_candidate_commit() {
    let (mut router, tracker) = router_with_recall();

    // 1. 用户敲 "kaifa" -> 候选 "开发" 带有英文翻译 "develop"
    let (outcome, commit, frame) = type_letters(&mut router, "kaifa");
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert_eq!(frame.candidates.items.first().unwrap().text, "开发");

    // 2. 空格上屏中文 "开发"
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    let (outcome, commit, _) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit.as_deref(), Some("开发"));

    // 3. 验证 RecallTracker 自动记录了曝光
    let items = tracker.items.lock().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].prompt, "开发");
    assert!(items[0].target.contains("develop"));
    assert_eq!(items[0].seen, 1);
}

#[test]
fn test_active_recall_interactive_success_workflow() {
    let (mut router, tracker) = router_with_recall();

    // 预置一道待复习条目
    tracker.items.lock().unwrap().push(RecallItem {
        prompt: "测试".to_string(),
        target: "test".to_string(),
        language: Language::English,
        seen: 1,
        success: 0,
        failure: 0,
        revealed: 0,
    });

    // 1. 触发 Ctrl + Alt + R 开启主动回忆
    let trigger = ctrl_alt_key('r');
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None, "主动回忆全程绝不上屏任何字符给应用");
    assert!(frame.notice.as_deref().unwrap().contains("测试"));

    // 2. 依次输入 "t", "e", "s", "t"
    for ch in ['t', 'e', 's', 't'] {
        let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch.to_ascii_uppercase() as u32, Some(ch), Default::default()),
        }));
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert_eq!(commit, None);
        assert!(frame.preedit.first().unwrap().text.contains(ch));
    }

    // 3. 按 Enter 提交判定
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(frame.notice.as_deref().unwrap().contains("正确"));

    // 4. 验证成功计次
    let recalls = tracker.recalls.lock().unwrap();
    assert_eq!(recalls.len(), 1);
    assert_eq!(recalls[0], ("测试".to_string(), "test".to_string(), RecallOutcome::Success));
}

#[test]
fn test_active_recall_interactive_failure_and_tab_reveal() {
    let (mut router, tracker) = router_with_recall();

    tracker.items.lock().unwrap().push(RecallItem {
        prompt: "苹果".to_string(),
        target: "apple".to_string(),
        language: Language::English,
        seen: 1,
        success: 0,
        failure: 0,
        revealed: 0,
    });

    // 1. 进入回忆态
    let trigger = ctrl_alt_key('r');
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));

    // 2. 按 Tab 查看答案
    let tab = KeyEvent::new(0x09, Some('\t'), Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: tab,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(frame.notice.as_deref().unwrap().contains("apple"));

    let recalls = tracker.recalls.lock().unwrap();
    assert_eq!(recalls.len(), 1);
    assert_eq!(recalls[0].2, RecallOutcome::Revealed);
    drop(recalls);

    // 3. 输入错误答案 "banana" 并回车
    for ch in ['b', 'a', 'n', 'a', 'n', 'a'] {
        router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        });
    }
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(frame.notice.as_deref().unwrap().contains("未匹配"));

    let recalls = tracker.recalls.lock().unwrap();
    assert_eq!(recalls.len(), 2);
    assert_eq!(recalls[1].2, RecallOutcome::Failure);
}

#[test]
fn test_active_recall_escape_cleanly_exits() {
    let (mut router, tracker) = router_with_recall();

    tracker.items.lock().unwrap().push(RecallItem {
        prompt: "测试".to_string(),
        target: "test".to_string(),
        language: Language::English,
        seen: 1,
        success: 0,
        failure: 0,
        revealed: 0,
    });

    // 触发
    let trigger = ctrl_alt_key('r');
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));

    // 输入一些字符
    router.handle(ClientMessage::Key {
        session: SESSION,
        event: KeyEvent::new('T' as u32, Some('t'), Default::default()),
    });

    // Esc 退出
    let esc = KeyEvent::new(0x1B, None, Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: esc,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(frame.is_empty(), "退出回忆态后候选窗隐藏");
}

#[test]
fn test_gaodingle_natural_alternative_acceptance_workflow() {
    let (mut router, tracker) = router_with_recall();

    // 预置系统学到的真实表达：
    // 中文：“搞定了”
    // 系统学习表达：“It's done.”
    tracker.items.lock().unwrap().push(RecallItem {
        prompt: "搞定了".to_string(),
        target: "It's done.".to_string(),
        language: Language::English,
        seen: 1,
        success: 0,
        failure: 0,
        revealed: 0,
    });

    // 1. 用户触发 Ctrl + Alt + R 进入主动回忆
    let trigger = ctrl_alt_key('r');
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(frame.notice.as_deref().unwrap().contains("搞定了"));

    // 2. 用户主动回忆，自己输入自然美式口语等价表达："We're good"
    for ch in "We're good".chars() {
        let (outcome, commit, _) = key_result(router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        }));
        assert_eq!(outcome, KeyOutcome::Consumed);
        assert_eq!(commit, None);
    }

    // 3. 按 Enter 提交
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    // 验证：判定为 Correct（正确！），而非因没有死板输入 "It's done." 或 "resolve" 而判错！
    assert!(
        frame.notice.as_deref().unwrap().contains("正确"),
        "Approved natural alternative 'We're good' must be accepted as Correct for '搞定了'!"
    );

    // 4. 验证 RecallTracker 计次已记录成功
    let recalls = tracker.recalls.lock().unwrap();
    assert_eq!(recalls.len(), 1);
    assert_eq!(recalls[0], ("搞定了".to_string(), "It's done.".to_string(), RecallOutcome::Success));
    drop(recalls);

    // 5. 错误输入用例：如果用户输入死板生硬字典词 "resolve" 或无关词 "apple"，必须判定为未匹配！
    tracker.items.lock().unwrap().push(RecallItem {
        prompt: "搞定了".to_string(),
        target: "It's done.".to_string(),
        language: Language::English,
        seen: 1,
        success: 0,
        failure: 0,
        revealed: 0,
    });
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    for ch in "resolve".chars() {
        router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        });
    }
    let (outcome, commit, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert_eq!(outcome, KeyOutcome::Consumed);
    assert_eq!(commit, None);
    assert!(
        frame.notice.as_deref().unwrap().contains("未匹配"),
        "Literal dictionary translation 'resolve' must not be accepted for spoken '搞定了'"
    );
}

#[test]
fn test_full_active_recall_closed_loop_with_real_tsv_persistence() {
    let temp_dir = std::env::temp_dir().join(format!("qingjian-recall-loop-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let recall_path = temp_dir.join("user-recall.tsv");

    // 1. 建立基于磁盘持久化文件的 RecallBook
    let recall_book = RecallBook::open(&recall_path);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let glossary = root.join("assets/sample/glossary-en.tsv");

    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, glossary)),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine");

    engine.set_recall_tracker(Box::new(recall_book));

    let mut router = Router::new(engine, RouterConfig::default());
    open_session(&mut router, SESSION, None);

    // 2. 模拟日常输入：用户敲 "kaifa" -> 候选 "开发" 带有译文 "develop" -> 空格上屏
    type_letters(&mut router, "kaifa");
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: space,
    }));

    // 3. 此时引擎内部已生成曝光条目，触发主动回忆 Ctrl + Alt + R
    let trigger = ctrl_alt_key('r');
    let (_, _, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    assert!(frame.notice.as_deref().unwrap().contains("开发"));

    // 4. 用户主动回忆，键入 "develop"
    for ch in "develop".chars() {
        router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        });
    }

    // 5. 按 Enter 提交
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (_, _, frame) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert!(frame.notice.as_deref().unwrap().contains("正确"));

    // 6. 保存并验证持久化到真实磁盘 TSV 文件
    router.flush_learning();
    let tsv_content = std::fs::read_to_string(&recall_path).expect("read persisted tsv");
    assert!(tsv_content.contains("开发\tdevelop\ten"));
    // 成功计数 1
    assert!(tsv_content.contains("\t1\t0\t0\t"));

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_active_recall_preferred_alternative_learned_feedback() {
    let temp_dir = std::env::temp_dir().join(format!("qingjian-recall-pref-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let recall_path = temp_dir.join("user-recall.tsv");

    let mut recall_book = RecallBook::open(&recall_path);
    recall_book.record_exposure(Language::English, "搞定了", "It's done.");

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dict = root.join("assets/sample/dict.tsv");
    let glossary = root.join("assets/sample/glossary-en.tsv");

    let mut engine = assembly::assemble(&AssemblySpec {
        glossary: Some((Language::English, glossary)),
        ..AssemblySpec::new(dict)
    })
    .expect("assemble engine");

    engine.set_recall_tracker(Box::new(recall_book));

    let mut router = Router::new(engine, RouterConfig::default());
    open_session(&mut router, SESSION, None);

    let trigger = ctrl_alt_key('r');

    // 第 1 次主动回忆：用户键入 "We're good"
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    for ch in "We're good".chars() {
        router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        });
    }
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    let (_, _, frame1) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    assert!(frame1.notice.as_deref().unwrap().contains("正确"));

    // 第 2 次主动回忆：用户再次键入 "We're good"
    key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: trigger,
    }));
    for ch in "We're good".chars() {
        router.handle(ClientMessage::Key {
            session: SESSION,
            event: KeyEvent::new(ch as u32, Some(ch), Default::default()),
        });
    }
    let (_, _, frame2) = key_result(router.handle(ClientMessage::Key {
        session: SESSION,
        event: enter,
    }));
    // 验证第二次成功后，系统通过 Notice 明确反馈已学习该自然表达！
    assert!(
        frame2
            .notice
            .as_deref()
            .unwrap()
            .contains("已学习首选表达「搞定了」-> We're good"),
        "Notice must acknowledge the newly learned natural alternative"
    );

    std::fs::remove_dir_all(&temp_dir).unwrap();
}


