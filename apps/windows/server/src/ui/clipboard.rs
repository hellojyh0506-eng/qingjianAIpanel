//! 系统剪贴板里的 Unicode 文本：复制与粘贴。

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

/// 剪贴板格式 `CF_UNICODETEXT`。
const CF_UNICODETEXT: u32 = 13;

/// 成功放进去返回 `true`；剪贴板被别的程序占着时返回 `false`。
pub(crate) fn copy_text(text: &str) -> bool {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut copied = false;
        if let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let ptr = GlobalLock(handle) as *mut u16;
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
                let _ = GlobalUnlock(handle);
                copied = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(handle.0))).is_ok();
            }
        }
        let _ = CloseClipboard();
        copied
    }
}

/// 读出剪贴板里的文本；没有文本或剪贴板被占着返回 `None`。
pub(crate) fn paste_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() || OpenClipboard(None).is_err() {
            return None;
        }
        let mut text = None;
        if let Ok(handle) = GetClipboardData(CF_UNICODETEXT) {
            let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if !ptr.is_null() {
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                text = Some(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)));
                let _ = GlobalUnlock(hglobal);
            }
        }
        let _ = CloseClipboard();
        text
    }
}
