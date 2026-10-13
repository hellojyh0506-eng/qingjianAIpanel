//! Linux 的字体文件清单：发行版装 Noto 的常见位置。以后读 fontconfig 配置再补。

use std::path::PathBuf;

pub(super) fn ui_fonts() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

pub(super) fn script_fonts(_locale: &str) -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansSC-Regular.otf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

pub(super) fn emoji_fonts() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

/// 加粗字重的字体文件（面板的高亮短语、标题用）；没有就退回常规字重，不影响候选窗。
pub(super) fn bold_fonts(_locale: &str) -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansSC-Bold.otf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}
