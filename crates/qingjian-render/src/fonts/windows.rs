//! Windows 的字体文件清单：Segoe UI、微软雅黑、Yu Gothic、Segoe UI Emoji（COLRv0）。

use std::path::PathBuf;

fn fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

pub(super) fn ui_fonts() -> Vec<PathBuf> {
    vec![
        fonts_dir().join("segoeui.ttf"),
        fonts_dir().join("arial.ttf"),
    ]
}

pub(super) fn script_fonts(locale: &str) -> Vec<PathBuf> {
    let dir = fonts_dir();
    let mut fonts = Vec::new();
    if locale.starts_with("zh-TW") || locale.starts_with("zh-HK") {
        fonts.push(dir.join("msjh.ttc"));
    } else if !locale.starts_with("ja") {
        fonts.push(dir.join("msyh.ttc"));
    }
    fonts.push(dir.join("YuGothR.ttc"));
    fonts.push(dir.join("yugothm.ttc"));
    fonts
}

pub(super) fn emoji_fonts() -> Vec<PathBuf> {
    vec![fonts_dir().join("seguiemj.ttf")]
}

/// 加粗字重的字体文件（面板的高亮短语、标题用）；没有就退回常规字重，不影响候选窗。
pub(super) fn bold_fonts(locale: &str) -> Vec<PathBuf> {
    let dir = fonts_dir();
    let mut fonts = vec![dir.join("segoeuib.ttf")];
    if locale.starts_with("zh-TW") || locale.starts_with("zh-HK") {
        fonts.push(dir.join("msjhbd.ttc"));
    } else if !locale.starts_with("ja") {
        fonts.push(dir.join("msyhbd.ttc"));
    }
    fonts
}
