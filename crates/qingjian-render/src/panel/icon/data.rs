//! Lucide 图标（ISC 许可，见同目录 LICENSE-lucide）：24 × 24 视口、描边风格的 path 数据。
//!
//! 本文件由 `tools/lucide-icons/gen.py` 生成，不要手改；要加图标改脚本里的清单后重新生成。

/// 面板用到的图标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    ArrowUp,
    BookPlus,
    Check,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    CircleAlert,
    CircleCheck,
    Clock,
    Copy,
    Ellipsis,
    FileText,
    Image,
    Lightbulb,
    LoaderCircle,
    MessageCircle,
    MessageSquare,
    Mic,
    Minus,
    Paperclip,
    Pencil,
    Pin,
    Plus,
    RefreshCw,
    ScanText,
    Send,
    Settings,
    Sparkles,
    Star,
    Target,
    Trash,
    Volume2,
    X,
}

impl Icon {
    /// 全部图标，测试里逐个验证能解析。
    pub const ALL: &'static [Icon] = &[
        Icon::ArrowUp,
        Icon::BookPlus,
        Icon::Check,
        Icon::ChevronDown,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::CircleAlert,
        Icon::CircleCheck,
        Icon::Clock,
        Icon::Copy,
        Icon::Ellipsis,
        Icon::FileText,
        Icon::Image,
        Icon::Lightbulb,
        Icon::LoaderCircle,
        Icon::MessageCircle,
        Icon::MessageSquare,
        Icon::Mic,
        Icon::Minus,
        Icon::Paperclip,
        Icon::Pencil,
        Icon::Pin,
        Icon::Plus,
        Icon::RefreshCw,
        Icon::ScanText,
        Icon::Send,
        Icon::Settings,
        Icon::Sparkles,
        Icon::Star,
        Icon::Target,
        Icon::Trash,
        Icon::Volume2,
        Icon::X,
    ];

    /// Lucide 里的图标名。
    pub fn name(self) -> &'static str {
        match self {
            Icon::ArrowUp => "arrow-up",
            Icon::BookPlus => "book-plus",
            Icon::Check => "check",
            Icon::ChevronDown => "chevron-down",
            Icon::ChevronRight => "chevron-right",
            Icon::ChevronUp => "chevron-up",
            Icon::CircleAlert => "circle-alert",
            Icon::CircleCheck => "circle-check",
            Icon::Clock => "clock",
            Icon::Copy => "copy",
            Icon::Ellipsis => "ellipsis",
            Icon::FileText => "file-text",
            Icon::Image => "image",
            Icon::Lightbulb => "lightbulb",
            Icon::LoaderCircle => "loader-circle",
            Icon::MessageCircle => "message-circle",
            Icon::MessageSquare => "message-square",
            Icon::Mic => "mic",
            Icon::Minus => "minus",
            Icon::Paperclip => "paperclip",
            Icon::Pencil => "pencil",
            Icon::Pin => "pin",
            Icon::Plus => "plus",
            Icon::RefreshCw => "refresh-cw",
            Icon::ScanText => "scan-text",
            Icon::Send => "send",
            Icon::Settings => "settings",
            Icon::Sparkles => "sparkles",
            Icon::Star => "star",
            Icon::Target => "target",
            Icon::Trash => "trash",
            Icon::Volume2 => "volume-2",
            Icon::X => "x",
        }
    }

    /// 组成这个图标的 path `d` 字符串，24 × 24 坐标系。
    pub(super) fn paths(self) -> &'static [&'static str] {
        match self {
            Icon::ArrowUp => &["m5 12 7-7 7 7", "M12 19V5"],
            Icon::BookPlus => &[
                "M12 7v6",
                "M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H19a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H6.5a1 1 0 0 1 0-5H20",
                "M9 10h6",
            ],
            Icon::Check => &["M20 6 9 17l-5-5"],
            Icon::ChevronDown => &["m6 9 6 6 6-6"],
            Icon::ChevronRight => &["m9 18 6-6-6-6"],
            Icon::ChevronUp => &["m18 15-6-6-6 6"],
            Icon::CircleAlert => &[
                "M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z",
                "M12 8L12 12",
                "M12 16L12.01 16",
            ],
            Icon::CircleCheck => &[
                "M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z",
                "m16 9-5.5 5.5L8 12",
            ],
            Icon::Clock => &["M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z", "M12 6v6l4 2"],
            Icon::Copy => &[
                "M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-10a2 2 0 0 1 -2 -2v-10a2 2 0 0 1 2 -2z",
                "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
            ],
            Icon::Ellipsis => &[
                "M11 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0z",
                "M18 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0z",
                "M4 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0z",
            ],
            Icon::FileText => &[
                "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
                "M14 2v5a1 1 0 0 0 1 1h5",
                "M10 9H8",
                "M16 13H8",
                "M16 17H8",
            ],
            Icon::Image => &[
                "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-14a2 2 0 0 1 2 -2z",
                "M7 9a2 2 0 1 0 4 0a2 2 0 1 0 -4 0z",
                "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21",
            ],
            Icon::Lightbulb => &[
                "M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5",
                "M9 18h6",
                "M10 22h4",
            ],
            Icon::LoaderCircle => &["M21 12a9 9 0 1 1-6.219-8.56"],
            Icon::MessageCircle => &[
                "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719",
            ],
            Icon::MessageSquare => &[
                "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
            ],
            Icon::Mic => &[
                "M12 19v3",
                "M19 10v2a7 7 0 0 1-14 0v-2",
                "M12 2h0a3 3 0 0 1 3 3v7a3 3 0 0 1 -3 3h0a3 3 0 0 1 -3 -3v-7a3 3 0 0 1 3 -3z",
            ],
            Icon::Minus => &["M5 12h14"],
            Icon::Paperclip => &[
                "m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551",
            ],
            Icon::Pencil => &[
                "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z",
                "m15 5 4 4",
            ],
            Icon::Pin => &[
                "M12 17v5",
                "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
            ],
            Icon::Plus => &["M5 12h14", "M12 5v14"],
            Icon::RefreshCw => &[
                "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8",
                "M21 3v5h-5",
                "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16",
                "M8 16H3v5",
            ],
            Icon::ScanText => &[
                "M3 7V5a2 2 0 0 1 2-2h2",
                "M17 3h2a2 2 0 0 1 2 2v2",
                "M21 17v2a2 2 0 0 1-2 2h-2",
                "M7 21H5a2 2 0 0 1-2-2v-2",
                "M7 8h8",
                "M7 12h10",
                "M7 16h6",
            ],
            Icon::Send => &[
                "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
                "m21.854 2.147-10.94 10.939",
            ],
            Icon::Settings => &[
                "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915",
                "M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0z",
            ],
            Icon::Sparkles => &[
                "M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z",
                "M20 2v4",
                "M22 4h-4",
                "M2 20a2 2 0 1 0 4 0a2 2 0 1 0 -4 0z",
            ],
            Icon::Star => &[
                "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
            ],
            Icon::Target => &[
                "M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z",
                "M6 12a6 6 0 1 0 12 0a6 6 0 1 0 -12 0z",
                "M10 12a2 2 0 1 0 4 0a2 2 0 1 0 -4 0z",
            ],
            Icon::Trash => &[
                "M10 11v6",
                "M14 11v6",
                "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6",
                "M3 6h18",
                "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
            ],
            Icon::Volume2 => &[
                "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z",
                "M16 9a5 5 0 0 1 0 6",
                "M19.364 18.364a9 9 0 0 0 0-12.728",
            ],
            Icon::X => &["M18 6 6 18", "m6 6 12 12"],
        }
    }
}
