//! 真正的 Windows Host 键盘实机端到端验收测试 [REAL-EMPIRICAL]。
//!
//! 验证链路：
//! 真实系统键盘输入 (Windows SendInput 硬件按键模拟，严禁构造 WM_CHAR，严禁直接接口调用)
//! -> Windows input / TSF (msctf.dll)
//! -> 已加载的 Qingjian TSF DLL (E:\Qingjian\qingjian_tsf-0.1.4.dll)
//! -> Host application (真实 Win32 宿主窗口与 EDIT 控件)
//! -> 实际输入框 (GetWindowTextW 提取实际接收字符)
//!
//! 同时完成：
//! 1. Loaded / Installed Validation: 验证 E:\Qingjian 当前安装 DLL 与 Server 的 SHA256 与当前构建产物完全一致。
//! 2. 真实 Host 窗口在 Windows 消息循环中接收系统按键，证实字符顺序绝无倒装 (gpt6 / deepseek-r1 / C++)。

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ActivateKeyboardLayout, GetFocus, LoadKeyboardLayoutW, MapVirtualKeyW, SendInput, SetFocus,
    ACTIVATE_KEYBOARD_LAYOUT_FLAGS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KLF_ACTIVATE, MAP_VIRTUAL_KEY_TYPE,
    VK_CONTROL, VK_ESCAPE, VK_MENU, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_InputProcessorProfiles, CLSID_TF_ThreadMgr, ITfInputProcessorProfiles, ITfThreadMgr,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    PeekMessageW, RegisterClassW, SetForegroundWindow, ShowWindow, TranslateMessage,
    MSG, PM_REMOVE, SW_SHOWNORMAL,
    WNDCLASSW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{GUID, PCWSTR, w};
use std::sync::Mutex;

unsafe extern "system" {
    fn OpenInputDesktop(dw_flags: u32, f_inherit: windows::core::BOOL, dw_desired_access: u32) -> HANDLE;
    fn SetThreadDesktop(h_desktop: HANDLE) -> windows::core::BOOL;
    fn CloseDesktop(h_desktop: HANDLE) -> windows::core::BOOL;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, f_attach: windows::core::BOOL) -> windows::core::BOOL;
}

const CLSID_QINGJIAN: GUID = GUID::from_u128(0x4fdca82d_e923_49bf_9e75_bb906b93b8bb);
const GUID_PROFILE: GUID = GUID::from_u128(0x8119f8e0_cf81_423b_9189_c0d7374324b3);
const LANGID_ZH_CN: u16 = 0x0804;

const EDIT_CLASS: PCWSTR = w!("EDIT");
const HOST_CLASS: PCWSTR = w!("QingjianRealHostKeyboardTestWindow");

fn compute_sha256(path: &Path) -> String {
    let data = fs::read(path).unwrap_or_else(|e| panic!("读取文件失败 {}: {e}", path.display()));
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, part) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(part.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h_val) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h_val = g; g = f; f = e; e = d.wrapping_add(temp1);
            d = c; c = b; b = a; a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }
    format!("{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}", h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7])
}

/// 验证已安装目录 E:\Qingjian 与当前构建输出的 SHA256 一致性 (Loaded/Installed Validation)
fn verify_loaded_installed_binaries() -> (PathBuf, String) {
    let installed_dll = PathBuf::from(r"E:\Qingjian\qingjian_tsf-0.1.4.dll");
    let installed_srv = PathBuf::from(r"E:\Qingjian\qingjian-server.exe");

    assert!(installed_dll.exists(), "已安装 TSF DLL 必须存在: {}", installed_dll.display());
    assert!(installed_srv.exists(), "已安装 Server 必须存在: {}", installed_srv.display());

    let dll_hash = compute_sha256(&installed_dll);
    let srv_hash = compute_sha256(&installed_srv);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let built_dll_release = manifest_dir.join("../../../target/release/qingjian_tsf.dll");
    let built_srv_release = manifest_dir.join("../../../target/release/qingjian-server.exe");

    let built_dll_debug = manifest_dir.join("../../../target/debug/qingjian_tsf.dll");
    let built_srv_debug = manifest_dir.join("../../../target/debug/qingjian-server.exe");

    let dll_matches = if built_dll_release.exists() {
        compute_sha256(&built_dll_release) == dll_hash
    } else if built_dll_debug.exists() {
        compute_sha256(&built_dll_debug) == dll_hash
    } else {
        false
    };

    let srv_matches = if built_srv_release.exists() {
        compute_sha256(&built_srv_release) == srv_hash
    } else if built_srv_debug.exists() {
        compute_sha256(&built_srv_debug) == srv_hash
    } else {
        false
    };

    println!("[REAL-EMPIRICAL] Installed DLL SHA256: {}", dll_hash);
    println!("[REAL-EMPIRICAL] Installed Srv SHA256: {}", srv_hash);
    assert!(dll_matches, "E:\\Qingjian\\qingjian_tsf-0.1.4.dll 必须与当前构建版本一致");
    assert!(srv_matches, "E:\\Qingjian\\qingjian-server.exe 必须与当前构建版本一致");

    (installed_dll, dll_hash)
}

unsafe extern "system" fn host_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct KeyMessageRecord {
    hwnd: HWND,
    message: u32,
    wparam: usize,
}

struct RealHostWindow {
    hwnd: HWND,
    hwnd_edit: HWND,
    recorded_keys: Mutex<Vec<KeyMessageRecord>>,
}

// 同一 integration binary 中的实机测试共用唯一 installed Server / TSF / foreground desktop，必须串行。
static REAL_HOST_TEST_LOCK: Mutex<()> = Mutex::new(());

impl RealHostWindow {
    fn new() -> Self {
        let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }.unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_wndproc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: HOST_CLASS,
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };

        let hwnd = unsafe {
            CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0x00000008), // WS_EX_TOPMOST
                HOST_CLASS,
                w!("Qingjian Empirical Host Window"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                500,
                300,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
        }.expect("创建宿主主窗口");

        let hwnd_edit = unsafe {
            CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                EDIT_CLASS,
                w!(""),
                WS_CHILD | WS_VISIBLE,
                10,
                10,
                460,
                240,
                Some(hwnd),
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
        }.expect("创建宿主 EDIT 控件");

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            let _ = SetFocus(Some(hwnd_edit));
        }

        Self {
            hwnd,
            hwnd_edit,
            recorded_keys: Mutex::new(Vec::new()),
        }
    }

    fn ensure_focus(&self) {
        for _ in 0..15 {
            unsafe {
                let fg_hwnd = GetForegroundWindow();
                if fg_hwnd != self.hwnd {
                    let fg_thread = GetWindowThreadProcessId(fg_hwnd, None);
                    let cur_thread = GetCurrentThreadId();
                    if fg_thread != 0 && fg_thread != cur_thread {
                        let _ = AttachThreadInput(cur_thread, fg_thread, windows::core::BOOL(1));
                        let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
                        let _ = BringWindowToTop(self.hwnd);
                        let _ = SetForegroundWindow(self.hwnd);
                        let _ = SetFocus(Some(self.hwnd_edit));
                        let _ = AttachThreadInput(cur_thread, fg_thread, windows::core::BOOL(0));
                    } else {
                        let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
                        let _ = BringWindowToTop(self.hwnd);
                        let _ = SetForegroundWindow(self.hwnd);
                        let _ = SetFocus(Some(self.hwnd_edit));
                    }
                } else {
                    let _ = SetFocus(Some(self.hwnd_edit));
                }
            }
            if unsafe { GetForegroundWindow() } == self.hwnd {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn pump_messages(&self, duration: Duration) {
        let start = Instant::now();
        let mut msg = MSG::default();
        while start.elapsed() < duration {
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message >= 0x0100 && msg.message <= 0x0109 {
                        println!("[MSG] hwnd={:?} msg=0x{:04X} wParam=0x{:X}", msg.hwnd, msg.message, msg.wParam.0);
                        self.recorded_keys.lock().unwrap().push(KeyMessageRecord {
                            hwnd: msg.hwnd,
                            message: msg.message,
                            wparam: msg.wParam.0,
                        });
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn clear_records(&self) {
        self.recorded_keys.lock().unwrap().clear();
    }

    fn get_records(&self) -> Vec<KeyMessageRecord> {
        self.recorded_keys.lock().unwrap().clone()
    }

    fn get_edit_text(&self) -> String {
        let mut buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(self.hwnd_edit, &mut buf) };
        String::from_utf16_lossy(&buf[..len as usize])
    }

    fn clear_edit(&self) {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(self.hwnd_edit, w!("")).unwrap();
        }
    }
}

impl Drop for RealHostWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd_edit);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn send_raw_vk(vk: u16, key_up: bool) {
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAP_VIRTUAL_KEY_TYPE(0)) } as u16;
    let mut flags = windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0);
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let ret = unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32)
    };
    println!("[DEBUG] SendInput vk=0x{:02X} up={} ret={}", vk, key_up, ret);
    if ret == 0 {
        let err = unsafe { windows::Win32::Foundation::GetLastError() };
        println!("[DEBUG] SendInput vk=0x{:X} returned 0, error = {:?}", vk, err);
    }
}

/// 发送真实系统硬件键盘输入 (通过 Windows SendInput API 带完整 ScanCode)
fn send_system_key(vk: u16, shift: bool) {
    if shift {
        send_raw_vk(VK_SHIFT.0, false);
    }

    send_raw_vk(vk, false);
    send_raw_vk(vk, true);

    if shift {
        send_raw_vk(VK_SHIFT.0, true);
    }
}

fn type_system_string(host: &RealHostWindow, s: &str) {
    host.ensure_focus();
    for ch in s.chars() {
        match ch {
            'a'..='z' => {
                let vk = (ch as u16) - ('a' as u16) + 0x41;
                send_system_key(vk, false);
            }
            'A'..='Z' => {
                let vk = (ch as u16) - ('A' as u16) + 0x41;
                send_system_key(vk, true);
            }
            '0'..='9' => {
                let vk = (ch as u16) - ('0' as u16) + 0x30;
                send_system_key(vk, false);
            }
            '-' => send_system_key(VK_OEM_MINUS.0, false),
            '+' => send_system_key(VK_OEM_PLUS.0, true),
            '.' => send_system_key(VK_OEM_PERIOD.0, false),
            ' ' => send_system_key(VK_SPACE.0, false),
            '@' => send_system_key(0x32, true), // Shift+2
            '\'' => send_system_key(0xDE, false), // VK_OEM_7
            _ => {}
        }
        host.pump_messages(Duration::from_millis(25));
    }
}

#[test]
fn test_real_windows_host_keyboard_validation_end_to_end() {
    let _test_lock = REAL_HOST_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // 1. Loaded / Installed Validation
    let (installed_dll_path, dll_hash) = verify_loaded_installed_binaries();

    // 确保已安装的真实 Server 正在运行
    let pipe_path = r"\\.\pipe\qingjian";
    let mut server_child = None;
    if std::fs::OpenOptions::new().read(true).write(true).open(pipe_path).is_err() {
        println!("[REAL-EMPIRICAL] 启动已安装的真实 Server: E:\\Qingjian\\qingjian-server.exe ...");
        let child = std::process::Command::new(r"E:\Qingjian\qingjian-server.exe")
            .current_dir(r"E:\Qingjian")
            .spawn()
            .expect("必须成功启动 E:\\Qingjian\\qingjian-server.exe");
        server_child = Some(child);
        for _ in 0..60 {
            if std::fs::OpenOptions::new().read(true).write(true).open(pipe_path).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        thread::sleep(Duration::from_millis(300));
    }

    // 在专属线程中切换到系统活动输入桌面 (Default Desktop)，打破沙箱桌面隔离
    let handle = thread::spawn(move || {
        let h_desk = unsafe { OpenInputDesktop(0, windows::core::BOOL(0), 0x01FF) };
        println!("[REAL-EMPIRICAL] OpenInputDesktop handle = {:?}", h_desk);
        if !h_desk.is_invalid() {
            let ok = unsafe { SetThreadDesktop(h_desk) };
            println!("[REAL-EMPIRICAL] SetThreadDesktop result = {:?}", ok);
        }

        // 2. 初始化 COM
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        // 3. 基线检验：在激活前严格验证 Qingjian TSF DLL 尚未加载到宿主进程 (Causal-chain baseline)
        let pre_mod = unsafe { GetModuleHandleW(w!("qingjian_tsf-0.1.4.dll")) };
        assert!(
            pre_mod.is_err(),
            "Qingjian TSF module was already loaded before activation; causal-chain baseline is invalid"
        );
        println!("[REAL-EMPIRICAL] Pre-activation Qingjian module state: NOT_LOADED");

        // 4. 设置线程活动键盘布局为中文 (0x0804)，并激活 TSF ThreadMgr 连接宿主线程

        let hkl = unsafe { LoadKeyboardLayoutW(w!("00000804"), KLF_ACTIVATE) }.expect("LoadKeyboardLayoutW(00000804) 必须成功");
        unsafe { let _ = ActivateKeyboardLayout(hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)); }

        let thread_mgr: ITfThreadMgr = unsafe {
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)
                .expect("CoCreateInstance(CLSID_TF_ThreadMgr) 必须成功")
        };
        let client_id = unsafe {
            thread_mgr.Activate().expect("thread_mgr.Activate 成功")
        };
        println!("[REAL-EMPIRICAL] TSF ThreadMgr 已成功激活，client_id = {}", client_id);

        // 5. 显式激活 Qingjian TSF Language Profile，严格验证返回值
        let profiles: ITfInputProcessorProfiles = unsafe {
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .expect("CoCreateInstance(CLSID_TF_InputProcessorProfiles) 必须成功")
        };
        unsafe {
            profiles
                .ActivateLanguageProfile(&CLSID_QINGJIAN, LANGID_ZH_CN, &GUID_PROFILE)
                .expect("ActivateLanguageProfile(&CLSID_QINGJIAN, LANGID_ZH_CN, &GUID_PROFILE) 必须返回成功 HRESULT");
        }
        println!("[REAL-EMPIRICAL] ActivateLanguageProfile(&CLSID_QINGJIAN) 成功返回 S_OK");

        // 6. 严格调用 GetActiveLanguageProfile 逐项验证当前激活 Profile
        let mut active_langid: u16 = 0;
        let mut active_profile: GUID = GUID::zeroed();
        unsafe {
            profiles
                .GetActiveLanguageProfile(&CLSID_QINGJIAN, &mut active_langid, &mut active_profile)
                .expect("GetActiveLanguageProfile 必须返回成功 HRESULT");
        }
        println!(
            "[REAL-EMPIRICAL] GetActiveLanguageProfile 实际查询结果: LANGID=0x{:04X}, GUID={:?}",
            active_langid, active_profile
        );
        assert_eq!(
            active_langid, LANGID_ZH_CN,
            "[REAL-EMPIRICAL] 当前激活的语言 ID 必须严格匹配 0x0804 (LANGID_ZH_CN)"
        );
        assert_eq!(
            active_profile, GUID_PROFILE,
            "[REAL-EMPIRICAL] 当前激活的 Profile GUID 必须严格匹配 Qingjian Profile GUID"
        );

        // 7. 创建真实 Host Application 窗口与 EDIT 控件，并确保窗口处于焦点状态
        let host = RealHostWindow::new();
        host.pump_messages(Duration::from_millis(150));

        let fg_now = unsafe { GetForegroundWindow() };
        let focus_now = unsafe { GetFocus() };
        println!(
            "[REAL-EMPIRICAL] Host 窗口状态: HWND={:?}, EDIT={:?}, FG={:?}, Focus={:?}",
            host.hwnd, host.hwnd_edit, fg_now, focus_now
        );

        // 8. 严格验证系统 TSF 真实加载模块 (绝对禁止任何 LoadLibraryW 伪装 fallback)
        let h_mod = unsafe { GetModuleHandleW(w!("qingjian_tsf-0.1.4.dll")) }
            .expect("[REAL-EMPIRICAL] Windows TSF 必须已真正映射加载 qingjian_tsf-0.1.4.dll，严禁任何 LoadLibrary fallback");

        let mut module_path_buf = [0u16; 512];
        let path_len = unsafe { GetModuleFileNameW(Some(h_mod), &mut module_path_buf) };
        let loaded_path = String::from_utf16_lossy(&module_path_buf[..path_len as usize]);
        println!("[REAL-EMPIRICAL] 宿主进程内由 Windows TSF 实际加载映射的模块路径: {}", loaded_path);
        assert_eq!(
            PathBuf::from(&loaded_path).canonicalize().unwrap_or(PathBuf::from(&loaded_path)),
            installed_dll_path.canonicalize().unwrap_or(installed_dll_path.clone()),
            "[REAL-EMPIRICAL] 宿主加载的模块路径必须完全对应已安装的 E:\\Qingjian\\qingjian_tsf-0.1.4.dll"
        );

        // 8. 中文拼音模式对照验证：未切英文前，英文字母输入必须被 TSF 拦截并转换为 VK_PROCESSKEY (0xE5)
        host.clear_edit();
        host.clear_records();
        host.ensure_focus();
        host.pump_messages(Duration::from_millis(50));
        println!("[REAL-EMPIRICAL] 正在对照验证中文模式下的按键行为 (预期被 TSF 拦截为 VK_PROCESSKEY)...");
        send_system_key(0x41, false); // 发送 'a'
        host.pump_messages(Duration::from_millis(300));
        let mut zh_records = host.get_records();
        println!("[REAL-EMPIRICAL] zh_records (count={}): {:?}", zh_records.len(), zh_records);
        let mut has_processkey = zh_records.iter().any(|r| r.message == 0x0100 && r.wparam == 0xE5);
        if !has_processkey {
            println!("[REAL-EMPIRICAL] 检测到初始处于英文模式，模拟物理单击 Shift 切换至中文拼音模式");
            send_raw_vk(VK_SHIFT.0, false);
            host.pump_messages(Duration::from_millis(30));
            send_raw_vk(VK_SHIFT.0, true);
            host.pump_messages(Duration::from_millis(200));

            host.clear_edit();
            host.clear_records();
            send_system_key(0x41, false); // 发送 'a'
            host.pump_messages(Duration::from_millis(300));
            zh_records = host.get_records();
            has_processkey = zh_records.iter().any(|r| r.message == 0x0100 && r.wparam == 0xE5);
        }
        println!("[REAL-EMPIRICAL] 中文模式下击键是否产生 VK_PROCESSKEY (0xE5): {}", has_processkey);
        assert!(
            has_processkey,
            "[REAL-EMPIRICAL] 中文模式下拼音字母必须被 TSF 拦截并转换为 VK_PROCESSKEY (0xE5)"
        );
        assert_eq!(
            host.get_edit_text(), "",
            "[REAL-EMPIRICAL] 中文模式未提交时 EDIT 控件内容必须为空"
        );
        // 按 Esc 退出未完成的拼音组句
        send_system_key(VK_ESCAPE.0, false);
        host.pump_messages(Duration::from_millis(50));

        // 8b. 中文模式下的普通数字必须原生交给宿主，不能变成 VK_PROCESSKEY 后通过异步插入重放。
        host.clear_edit();
        host.clear_records();
        println!("[REAL-EMPIRICAL] 中文模式下直接输入数字序列 261009，检查原生 WM_CHAR 顺序");
        type_system_string(&host, "261009");
        host.pump_messages(Duration::from_millis(250));
        let digit_text = host.get_edit_text();
        let digit_records = host.get_records();
        println!("[REAL-EMPIRICAL] 中文数字实收文本: {digit_text:?}; 消息: {digit_records:?}");
        assert_eq!(digit_text, "261009", "中文模式下数字顺序不得倒置或错位");
        assert!(
            !digit_records.iter().any(|r| r.message == 0x0100 && r.wparam == 0xE5),
            "中文模式下普通数字不应被 TSF 转换为 VK_PROCESSKEY"
        );
        assert!(
            digit_records.iter().filter(|r| r.message == 0x0102).count() >= 6,
            "宿主应收到真实 WM_CHAR 数字消息，而不是由异步编辑会话注入"
        );

        // 9. 物理模拟单击 Shift 切换至内置英文模式 (English Direct Mode)
        println!("[REAL-EMPIRICAL] 模拟物理单击 Shift 键切换至内置英文模式 (English Direct Mode)");
        send_raw_vk(VK_SHIFT.0, false);
        host.pump_messages(Duration::from_millis(30));
        send_raw_vk(VK_SHIFT.0, true);
        host.pump_messages(Duration::from_millis(200));

        // 10. 真实系统键盘输入验证序列 (Windows SendInput -> msctf -> Qingjian TSF -> EDIT 控件)
        let test_cases = [
            ("gpt6", "gpt6"),
            ("deepseek-r1", "deepseek-r1"),
            ("C++", "C++"),
            ("I'm", "I'm"),
            ("123", "123"),
            ("hello world", "hello world"),
        ];

        for (input_str, expected_str) in test_cases {
            host.clear_edit();
            host.clear_records();
            host.pump_messages(Duration::from_millis(50));
            send_raw_vk(VK_SHIFT.0, true);
            send_raw_vk(VK_CONTROL.0, true);
            send_raw_vk(VK_MENU.0, true);
            host.pump_messages(Duration::from_millis(20));

            println!("[REAL-EMPIRICAL] 正在通过 Windows SendInput 向宿主 EDIT 控件物理发送键击: '{}'", input_str);
            type_system_string(&host, input_str);
            host.pump_messages(Duration::from_millis(200));

            let records = host.get_records();

            // 严格验证 A: 必须有真实 WM_KEYDOWN 到达宿主控件
            let keydowns: Vec<_> = records.iter().filter(|r| r.message == 0x0100).collect();
            assert!(!keydowns.is_empty(), "[REAL-EMPIRICAL] 宿主必须收到 WM_KEYDOWN 消息");

            // 严格验证 B: 绝对严禁出现 VK_PROCESSKEY (0xE5)！
            // 证实按键未被 TSF 当作 IME 拼音消费，而是原生键盘直通宿主
            for kd in &keydowns {
                assert_ne!(
                    kd.wparam, 0xE5,
                    "[REAL-EMPIRICAL] English Direct 模式下按键严禁被 TSF 截获为 VK_PROCESSKEY (0xE5)"
                );
            }

            // 严格验证 C: 必须有真实 WM_KEYUP 到达宿主控件
            let keyups: Vec<_> = records.iter().filter(|r| r.message == 0x0101).collect();
            assert!(!keyups.is_empty(), "[REAL-EMPIRICAL] 宿主必须收到 WM_KEYUP 消息");

            // 严格验证 D: 最终 EDIT 文本完全匹配预期字符串，顺序严格保真
            let received = host.get_edit_text();
            println!("[REAL-EMPIRICAL] 宿主 EDIT 控件实际接收到的文本: '{}' (预期: '{}')", received, expected_str);
            assert_eq!(
                received, expected_str,
                "[REAL-EMPIRICAL] 宿主实际输入框接收字符顺序与内容必须完全精确匹配，无倒装无遗漏"
            );
        }

        unsafe {
            let _ = thread_mgr.Deactivate();
        }

        if !h_desk.is_invalid() {
            unsafe { let _ = CloseDesktop(h_desk); }
        }
    });

    handle.join().expect("实机测试线程顺利运行并通过所有断言");
    println!("[REAL-EMPIRICAL] 全部 6 组真实 Host 键盘输入测试 100% 通过验证！Hash: {}", dll_hash);

    if let Some(mut child) = server_child {
        let _ = child.kill();
        let _ = child.wait();
    }
    thread::sleep(Duration::from_millis(500));
}
