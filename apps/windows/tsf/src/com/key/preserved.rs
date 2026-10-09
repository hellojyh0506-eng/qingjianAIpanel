//! 「翻译选中文字」与「Ctrl + Alt + Space 切换中英」两个快捷键登记成 TSF **保留键**（preserved key）。
//! 带 Alt 的组合是系统键，不经击键 sink（真机：Ctrl+Alt+T 在 `OnTestKeyDown` 里从没出现过）；
//! 保留键由 TSF 在应用之前匹配、回调 `OnPreservedKey`，UWP 里也一样。翻译组合来自
//! `[shortcut] translate_selection`，激活时读一次配置（AppContainer 读不到用户目录时用缺省 Ctrl+Alt+T）；
//! 切换键来自 `[shortcut] switch_mode`，那个值由 Server 经协议下发（DLL 不读配置文件），变了就地重登记。

use windows::Win32::UI::Input::KeyboardAndMouse::VK_SPACE;
use windows::Win32::UI::TextServices::{
    ITfKeystrokeMgr, TF_MOD_ALT, TF_MOD_CONTROL, TF_MOD_SHIFT, TF_PRESERVEDKEY,
};
use windows::core::{GUID, Result};

use qingjian_platform::protocol::{KeyEvent, KeyModifiers};
use qingjian_platform::{Config, KeyCombo};

use crate::com::log::log;

/// 本保留键的标识，`OnPreservedKey` 按它认。
pub(crate) const GUID_TRANSLATE: GUID = GUID::from_u128(0x5c0a7b12_3d4e_4f60_8a91_2b3c4d5e6f70);

/// Ctrl + Alt + Space 中英切换键的保留键标识。
pub(crate) const GUID_SWITCH_MODE: GUID = GUID::from_u128(0x2f6b8c51_9a34_4e7d_b2c8_5d1e0f3a7b64);

/// Ctrl + Alt + E 打开/关闭 English Assistant（学习助手）保留键标识。
pub(crate) const GUID_ENGLISH_ASSISTANT: GUID = GUID::from_u128(0x7e1a9b23_4c5d_6e7f_8a9b_0c1d2e3f4a5b);

/// Ctrl + Alt + D 打开/关闭 AI 对话助手保留键标识。
pub(crate) const GUID_ENGLISH_DRILL: GUID = GUID::from_u128(0x8a2b0c34_5d6e_7f80_9b0c_1d2e3f4a5b6c);

/// Ctrl + Alt + L 切换英语学习模式保留键标识。
pub(crate) const GUID_LEARNING_MODE_TOGGLE: GUID = GUID::from_u128(0x9b3c4d5e_6f70_8a91_2b3c_4d5e6f708a9b);

/// Ctrl + Alt + R 触发 FSRS 今日复习队列保留键标识。
pub(crate) const GUID_LEARNING_REVIEW: GUID = GUID::from_u128(0xac4d5e6f_708a_912b_3c4d_5e6f708a9bac);

/// 旧版 GUID_LEARNING_MODE 别名兼容
#[allow(dead_code)]
pub(crate) const GUID_LEARNING_MODE: GUID = GUID_ENGLISH_ASSISTANT;

/// msctf.h 的 `TF_MOD_LWIN`（windows crate 没导出）。
const TF_MOD_LWIN: u32 = 0x08;

/// 读 `%APPDATA%\Qingjian\config.toml` 里的组合；读不到 / 解析失败用缺省。
pub(crate) fn load_combo() -> KeyCombo {
    let Some(path) = qingjian_platform::dirs::config_path() else {
        return KeyCombo::TRANSLATE_DEFAULT;
    };
    match Config::load(&path) {
        Ok(config) => config.shortcut.translate_selection,
        Err(error) => {
            log(&format!("读配置取翻译快捷键失败，用缺省: {error}"));
            KeyCombo::TRANSLATE_DEFAULT
        }
    }
}

/// Ctrl + Alt + Space 的 `TF_PRESERVEDKEY`。不用 Ctrl + Space：中文 Windows 把它绑成系统的
/// 「输入法/非输入法切换」，系统先截走，保留键收不到。
fn switch_mode_key() -> TF_PRESERVEDKEY {
    TF_PRESERVEDKEY {
        uVKey: VK_SPACE.0 as u32,
        uModifiers: TF_MOD_CONTROL | TF_MOD_ALT,
    }
}

/// 登记 Ctrl + Alt + Space 为中英切换保留键（`switch_mode` 勾了它时）。
pub(crate) fn register_switch_mode(keystroke: &ITfKeystrokeMgr, tid: u32) -> Result<()> {
    let description: Vec<u16> = "切换中英文（青简）".encode_utf16().collect();
    unsafe { keystroke.PreserveKey(tid, &GUID_SWITCH_MODE, &switch_mode_key(), &description) }
}

pub(crate) fn unregister_switch_mode(keystroke: &ITfKeystrokeMgr) {
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_SWITCH_MODE, &switch_mode_key()) };
}

fn preserved_key(combo: KeyCombo) -> TF_PRESERVEDKEY {
    let m = combo.modifiers;
    let mut modifiers = 0;
    if m.control {
        modifiers |= TF_MOD_CONTROL;
    }
    if m.option {
        modifiers |= TF_MOD_ALT;
    }
    if m.shift {
        modifiers |= TF_MOD_SHIFT;
    }
    if m.command {
        modifiers |= TF_MOD_LWIN;
    }
    TF_PRESERVEDKEY {
        uVKey: combo.key.to_ascii_uppercase() as u32,
        uModifiers: modifiers,
    }
}

pub(crate) fn register(keystroke: &ITfKeystrokeMgr, tid: u32, combo: KeyCombo) -> Result<()> {
    let key = preserved_key(combo);
    let description: Vec<u16> = "翻译选中文字".encode_utf16().collect();
    unsafe { keystroke.PreserveKey(tid, &GUID_TRANSLATE, &key, &description) }
}

pub(crate) fn unregister(keystroke: &ITfKeystrokeMgr, combo: KeyCombo) {
    let key = preserved_key(combo);
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_TRANSLATE, &key) };
}

/// 保留键命中时喂给 Server 的按键：Router 按字符 + 物理修饰键与配置比对。
pub(crate) fn key_event(combo: KeyCombo, english_mode: bool) -> KeyEvent {
    let m = combo.modifiers;
    KeyEvent::new(
        combo.key.to_ascii_uppercase() as u32,
        Some(combo.key),
        KeyModifiers {
            ctrl: m.control,
            shift: m.shift,
            alt: m.option,
            win: m.command,
            caps: false,
            english_mode,
        },
    )
}

/// Ctrl + Alt + E 的 `TF_PRESERVEDKEY`（学习助手）。
fn assistant_key() -> TF_PRESERVEDKEY {
    TF_PRESERVEDKEY {
        uVKey: b'E' as u32,
        uModifiers: TF_MOD_CONTROL | TF_MOD_ALT,
    }
}

/// Ctrl + Alt + D 的 `TF_PRESERVEDKEY`（AI 对话助手）。
fn drill_key() -> TF_PRESERVEDKEY {
    TF_PRESERVEDKEY {
        uVKey: b'D' as u32,
        uModifiers: TF_MOD_CONTROL | TF_MOD_ALT,
    }
}

/// Ctrl + Alt + L 的 `TF_PRESERVEDKEY`（切换学习模式）。
fn learning_mode_key() -> TF_PRESERVEDKEY {
    TF_PRESERVEDKEY {
        uVKey: b'L' as u32,
        uModifiers: TF_MOD_CONTROL | TF_MOD_ALT,
    }
}

/// Ctrl + Alt + R 的 `TF_PRESERVEDKEY`（复习队列）。
fn review_key() -> TF_PRESERVEDKEY {
    TF_PRESERVEDKEY {
        uVKey: b'R' as u32,
        uModifiers: TF_MOD_CONTROL | TF_MOD_ALT,
    }
}

/// 登记英语学习系统保留键（Ctrl+Alt+E/D/L/R）。
pub(crate) fn register_learning_mode(keystroke: &ITfKeystrokeMgr, tid: u32) -> Result<()> {
    let desc_e: Vec<u16> = "打开/关闭英语学习助手（青简）".encode_utf16().collect();
    let _ = unsafe { keystroke.PreserveKey(tid, &GUID_ENGLISH_ASSISTANT, &assistant_key(), &desc_e) };
    let desc_d: Vec<u16> = "打开/关闭AI对话助手（青简）".encode_utf16().collect();
    let _ = unsafe { keystroke.PreserveKey(tid, &GUID_ENGLISH_DRILL, &drill_key(), &desc_d) };
    let desc_l: Vec<u16> = "切换英语学习模式（青简）".encode_utf16().collect();
    let _ = unsafe { keystroke.PreserveKey(tid, &GUID_LEARNING_MODE_TOGGLE, &learning_mode_key(), &desc_l) };
    let desc_r: Vec<u16> = "进入今日复习队列（青简）".encode_utf16().collect();
    let _ = unsafe { keystroke.PreserveKey(tid, &GUID_LEARNING_REVIEW, &review_key(), &desc_r) };
    Ok(())
}

pub(crate) fn unregister_learning_mode(keystroke: &ITfKeystrokeMgr) {
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_ENGLISH_ASSISTANT, &assistant_key()) };
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_ENGLISH_DRILL, &drill_key()) };
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_LEARNING_MODE_TOGGLE, &learning_mode_key()) };
    let _ = unsafe { keystroke.UnpreserveKey(&GUID_LEARNING_REVIEW, &review_key()) };
}

pub(crate) fn assistant_event(english_mode: bool) -> KeyEvent {
    KeyEvent::new(
        b'E' as u32,
        Some('e'),
        KeyModifiers {
            ctrl: true,
            shift: false,
            alt: true,
            win: false,
            caps: false,
            english_mode,
        },
    )
}

pub(crate) fn drill_event(english_mode: bool) -> KeyEvent {
    KeyEvent::new(
        b'D' as u32,
        Some('d'),
        KeyModifiers {
            ctrl: true,
            shift: false,
            alt: true,
            win: false,
            caps: false,
            english_mode,
        },
    )
}

pub(crate) fn mode_toggle_event(english_mode: bool) -> KeyEvent {
    KeyEvent::new(
        b'L' as u32,
        Some('l'),
        KeyModifiers {
            ctrl: true,
            shift: false,
            alt: true,
            win: false,
            caps: false,
            english_mode,
        },
    )
}

pub(crate) fn review_event(english_mode: bool) -> KeyEvent {
    KeyEvent::new(
        b'R' as u32,
        Some('r'),
        KeyModifiers {
            ctrl: true,
            shift: false,
            alt: true,
            win: false,
            caps: false,
            english_mode,
        },
    )
}

/// 英语学习模式保留键命中时喂给 Server 的按键。
#[allow(dead_code)]
pub(crate) fn learning_mode_event(english_mode: bool) -> KeyEvent {
    assistant_event(english_mode)
}
