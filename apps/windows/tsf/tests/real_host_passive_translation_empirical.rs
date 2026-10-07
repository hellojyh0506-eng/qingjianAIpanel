//! 真正的 Windows Host 被动翻译 (Passive Translation) 实机端到端验收测试 [REAL-EMPIRICAL]。
//!
//! 验证链路：
//! 1. Windows Host 真实键盘物理按键序列 (SendInput: "kaifa")
//! 2. 真实系统 TSF (msctf.dll) -> 已加载的 Qingjian TSF DLL (E:\Qingjian\qingjian_tsf-0.1.4.dll)
//! 3. TSF IPC -> 真实运行的后台安装版服务进程 (E:\Qingjian\qingjian-server.exe)
//! 4. 候选窗口 QingjianCandidateWindow 真实弹出显示（包含中文候选「开发」与被动翻译 annotation "develop"）
//! 5. 阶段 A（被动学习展示态）：Host EDIT 控件内容必须为空，被动翻译绝不自动上屏
//! 6. 阶段 B（Space 正常提交）：物理按 Space 键，Host EDIT 控件接收到的内容必须严格为 "开发"（中文候选），英文翻译绝不上屏
//! 7. 阶段 C（Ctrl+1 显式学习提交）：再次输入 "kaifa"，物理按下真实快捷键 Ctrl+1，Host EDIT 控件接收到的内容必须严格为 "develop"（被动翻译第一条 sense 上屏）

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ActivateKeyboardLayout, LoadKeyboardLayoutW, MapVirtualKeyW, SendInput, SetFocus,
    ACTIVATE_KEYBOARD_LAYOUT_FLAGS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KLF_ACTIVATE, MAP_VIRTUAL_KEY_TYPE,
    VK_ESCAPE, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_InputProcessorProfiles, CLSID_TF_ThreadMgr, ITfInputProcessorProfiles, ITfThreadMgr,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, GetWindowRect,
    IsWindowVisible, PeekMessageW, RegisterClassW, SetForegroundWindow, ShowWindow, TranslateMessage,
    MSG, PM_REMOVE, SW_SHOWNORMAL,
    WNDCLASSW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{GUID, PCWSTR, w};

unsafe extern "system" {
    fn OpenInputDesktop(dw_flags: u32, f_inherit: windows::core::BOOL, dw_desired_access: u32) -> HANDLE;
    fn SetThreadDesktop(h_desktop: HANDLE) -> windows::core::BOOL;
    fn CloseDesktop(h_desktop: HANDLE) -> windows::core::BOOL;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, f_attach: windows::core::BOOL) -> windows::core::BOOL;
    fn GetThreadDesktop(dw_thread_id: u32) -> HANDLE;
    fn EnumDesktopWindows(
        h_desktop: HANDLE,
        lpfn: Option<unsafe extern "system" fn(HWND, LPARAM) -> windows::core::BOOL>,
        lparam: LPARAM,
    ) -> windows::core::BOOL;
}

const CLSID_QINGJIAN: GUID = GUID::from_u128(0x4fdca82d_e923_49bf_9e75_bb906b93b8bb);
const GUID_PROFILE: GUID = GUID::from_u128(0x8119f8e0_cf81_423b_9189_c0d7374324b3);
const LANGID_ZH_CN: u16 = 0x0804;

const EDIT_CLASS: PCWSTR = w!("EDIT");
const HOST_CLASS: PCWSTR = w!("QingjianPassiveTranslationEmpiricalHostWindow");

static ORIGINAL_DESKTOP: Mutex<Option<isize>> = Mutex::new(None);

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

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
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
    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
    )
}

fn verify_loaded_installed_binaries() -> (PathBuf, String) {
    let installed_dll = PathBuf::from(r"E:\Qingjian\qingjian_tsf-0.1.4.dll");
    let installed_srv = PathBuf::from(r"E:\Qingjian\qingjian-server.exe");
    assert!(installed_dll.exists(), "E:\\Qingjian\\qingjian_tsf-0.1.4.dll 必须存在");
    assert!(installed_srv.exists(), "E:\\Qingjian\\qingjian-server.exe 必须存在");

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

    println!("[REAL-EMPIRICAL] Phase 3 Installed DLL SHA256: {}", dll_hash);
    println!("[REAL-EMPIRICAL] Phase 3 Installed Srv SHA256: {}", srv_hash);
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
                w!("Qingjian Empirical Passive Translation Host Window"),
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
            '\'' => send_system_key(0xDE, false),
            ' ' => send_system_key(VK_SPACE.0, false),
            _ => panic!("未映射的按键字符: {}", ch),
        }
        host.pump_messages(Duration::from_millis(30));
    }
}

fn find_window_by_class(class_name: &str) -> Option<HWND> {
    struct Search<'a> {
        target: &'a str,
        found: Option<HWND>,
    }
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search<'_>) };
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        if len > 0 {
            let class = String::from_utf16_lossy(&buf[..len as usize]);
            if class == search.target {
                search.found = Some(hwnd);
                return windows::core::BOOL(0);
            }
        }
        windows::core::BOOL(1)
    }

    let mut search = Search {
        target: class_name,
        found: None,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut search as *mut Search<'_> as isize),
        );
    }
    if search.found.is_none() {
        if let Some(desk_val) = *ORIGINAL_DESKTOP.lock().unwrap() {
            unsafe {
                let _ = EnumDesktopWindows(
                    HANDLE(desk_val as *mut std::ffi::c_void),
                    Some(enum_proc),
                    LPARAM(&mut search as *mut Search<'_> as isize),
                );
            }
        }
    }
    search.found
}

#[test]
fn test_real_windows_host_passive_translation_end_to_end() {
    let (installed_dll_path, _) = verify_loaded_installed_binaries();

    let pipe_path = r"\\.\pipe\qingjian";
    let mut server_child = None;
    if std::fs::OpenOptions::new().read(true).write(true).open(pipe_path).is_err() {
        println!("[REAL-EMPIRICAL] Phase 3：启动已安装真实 Server: E:\\Qingjian\\qingjian-server.exe");
        let child = std::process::Command::new(r"E:\Qingjian\qingjian-server.exe")
            .current_dir(r"E:\Qingjian")
            .spawn()
            .expect("Phase 3 real-host 必须成功启动安装版 Server");
        server_child = Some(child);
        let mut ready = false;
        for _ in 0..60 {
            if std::fs::OpenOptions::new().read(true).write(true).open(pipe_path).is_ok() {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(ready, "Phase 3 real-host 必须连接到安装版 Server Named Pipe");
        thread::sleep(Duration::from_millis(300));
    }

    let handle = thread::spawn(move || {
        let orig_desk = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
        if !orig_desk.is_invalid() {
            *ORIGINAL_DESKTOP.lock().unwrap() = Some(orig_desk.0 as isize);
        }
        let h_desk = unsafe { OpenInputDesktop(0, windows::core::BOOL(0), 0x01FF) };
        println!("[REAL-EMPIRICAL] Phase 3 OpenInputDesktop = {:?}", h_desk);
        if !h_desk.is_invalid() {
            let ok = unsafe { SetThreadDesktop(h_desk) };
            println!("[REAL-EMPIRICAL] Phase 3 SetThreadDesktop = {:?}", ok);
        }

        let co_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        assert!(
            co_init.is_ok(),
            "Phase 3 real-host CoInitializeEx 必须成功: {co_init:?}"
        );

        let pre_mod = unsafe { GetModuleHandleW(w!("qingjian_tsf-0.1.4.dll")) };
        assert!(
            pre_mod.is_err(),
            "[REAL-EMPIRICAL] Phase 3 activation 前 TSF DLL 必须为 NOT_LOADED"
        );
        println!("[REAL-EMPIRICAL] Phase 3 pre-activation module = NOT_LOADED");

        let hkl = unsafe { LoadKeyboardLayoutW(w!("00000804"), KLF_ACTIVATE) }.expect("Phase 3 LoadKeyboardLayoutW 必须成功");
        unsafe { let _ = ActivateKeyboardLayout(hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)); }

        let thread_mgr: ITfThreadMgr = unsafe {
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)
                .expect("Phase 3 ITfThreadMgr 必须创建成功")
        };
        let _client_id = unsafe { thread_mgr.Activate().expect("Phase 3 ThreadMgr.Activate 必须成功") };

        let profiles: ITfInputProcessorProfiles = unsafe {
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .expect("Phase 3 ITfInputProcessorProfiles 必须创建成功")
        };
        unsafe {
            profiles
                .ActivateLanguageProfile(&CLSID_QINGJIAN, LANGID_ZH_CN, &GUID_PROFILE)
                .expect("Phase 3 ActivateLanguageProfile 必须成功");
        }

        let mut active_langid = 0u16;
        let mut active_profile = GUID::zeroed();
        unsafe {
            profiles
                .GetActiveLanguageProfile(&CLSID_QINGJIAN, &mut active_langid, &mut active_profile)
                .expect("Phase 3 GetActiveLanguageProfile 必须成功");
        }
        assert_eq!(active_langid, LANGID_ZH_CN);
        assert_eq!(active_profile, GUID_PROFILE);

        let host = RealHostWindow::new();
        host.pump_messages(Duration::from_millis(200));
        host.ensure_focus();
        send_raw_vk(0x12, true); // 释放可能残留的 Alt 状态，保证 WM_KEYDOWN 正常分派
        host.pump_messages(Duration::from_millis(50));
        assert_eq!(unsafe { GetForegroundWindow() }, host.hwnd);

        let h_mod = unsafe { GetModuleHandleW(w!("qingjian_tsf-0.1.4.dll")) }
            .expect("Phase 3 real TSF DLL 必须在激活后加载");
        let mut module_path_buf = [0u16; 512];
        let path_len = unsafe { GetModuleFileNameW(Some(h_mod), &mut module_path_buf) };
        let loaded_path = String::from_utf16_lossy(&module_path_buf[..path_len as usize]);
        println!("[REAL-EMPIRICAL] Phase 3 loaded TSF path = {}", loaded_path);
        assert_eq!(
            PathBuf::from(&loaded_path).canonicalize().unwrap_or(PathBuf::from(&loaded_path)),
            installed_dll_path.canonicalize().unwrap_or(installed_dll_path.clone())
        );

        // 先用一个真实字母建立中文拼音模式对照，确保当前 TSF 真正处于中文路径。
        host.clear_edit();
        host.clear_records();
        send_system_key(0x41, false); // a
        host.pump_messages(Duration::from_millis(250));
        let mut zh_records = host.get_records();
        let mut has_processkey = zh_records.iter().any(|r| r.message == 0x0100 && r.wparam == 0xE5);
        if !has_processkey {
            println!("[REAL-EMPIRICAL] Phase 3 检测到初始处于英文直通状态，单击 Shift 切换为中文拼音模式");
            send_raw_vk(VK_SHIFT.0, false);
            host.pump_messages(Duration::from_millis(30));
            send_raw_vk(VK_SHIFT.0, true);
            host.pump_messages(Duration::from_millis(200));

            host.clear_edit();
            host.clear_records();
            send_system_key(0x41, false); // a
            host.pump_messages(Duration::from_millis(250));
            zh_records = host.get_records();
            has_processkey = zh_records.iter().any(|r| r.message == 0x0100 && r.wparam == 0xE5);
        }
        assert!(
            has_processkey,
            "[REAL-EMPIRICAL] Phase 3 中文模式必须仍产生 VK_PROCESSKEY"
        );
        send_system_key(VK_ESCAPE.0, false);
        host.pump_messages(Duration::from_millis(100));
        host.clear_edit();

        // 本地词典确定性场景：kaifa -> 开发，旁边应出现被动英文 annotation。
        println!("[REAL-EMPIRICAL] Phase 3 输入真实本地释义词: kaifa");
        type_system_string(&host, "kaifa");
        host.pump_messages(Duration::from_millis(700));

        let composing_text = host.get_edit_text();
        assert_eq!(
            composing_text,
            "",
            "[REAL-EMPIRICAL] 被动翻译展示阶段不得自动写入 Host EDIT"
        );

        let mut candidate_hwnd_opt = None;
        for _ in 0..25 {
            host.pump_messages(Duration::from_millis(50));
            if let Some(hwnd) = find_window_by_class("QingjianCandidateWindow") {
                candidate_hwnd_opt = Some(hwnd);
                break;
            }
        }
        let candidate_hwnd = candidate_hwnd_opt
            .expect("[REAL-EMPIRICAL] 输入 kaifa 后必须存在 QingjianCandidateWindow");
        assert!(
            unsafe { IsWindowVisible(candidate_hwnd).as_bool() },
            "[REAL-EMPIRICAL] 输入 kaifa 后候选窗口必须真实可见"
        );
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(candidate_hwnd, &mut rect)
                .expect("[REAL-EMPIRICAL] 必须读取候选窗口真实屏幕矩形");
        }
        assert!(rect.right > rect.left && rect.bottom > rect.top);
        println!(
            "[REAL-EMPIRICAL] Phase 3 候选窗口真实可见 HWND={:?} rect=({},{})->({},{})",
            candidate_hwnd, rect.left, rect.top, rect.right, rect.bottom
        );
        host.pump_messages(Duration::from_millis(500));

        // Space：只能提交中文候选「开发」，英文 annotation 不得自动进入 Host。
        send_system_key(VK_SPACE.0, false);
        host.pump_messages(Duration::from_millis(500));
        let committed_zh = host.get_edit_text();
        println!("[REAL-EMPIRICAL] Phase 3 Space 实际 Host 文本 = {:?}", committed_zh);
        assert_eq!(
            committed_zh,
            "开发",
            "[REAL-EMPIRICAL] Space 必须只提交中文候选，不得自动提交英文翻译"
        );

        // 再来一次同一真实输入，验证 Ctrl+1 才显式上屏英文翻译。
        host.clear_edit();
        host.clear_records();
        type_system_string(&host, "kaifa");
        host.pump_messages(Duration::from_millis(700));
        let mut candidate_hwnd_2_opt = None;
        for _ in 0..25 {
            host.pump_messages(Duration::from_millis(50));
            if let Some(hwnd) = find_window_by_class("QingjianCandidateWindow") {
                candidate_hwnd_2_opt = Some(hwnd);
                break;
            }
        }
        let candidate_hwnd_2 = candidate_hwnd_2_opt
            .expect("[REAL-EMPIRICAL] 第二次 kaifa 后必须存在候选窗口");
        assert!(unsafe { IsWindowVisible(candidate_hwnd_2).as_bool() });

        // Ctrl+1 由真实系统按键组合产生，Server 应把第一条被动翻译 sense 提交到 Host。
        send_raw_vk(0x11, false); // Ctrl down
        send_raw_vk(0x31, false); // 1 down
        send_raw_vk(0x31, true);  // 1 up
        send_raw_vk(0x11, true);  // Ctrl up
        host.pump_messages(Duration::from_millis(500));

        let committed_en = host.get_edit_text();
        println!("[REAL-EMPIRICAL] Phase 3 Ctrl+1 实际 Host 文本 = {:?}", committed_en);
        assert_eq!(
            committed_en,
            "develop",
            "[REAL-EMPIRICAL] Ctrl+1 必须显式提交被动翻译第一条 sense"
        );

        unsafe {
            let _ = thread_mgr.Deactivate();
        }

        if !h_desk.is_invalid() {
            unsafe {
                let _ = CloseDesktop(h_desk);
            }
        }
    });

    handle.join().expect("Phase 3 real-host 实机线程必须顺利完成");

    if let Some(mut child) = server_child {
        let _ = child.kill();
        let _ = child.wait();
    }

    println!(
        "[REAL-EMPIRICAL] PHASE 3 Passive Translation real Windows Host 核心链路通过：kaifa -> annotation-only -> Space=开发 -> Ctrl+1=develop"
    );
}
