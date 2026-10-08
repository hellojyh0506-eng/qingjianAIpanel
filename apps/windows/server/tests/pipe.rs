//! 真命名管道的端到端测试（仅 Windows）：监听线程 + 文件句柄客户端，走完「开会话 → 敲 nihao → 收候选」。

#![cfg(windows)]

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use qingjian_core::Language;
use qingjian_learning::RecallBook;
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, PROTOCOL_VERSION, ServerMessage, SessionId,
};
use qingjian_windows_server::ipc::pipe::serve_pipe;
use qingjian_windows_server::ipc::{read_message, write_message};
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

const SESSION: SessionId = SessionId(1);

fn letter(c: char) -> KeyEvent {
    KeyEvent::new(c.to_ascii_uppercase() as u32, Some(c), Default::default())
}

/// 客户端重试打开管道，直到监听线程建好实例。
fn connect(name: &str) -> std::fs::File {
    for _ in 0..50 {
        match OpenOptions::new().read(true).write(true).open(name) {
            Ok(file) => return file,
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    panic!("连不上管道 {name}");
}

#[test]
fn named_pipe_round_trips_the_open_type_loop() {
    let name = format!(r"\\.\pipe\qingjian-test-{}", std::process::id());

    // 监听线程服务完一个客户端后阻塞等下一个，随进程退出即可。
    let server_name = name.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec {
            glossary: Some((
                Language::English,
                root.join("assets/sample/glossary-en.tsv"),
            )),
            ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
        })
        .expect("assemble engine from sample data");
        let mut router = Router::new(engine, RouterConfig::default());
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_name, &mut router, work_tx, work_rx);
    });

    let mut client = connect(&name);

    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    for c in "nihao".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session: SESSION,
                event: letter(c),
            },
        )
        .unwrap();
    }

    // 开会话先回一条 `SessionOpened`，之后五个按键各回一条 `KeyResult`。
    let mut last_frame = None;
    for _ in 0..6 {
        let message: ServerMessage = read_message(&mut client)
            .expect("read response")
            .expect("server closed early");
        if let ServerMessage::KeyResult { frame, .. } = message {
            last_frame = Some(frame);
        }
    }

    let frame = last_frame.expect("至少一条 KeyResult");
    let preedit: String = frame.preedit.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(preedit, "ni'hao");
    let texts: Vec<&str> = frame
        .candidates
        .items
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        texts.contains(&"你好"),
        "候选里应有「你好」，实际：{texts:?}"
    );
}

#[test]
fn named_pipe_passive_translation_and_space_commits_chinese_only() {
    let name = format!(r"\\.\pipe\qingjian-test-passive-{}", std::process::id());

    let server_name = name.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec {
            glossary: Some((
                Language::English,
                root.join("assets/sample/glossary-en.tsv"),
            )),
            ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
        })
        .expect("assemble engine from sample data");
        let mut router = Router::new(engine, RouterConfig::default());
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_name, &mut router, work_tx, work_rx);
    });

    let mut client = connect(&name);

    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();

    // 经真实命名管道输入「kaifa」
    for c in "kaifa".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session: SESSION,
                event: letter(c),
            },
        )
        .unwrap();
    }

    let mut last_frame = None;
    for _ in 0..6 {
        let message: ServerMessage = read_message(&mut client)
            .expect("read response")
            .expect("server closed early");
        if let ServerMessage::KeyResult { frame, .. } = message {
            last_frame = Some(frame);
        }
    }

    let frame = last_frame.expect("至少一条 KeyResult");
    let first = frame.candidates.items.first().expect("首个候选");
    assert_eq!(first.text, "开发");
    let trans = first.translation.as_ref().expect("开发带被动英文翻译");
    assert!(
        trans.senses().iter().any(|s| s.text.contains("develop")),
        "译文含 develop: {:?}",
        trans.senses()
    );

    // 经真实命名管道发送 Space 键（0x20）
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    write_message(
        &mut client,
        &ClientMessage::Key {
            session: SESSION,
            event: space,
        },
    )
    .unwrap();

    let commit_message: ServerMessage = read_message(&mut client)
        .expect("read response")
        .expect("server closed early");
    let (commit_text, after_frame) = match commit_message {
        ServerMessage::KeyResult { commit, frame, .. } => (commit, frame),
        other => panic!("expected KeyResult for space, got {other:?}"),
    };

    // 跨命名管道验证：Space 必须只上屏中文「开发」，绝不上屏英文「develop」
    assert_eq!(
        commit_text.as_deref(),
        Some("开发"),
        "Space 上屏结果必须为中文候选"
    );
    assert!(after_frame.is_empty(), "上屏后候选框收起");
}

#[test]
fn named_pipe_active_recall_round_trip() {
    let name = format!(r"\\.\pipe\qingjian-test-recall-{}", std::process::id());
    let temp_dir = std::env::temp_dir().join(format!("qingjian-pipe-recall-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let recall_path = temp_dir.join("user-recall.tsv");

    let server_name = name.clone();
    let recall_path_clone = recall_path.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let mut engine = assembly::assemble(&AssemblySpec {
            glossary: Some((
                Language::English,
                root.join("assets/sample/glossary-en.tsv"),
            )),
            ..AssemblySpec::new(root.join("assets/sample/dict.tsv"))
        })
        .expect("assemble engine from sample data");
        let recall_book = RecallBook::open(recall_path_clone);
        engine.set_recall_tracker(Box::new(recall_book));
        let mut router = Router::new(engine, RouterConfig::default());
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_name, &mut router, work_tx, work_rx);
    });

    let mut client = connect(&name);

    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap();

    // 1. 输入 kaifa 并空格上屏，产生曝光
    for c in "kaifa".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session: SESSION,
                event: letter(c),
            },
        )
        .unwrap();
        let _: ServerMessage = read_message(&mut client).unwrap().unwrap();
    }
    let space = KeyEvent::new(0x20, Some(' '), Default::default());
    write_message(
        &mut client,
        &ClientMessage::Key {
            session: SESSION,
            event: space,
        },
    )
    .unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap();

    // 2. 发送 Ctrl + Alt + R 开启主动回忆
    let ctrl_alt_r = KeyEvent::new(
        'R' as u32,
        Some('r'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            caps: false,
            english_mode: false,
        },
    );
    write_message(
        &mut client,
        &ClientMessage::Key {
            session: SESSION,
            event: ctrl_alt_r,
        },
    )
    .unwrap();
    let recall_open_msg: ServerMessage = read_message(&mut client).unwrap().unwrap();
    if let ServerMessage::KeyResult { frame, .. } = recall_open_msg {
        assert!(frame.notice.as_deref().unwrap().contains("开发"));
    } else {
        panic!("expected KeyResult for Ctrl+Alt+R");
    }

    // 3. 跨管道键入 "develop"
    for c in "develop".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session: SESSION,
                event: letter(c),
            },
        )
        .unwrap();
        let _: ServerMessage = read_message(&mut client).unwrap().unwrap();
    }

    // 4. 按 Enter 提交
    let enter = KeyEvent::new(0x0D, Some('\r'), Default::default());
    write_message(
        &mut client,
        &ClientMessage::Key {
            session: SESSION,
            event: enter,
        },
    )
    .unwrap();
    let result_msg: ServerMessage = read_message(&mut client).unwrap().unwrap();
    if let ServerMessage::KeyResult { frame, .. } = result_msg {
        assert!(frame.notice.as_deref().unwrap().contains("正确"));
    } else {
        panic!("expected KeyResult for enter");
    }

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

static REAL_PIPE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_real_running_server_shui_yue_and_zhengliushui() {
    let _guard = REAL_PIPE_MUTEX.lock().unwrap();
    let mut client = match OpenOptions::new().read(true).write(true).open(r"\\.\pipe\qingjian") {
        Ok(f) => f,
        Err(_) => return, // Server not running, skip
    };
    let session = SessionId(999);
    write_message(&mut client, &ClientMessage::OpenSession {
        session,
        app: None,
        protocol: PROTOCOL_VERSION,
    }).unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap(); // SessionOpened

    let type_keys = |client: &mut std::fs::File, input: &str| -> qingjian_platform::protocol::Frame {
        let mut last_frame = None;
        for c in input.chars() {
            write_message(client, &ClientMessage::Key {
                session,
                event: letter(c),
            }).unwrap();
            let msg: ServerMessage = read_message(client).unwrap().unwrap();
            if let ServerMessage::KeyResult { frame, .. } = msg {
                last_frame = Some(frame);
            }
        }
        last_frame.unwrap()
    };

    let commit_esc = |client: &mut std::fs::File| {
        let esc = KeyEvent::new(0x1B, None, Default::default());
        write_message(client, &ClientMessage::Key {
            session,
            event: esc,
        }).unwrap();
        let _: ServerMessage = read_message(client).unwrap().unwrap();
    };

    // 1. 打 shui
    let frame_shui = type_keys(&mut client, "shui");
    let candidates_shui: Vec<String> = frame_shui.candidates.items.into_iter().map(|c| c.text).collect();
    println!("candidates_shui: {:?}", candidates_shui);
    assert!(!candidates_shui.is_empty());
    assert_ne!(candidates_shui[0], "说", "shui 首选绝不能是「说」");
    if candidates_shui.len() > 1 {
        assert_ne!(candidates_shui[1], "说");
    }
    if candidates_shui.len() > 2 {
        assert_ne!(candidates_shui[2], "说");
    }
    commit_esc(&mut client);

    // 2. 打 yue
    let frame_yue = type_keys(&mut client, "yue");
    let candidates_yue: Vec<String> = frame_yue.candidates.items.into_iter().map(|c| c.text).collect();
    println!("candidates_yue: {:?}", candidates_yue);
    assert_eq!(candidates_yue[0], "月", "yue 首选应为「月」");
    assert!(!candidates_yue[..3.min(candidates_yue.len())].contains(&"说".to_string()), "yue 前三候选不能包含「说」");
    commit_esc(&mut client);

    // 3. 打 qing'chun'de'zheng'liu'shui
    let frame_all = type_keys(&mut client, "qingchundezhengliushui");
    let candidates_all: Vec<String> = frame_all.candidates.items.into_iter().map(|c| c.text).collect();
    println!("candidates_all: {:?}", candidates_all);
    assert!(!candidates_all.is_empty());
    assert_eq!(candidates_all[0], "青春的蒸馏水", "首选必须是「青春的蒸馏水」，不能是「青春的蒸馏说」");
    commit_esc(&mut client);
}

#[test]
fn test_real_running_server_learning_mode_switch() {
    let _guard = REAL_PIPE_MUTEX.lock().unwrap();
    let mut client = match OpenOptions::new().read(true).write(true).open(r"\\.\pipe\qingjian") {
        Ok(f) => f,
        Err(_) => return, // Server not running, skip
    };
    let session = SessionId(998);
    write_message(&mut client, &ClientMessage::OpenSession {
        session,
        app: None,
        protocol: PROTOCOL_VERSION,
    }).unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap(); // SessionOpened

    // 1. 发送 Ctrl+Alt+L 切换模式
    let toggle_key = KeyEvent::new(
        'L' as u32,
        Some('l'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    write_message(&mut client, &ClientMessage::Key {
        session,
        event: toggle_key,
    }).unwrap();
    let msg: ServerMessage = read_message(&mut client).unwrap().unwrap();
    if let ServerMessage::KeyResult { frame, .. } = msg {
        println!("Switched learning mode via pipe: {:?}", frame.learning_mode);
        assert!(
            frame.learning_mode == qingjian_platform::LearningMode::Learn
            || frame.learning_mode == qingjian_platform::LearningMode::Drill
            || frame.learning_mode == qingjian_platform::LearningMode::Daily
        );
        assert!(frame.notice.is_some());
    } else {
        panic!("expected KeyResult");
    }
}

#[test]
fn test_real_running_server_fsrs_review_round_trip() {
    let _guard = REAL_PIPE_MUTEX.lock().unwrap();
    let mut client = match OpenOptions::new().read(true).write(true).open(r"\\.\pipe\qingjian") {
        Ok(f) => f,
        Err(_) => return, // Server not running, skip
    };
    let session = SessionId(999);
    write_message(&mut client, &ClientMessage::OpenSession {
        session,
        app: None,
        protocol: PROTOCOL_VERSION,
    }).unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap(); // SessionOpened

    // 1. 打 nihao 建立组句
    for c in "nihao".chars() {
        write_message(
            &mut client,
            &ClientMessage::Key {
                session,
                event: letter(c),
            },
        ).unwrap();
        let _: ServerMessage = read_message(&mut client).unwrap().unwrap();
    }

    // 2. 发送 Ctrl+Alt+R 启动 FSRS 今日复习交互
    let review_key = KeyEvent::new(
        'R' as u32,
        Some('r'),
        KeyModifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        },
    );
    write_message(&mut client, &ClientMessage::Key {
        session,
        event: review_key,
    }).unwrap();

    let msg: ServerMessage = read_message(&mut client).unwrap().unwrap();
    if let ServerMessage::KeyResult { frame, outcome, .. } = msg {
        assert_eq!(outcome, qingjian_platform::protocol::KeyOutcome::Consumed);
        println!("FSRS review triggered on real server: learning_card={:?}", frame.learning_card.is_some());
        if let Some(card) = frame.learning_card {
            println!("Drill stage: {:?}", card.drill_stage);
            // 无论是进入 Review 阶段还是提示今日无到期，均正确捕获
            assert!(
                card.drill_stage == Some(qingjian_platform::protocol::DrillStage::Review)
                || card.drill_notice.is_some()
            );
        }
    } else {
        panic!("expected KeyResult");
    }

    // 3. 发送 Esc 退出复习/清空组句
    let esc_key = KeyEvent::new(0x1B, None, Default::default());
    write_message(&mut client, &ClientMessage::Key {
        session,
        event: esc_key,
    }).unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap();

    // 再次 Esc 清理组句
    write_message(&mut client, &ClientMessage::Key {
        session,
        event: esc_key,
    }).unwrap();
    let _: ServerMessage = read_message(&mut client).unwrap().unwrap();
}


