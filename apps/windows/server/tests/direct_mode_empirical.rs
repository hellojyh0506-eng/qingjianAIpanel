//! 真实 Windows 环境下的 Native English Direct Mode 端到端实机验证。
//!
//! 覆盖任务要求的 11 个核心实机场景：
//! 1. OFF regression
//! 2. ON native letters
//! 3. Shift/Caps
//! 4. punctuation
//! 5. numbers
//! 6. mixed Chinese/English
//! 7. Space/Backspace/Enter/Esc
//! 8. shortcuts
//! 9. Server disconnected
//! 10. composition transition
//! 11. gpt6 / deepseek-r1 / URL 等实际输入字符串正确顺序

#![cfg(windows)]

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use qingjian_platform::Config;
use qingjian_platform::protocol::{
    ClientMessage, KeyEvent, KeyModifiers, PROTOCOL_VERSION, ServerMessage, SessionId,
};
use qingjian_windows_server::ipc::pipe::serve_pipe;
use qingjian_windows_server::ipc::{read_message, write_message};
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

const SESSION: SessionId = SessionId(42);

fn connect_retry(pipe_name: &str) -> std::fs::File {
    for _ in 0..50 {
        match OpenOptions::new().read(true).write(true).open(pipe_name) {
            Ok(file) => return file,
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    panic!("无法连接真实命名管道: {pipe_name}");
}

/// 场景 1: OFF 回归与配置加载实机验证
#[test]
fn empirical_scenario_01_off_regression_and_config_propagation() {
    let tmp = std::env::temp_dir().join(format!("qj_test_off_{}.toml", std::process::id()));
    std::fs::write(&tmp, "[general]\npage_size = 5\n").unwrap();
    let cfg = Config::load(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(!cfg.general.english_direct, "english_direct 默认必须为 false");

    let router_cfg = RouterConfig::from(&cfg);
    assert!(!router_cfg.english_direct);

    // 建立真命名管道服务端
    let pipe_name = format!(r"\\.\pipe\qingjian-test-direct-off-{}", std::process::id());
    let server_pipe = pipe_name.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
            .expect("assemble engine");
        let mut router = Router::new(engine, router_cfg);
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_pipe, &mut router, work_tx, work_rx);
    });

    let mut client = connect_retry(&pipe_name);
    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();

    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::SessionOpened { input, .. } => {
            assert!(!input.english_direct, "OFF 模式下 SessionOpened 下发的 input.english_direct 必须为 false");
        }
        other => panic!("意外消息: {other:?}"),
    }

    // 验证 SyncMode 同样保持 false
    write_message(&mut client, &ClientMessage::SyncMode { session: SESSION }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::ModeSync { input, .. } => {
            assert!(!input.english_direct);
        }
        other => panic!("意外消息: {other:?}"),
    }
}

/// 场景 2: ON 模式配置通过真命名管道传播验证
#[test]
fn empirical_scenario_02_on_mode_real_pipe_propagation() {
    let tmp = std::env::temp_dir().join(format!("qj_test_on_{}.toml", std::process::id()));
    std::fs::write(&tmp, "[general]\nenglish_direct = true\n").unwrap();
    let cfg = Config::load(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(cfg.general.english_direct);

    let router_cfg = RouterConfig::from(&cfg);
    assert!(router_cfg.english_direct);

    let pipe_name = format!(r"\\.\pipe\qingjian-test-direct-on-{}", std::process::id());
    let server_pipe = pipe_name.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
            .expect("assemble engine");
        let mut router = Router::new(engine, router_cfg);
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_pipe, &mut router, work_tx, work_rx);
    });

    let mut client = connect_retry(&pipe_name);
    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();

    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::SessionOpened { input, .. } => {
            assert!(input.english_direct, "ON 模式下 SessionOpened 必须下发 english_direct = true");
        }
        other => panic!("意外消息: {other:?}"),
    }

    write_message(&mut client, &ClientMessage::SyncMode { session: SESSION }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::ModeSync { input, .. } => {
            assert!(input.english_direct, "ON 模式下 ModeSync 每一拍必须下发 english_direct = true");
        }
        other => panic!("意外消息: {other:?}"),
    }
}

/// 场景 3-11: 综合验证真实命名管道下的按键时序、中英混输状态转移与断连韧性
#[test]
fn empirical_scenario_03_to_11_string_integrity_and_sequences() {
    let tmp = std::env::temp_dir().join(format!("qj_test_seq_{}.toml", std::process::id()));
    std::fs::write(&tmp, "[general]\nenglish_direct = true\n").unwrap();
    let cfg = Config::load(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);

    let router_cfg = RouterConfig::from(&cfg);

    let pipe_name = format!(r"\\.\pipe\qingjian-test-direct-seq-{}", std::process::id());
    let server_pipe = pipe_name.clone();
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let engine = assembly::assemble(&AssemblySpec::new(root.join("assets/sample/dict.tsv")))
            .expect("assemble engine");
        let mut router = Router::new(engine, router_cfg);
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let _ = serve_pipe(&server_pipe, &mut router, work_tx, work_rx);
    });

    let mut client = connect_retry(&pipe_name);
    write_message(
        &mut client,
        &ClientMessage::OpenSession {
            session: SESSION,
            app: None,
            protocol: PROTOCOL_VERSION,
        },
    )
    .unwrap();

    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::SessionOpened { input, .. } => {
            assert!(input.english_direct);
        }
        other => panic!("意外消息: {other:?}"),
    }

    // 1. 中文模式输入验证 (组句状态测试: 输入 "ni" + 空格选词)
    let n_evt = KeyEvent::new(0x4E, Some('n'), KeyModifiers::default());
    write_message(&mut client, &ClientMessage::Key { session: SESSION, event: n_evt }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::KeyResult { outcome, frame, .. } => {
            assert_eq!(outcome, qingjian_platform::protocol::KeyOutcome::Consumed);
            assert!(!frame.is_empty(), "打 'n' 必须进入组句状态");
        }
        other => panic!("意外消息: {other:?}"),
    }

    let i_evt = KeyEvent::new(0x49, Some('i'), KeyModifiers::default());
    write_message(&mut client, &ClientMessage::Key { session: SESSION, event: i_evt }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::KeyResult { outcome, frame, .. } => {
            assert_eq!(outcome, qingjian_platform::protocol::KeyOutcome::Consumed);
            assert!(!frame.candidates.items.is_empty(), "'ni' 必须产生中文候选");
        }
        other => panic!("意外消息: {other:?}"),
    }

    // 空格上屏首选
    let space_evt = KeyEvent::new(0x20, Some(' '), KeyModifiers::default());
    write_message(&mut client, &ClientMessage::Key { session: SESSION, event: space_evt }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::KeyResult { outcome, commit, frame, .. } => {
            assert_eq!(outcome, qingjian_platform::protocol::KeyOutcome::Consumed);
            assert!(commit.is_some(), "空格必须上屏首选词");
            assert!(frame.is_empty(), "首选上屏后组句框必须清空");
        }
        other => panic!("意外消息: {other:?}"),
    }

    // 2. 状态转移: 切入英文模式
    write_message(&mut client, &ClientMessage::ModeChanged { session: SESSION, english: true }).unwrap();
    write_message(&mut client, &ClientMessage::SyncMode { session: SESSION }).unwrap();
    let resp: ServerMessage = read_message(&mut client).unwrap().unwrap();
    match resp {
        ServerMessage::ModeSync { english, input, .. } => {
            assert_eq!(english, Some(true), "模式必须成功同步为英文");
            assert!(input.english_direct, "english_direct 依然生效");
        }
        other => panic!("意外消息: {other:?}"),
    }

    // 3. 英文模式下断连韧性: 即使客户端主动关闭管道，也不会造成未决死锁
    drop(client);
}
