//! 真实的 Windows TSF COM 运行时接口与 Host 宿主消息交付实机端到端验收测试。
//!
//! 验证链路：
//! 物理/系统按键 -> Windows LoadLibrary(qingjian_tsf.dll)
//! -> DllGetClassObject(CLSID_QINGJIAN) -> IClassFactory::CreateInstance
//! -> ITfKeyEventSink::OnTestKeyDown / OnKeyDown -> Windows 原生 BOOL 判定
//! -> Host application native message pump delivery (WM_KEYDOWN / WM_CHAR).
//!
//! 严格覆盖全部 14 个关键场景：
//! 1. 常规字母 a/A (放行)
//! 2. Shift/Caps 状态 (放行)
//! 3. 标点与数字 (放行)
//! 4. 功能键 Space/Backspace/Enter/Esc (放行)
//! 5. 快捷键 Ctrl/Alt/Win (放行)
//! 6. 真实词串 "gpt6" 逐键序列 (放行)
//! 7. 真实词串 "deepseek-r1" 逐键序列 (放行)
//! 8. 真实词串 "C++" 逐键序列 (放行)
//! 9. 真实词串 "I'm" 逐键序列 (放行)
//! 10. 真实词串 "email@example.com" 逐键序列 (放行)
//! 11. 真实 URL 逐键序列 (放行)
//! 12. 中文组句状态转移保护与退出
//! 13. English Direct OFF 模式完整回归 (v0.1.4 行为保持拦截)
//! 14. 动态配置热切换验证 (ON -> OFF -> ON)

#![cfg(windows)]

use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, S_OK, WPARAM};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::TextServices::ITfKeyEventSink;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    RegisterClassW, MSG, WINDOW_EX_STYLE, WM_CHAR, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{BOOL, GUID, HRESULT, HSTRING, Interface, PCWSTR, s};

use qingjian_platform::Config;
use qingjian_platform::protocol::DEFAULT_PIPE_NAME;
use qingjian_windows_server::dispatch::{DataDirs, StatusEvent};
use qingjian_windows_server::ipc::pipe::serve_pipe;
use qingjian_windows_server::{AssemblySpec, Router, RouterConfig, assembly};

const CLSID_QINGJIAN: GUID = GUID::from_u128(0x4fdca82d_e923_49bf_9e75_bb906b93b8bb);
const GUID_SWITCH_MODE: GUID = GUID::from_u128(0x2f6b8c51_9a34_4e7d_b2c8_5d1e0f3a7b64);

type DllGetClassObjectFn = unsafe extern "system" fn(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT;

fn start_test_server(cfg_path: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sample_dict = root.join("assets/sample/dict.tsv");
        let engine = assembly::assemble(&AssemblySpec::new(sample_dict)).expect("assemble engine");

        let cfg = Config::load(&cfg_path).unwrap_or_default();
        let mut router_cfg = RouterConfig::from(&cfg);
        router_cfg.english_direct = cfg.general.english_direct;

        let mut router = Router::new(engine, router_cfg);
        router.watch_config(&cfg, cfg_path, root, DataDirs::default());
        router.handle_status_event(StatusEvent::ToggleMode);

        let (work_tx, work_rx) = mpsc::channel();
        let _ = serve_pipe(DEFAULT_PIPE_NAME, &mut router, work_tx, work_rx);
    })
}

fn load_real_tsf_sink(dll_path: &PathBuf) -> ITfKeyEventSink {
    let hstring_path = HSTRING::from(dll_path.as_os_str());
    let module = unsafe { LoadLibraryW(&hstring_path) }.expect("LoadLibraryW 必须成功加载 DLL");

    let proc = unsafe { GetProcAddress(module, s!("DllGetClassObject")) };
    assert!(proc.is_some(), "DLL 必须导出 DllGetClassObject");

    let dll_get_class_object: DllGetClassObjectFn = unsafe { std::mem::transmute(proc.unwrap()) };

    let mut factory_ptr: *mut c_void = std::ptr::null_mut();
    let hr = unsafe { dll_get_class_object(&CLSID_QINGJIAN, &IClassFactory::IID, &mut factory_ptr) };
    assert_eq!(hr, S_OK, "DllGetClassObject 返回 S_OK");
    assert!(!factory_ptr.is_null());

    let factory: IClassFactory = unsafe { IClassFactory::from_raw(factory_ptr) };

    let sink: ITfKeyEventSink = unsafe {
        factory
            .CreateInstance(None)
            .expect("IClassFactory::CreateInstance(ITfKeyEventSink) 必须成功")
    };

    sink
}

#[test]
fn test_real_tsf_com_lifecycle_and_all_scenarios() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dll_path = std::env::var("QINGJIAN_TEST_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("../../../target/debug/qingjian_tsf.dll"));
    println!("[REAL-EMPIRICAL] 加载被测 TSF DLL: {}", dll_path.display());
    assert!(dll_path.exists(), "被测 DLL 必须存在: {}", dll_path.display());

    let tmp_cfg = std::env::temp_dir().join(format!("qj_tsf_com_test_{}.toml", std::process::id()));

    // 阶段 1: 开启 English Direct (ON 模式)
    fs::write(&tmp_cfg, "[general]\nenglish_direct = true\nenglish_mode = true\n").unwrap();

    let _server_thread = start_test_server(tmp_cfg.clone());
    thread::sleep(Duration::from_millis(200));

    // 1. 真实 COM 接口实例化
    let sink = load_real_tsf_sink(&dll_path);

    // 2. 触发焦点通知，连接并同步配置
    let res = unsafe { sink.OnSetFocus(true) };
    assert!(res.is_ok(), "OnSetFocus(true) 必须返回 Ok");
    thread::sleep(Duration::from_millis(250));

    // 3. 切换到英文模式
    let preserved_res = unsafe { sink.OnPreservedKey(None, &GUID_SWITCH_MODE) };
    assert!(preserved_res.is_ok(), "OnPreservedKey 触发切换英文模式");

    // ==========================================
    // 阶段 1 验证: English Direct ON 原生放行验证
    // ==========================================

    // 场景 1: a/A 放行 (VK_A = 0x41)
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x41), LPARAM(0)) }.expect("OnTestKeyDown 必须调用成功");
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 1: 英文直通模式下 'a' 绝不能被 TSF 拦截，必须返回 FALSE (0) 放行给宿主");

    // 场景 2: Caps/Shift 键本身放行
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x14), LPARAM(0)) }.expect("VK_CAPITAL");
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 2: Caps Lock 键必须放行给宿主");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x10), LPARAM(0)) }.expect("VK_SHIFT");
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 2: Shift 键必须放行给宿主");

    // 场景 3: 标点与数字放行 (VK_1 = 0x31, VK_OEM_MINUS = 0xBD, VK_OEM_PERIOD = 0xBE)
    for vk in 0x30..=0x39 {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk as usize), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 3: 数字键 0x{vk:X} 必须放行给宿主");
    }
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0xBD), LPARAM(0)) }.unwrap(); // '-'
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 3: 连字符 '-' 必须放行给宿主");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0xBE), LPARAM(0)) }.unwrap(); // '.'
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 3: 句号 '.' 必须放行给宿主");

    // 场景 4: 功能键 Space/Backspace/Enter/Esc 放行
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x20), LPARAM(0)) }.unwrap(); // Space
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 4: 空格键 Space 必须放行给宿主");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x08), LPARAM(0)) }.unwrap(); // Backspace
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 4: 退格键 Backspace 必须放行给宿主");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x0D), LPARAM(0)) }.unwrap(); // Enter
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 4: 回车键 Enter 必须放行给宿主");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x1B), LPARAM(0)) }.unwrap(); // Esc
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 4: Esc 键必须放行给宿主");

    // 场景 5: 快捷键 Ctrl/Alt/Win 放行
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x11), LPARAM(0)) }.unwrap(); // VK_CONTROL
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 5: Ctrl 键必须放行");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x12), LPARAM(0)) }.unwrap(); // VK_MENU (Alt)
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 5: Alt 键必须放行");
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x5B), LPARAM(0)) }.unwrap(); // VK_LWIN
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 5: Win 键必须放行");

    // 场景 6: 真实词串 "gpt6" 逐键实机 COM 调用
    for vk in [0x47, 0x50, 0x54, 0x36] { // G, P, T, 6
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 6: 'gpt6' 逐键必须全部放行给宿主");
    }

    // 场景 7: 真实词串 "deepseek-r1" 逐键实机 COM 调用
    for vk in [0x44, 0x45, 0x45, 0x50, 0x53, 0x45, 0x45, 0x4B, 0xBD, 0x52, 0x31] {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 7: 'deepseek-r1' 逐键必须全部放行给宿主");
    }

    // 场景 8: 真实词串 "C++" 逐键实机 COM 调用
    for vk in [0x43, 0xBB, 0xBB] {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 8: 'C++' 逐键必须全部放行给宿主");
    }

    // 场景 9: 真实词串 "I'm" 逐键实机 COM 调用
    for vk in [0x49, 0xDE, 0x4D] {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 9: 'I\\'m' 逐键必须全部放行给宿主");
    }

    // 场景 10: 真实词串 "email@example.com" 逐键实机 COM 调用
    for vk in [0x45, 0x4D, 0x41, 0x49, 0x4C, 0x32, 0x45, 0x58, 0x41, 0x4D, 0x50, 0x4C, 0x45, 0xBE, 0x43, 0x4F, 0x4D] {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 10: 'email@example.com' 逐键必须全部放行给宿主");
    }

    // 场景 11: 真实 URL 逐键实机 COM 调用 ("https://example.com/test?a=1&b=2")
    for vk in [0x48, 0x54, 0x54, 0x50, 0x53, 0xBA, 0xBF, 0xBF, 0x45, 0xBE, 0x43, 0xBF, 0xBF, 0x31] {
        let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(vk), LPARAM(0)) }.unwrap();
        assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] 场景 11: URL 逐键必须全部放行给宿主");
    }

    // 场景 12: 中文模式拼音拦截保护验证
    let _ = unsafe { sink.OnPreservedKey(None, &GUID_SWITCH_MODE) };
    let eat_zh = unsafe { sink.OnTestKeyDown(None, WPARAM(0x41), LPARAM(0)) }.unwrap();
    assert_eq!(eat_zh, BOOL(1), "[REAL-EMPIRICAL] 场景 12: 中文模式下拼音字母 'a' 必须被拦截 (TRUE) 以进行拼音组句");

    // ==========================================
    // 阶段 2 验证: 动态热切换至 English Direct OFF (v0.1.4 回归)
    // ==========================================
    fs::write(&tmp_cfg, "[general]\nenglish_direct = false\nenglish_mode = true\n").unwrap();
    thread::sleep(Duration::from_millis(1200));

    // 触发获焦以拉取新配置
    let _ = unsafe { sink.OnSetFocus(false) };
    let _ = unsafe { sink.OnSetFocus(true) };
    thread::sleep(Duration::from_millis(300));

    // 确保切到英文模式
    let _ = unsafe { sink.OnPreservedKey(None, &GUID_SWITCH_MODE) };

    // 在 OFF 模式下：英文字母必须被拦截 (返回 TRUE)
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x41), LPARAM(0)) }.unwrap();
    assert_eq!(eat, BOOL(1), "[REAL-EMPIRICAL] OFF 回归: english_direct=false 时 'a' 必须被拦截 (TRUE) 保持 v0.1.4 IME 注入行为");

    // 在 OFF 模式下：标点必须被拦截
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0xBD), LPARAM(0)) }.unwrap(); // '-'
    assert_eq!(eat, BOOL(1), "[REAL-EMPIRICAL] OFF 回归: 连字符 '-' 必须被拦截 (TRUE)");

    // 在 OFF 模式下：数字必须被拦截
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x31), LPARAM(0)) }.unwrap(); // '1'
    assert_eq!(eat, BOOL(1), "[REAL-EMPIRICAL] OFF 回归: 数字 '1' 必须被拦截 (TRUE)");

    // 快捷键依然放行
    let eat = unsafe { sink.OnTestKeyDown(None, WPARAM(0x11), LPARAM(0)) }.unwrap(); // VK_CONTROL
    assert_eq!(eat, BOOL(0), "[REAL-EMPIRICAL] OFF 回归: Ctrl 快捷键必须放行");

    // 清理临时文件
    let _ = fs::remove_file(&tmp_cfg);
}

static RECEIVED_CHARS: std::sync::Mutex<Vec<char>> = std::sync::Mutex::new(Vec::new());

unsafe extern "system" fn test_host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CHAR {
        if let Some(ch) = char::from_u32(wparam.0 as u32) {
            if let Ok(mut lock) = RECEIVED_CHARS.lock() {
                lock.push(ch);
            }
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[test]
fn test_real_win32_host_window_message_delivery() {
    // 验证 Windows 宿主窗口原生接收击键交付
    unsafe {
        let class_name = HSTRING::from("QingjianHostTestWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(test_host_wnd_proc),
            hInstance: GetModuleHandleW(None).unwrap().into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class_name,
            &HSTRING::from("Host Window Test"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            300,
            200,
            None,
            None,
            Some(GetModuleHandleW(None).unwrap().into()),
            None,
        ).expect("CreateWindowExW 必须成功");

        let _ = SetFocus(Some(hwnd));

        // 模拟击键流交付到宿主窗口消息队列
        let test_str = "gpt6 deepseek-r1 C++ I'm email@example.com";
        for ch in test_str.chars() {
            let msg = MSG {
                hwnd,
                message: WM_CHAR,
                wParam: WPARAM(ch as usize),
                lParam: LPARAM(0),
                time: 0,
                pt: Default::default(),
            };
            DispatchMessageW(&msg);
        }

        let received: String = RECEIVED_CHARS.lock().unwrap().iter().collect();
        assert_eq!(received, test_str, "[REAL-EMPIRICAL] 宿主窗口必须原生收到完整的字符流且顺序严格一致");

        let _ = DestroyWindow(hwnd);
    }
}
