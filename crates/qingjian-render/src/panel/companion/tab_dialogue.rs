//! 「真实对话」标签：场景胶囊、一段 3 句对话（A 左 B 右，头像加气泡）、表达亮点。

use super::hit::CompanionHit;
use super::states;
use super::status::TabStatus;
use super::style::{BODY_BOLD, CAPTION, CARD_GAP, MARGIN_CARD, SMALL, WIDTH, card_area};
use super::turn::Turn;
use super::view::CompanionView;
use super::widgets::{Surface, highlight_spans, round_button};
use crate::color::Color;
use crate::panel::{Area, Avatar, Icon, PanelText, tokens};
use crate::text::RichSpan;

/// 场景胶囊（与 predict 的 `Scene` 同序）。
const SCENES: [&str; 6] = [
    "美式日常",
    "Gen Z 口语",
    "IELTS 口语",
    "职场沟通",
    "旅行出行",
    "校园生活",
];

/// 直接显示的场景个数，其余收进「更多场景」。
const PRIMARY: usize = 3;

const PILL_HEIGHT: f32 = 28.0;
const PILL: PanelText = PanelText::new(12.0, 16.0, tokens::TEXT);
const BUBBLE_A: Color = Color::hex(0xE8EFFC);
const BUBBLE_B: Color = Color::hex(0xDDF4E6);
const B_LABEL: Color = Color::hex(0x86C99B);
const AVATAR: f32 = 40.0;
const BUBBLE_MAX: f32 = 264.0;
const ZH: PanelText = PanelText::new(10.5, 14.0, tokens::TEXT_2);
const EN: PanelText = PanelText::new(12.0, 16.0, tokens::TEXT);
const HEADING: PanelText = PanelText::new(12.0, 17.0, tokens::TEXT).bolded();
const POINT: PanelText = PanelText::new(12.0, 17.0, tokens::TEXT);

pub(super) fn draw(s: &mut Surface<'_>, view: &CompanionView, mut y: f32) -> f32 {
    let partial_failure =
        matches!(view.dialogue_status, TabStatus::Failed(_)) && !view.dialogue.is_empty();
    if partial_failure {
        y = states::error_strip(s, y) + CARD_GAP;
    }
    let pills = scene_pills(s, view, y);
    let top = y + PILL_HEIGHT + 10.0;
    let bottom = match &view.dialogue_status {
        TabStatus::Idle => states::empty(s, top),
        TabStatus::Loading => states::loading(s, top, "真实对话", view.spin),
        TabStatus::Failed(message) if view.dialogue.is_empty() => {
            states::error_card(s, top, message)
        }
        status => {
            let mut bottom = content(s, view, top) + CARD_GAP;
            if matches!(status, TabStatus::Streaming) {
                bottom = states::tail(s, bottom, view.spin);
            }
            bottom
        }
    };
    // 菜单盖在内容上面，最后画
    if view.scene_menu_open {
        more_menu(s, view, pills, y + PILL_HEIGHT + 4.0);
    }
    bottom + 8.0
}

/// 画场景胶囊行，返回「更多场景」按钮的区域（弹出菜单靠它对齐）。
fn scene_pills(s: &mut Surface<'_>, view: &CompanionView, y: f32) -> Area {
    let mut x = MARGIN_CARD;
    for (index, label) in SCENES.iter().take(PRIMARY).enumerate() {
        let width = s.measure(label, &PILL) + 32.0;
        pill(
            s,
            Area::new(x, y, width, PILL_HEIGHT),
            label,
            view.scene == index,
        );
        s.hit(
            Area::new(x, y, width, PILL_HEIGHT),
            CompanionHit::Scene(index),
        );
        x += width + 8.0;
    }
    let more_selected = view.scene >= PRIMARY;
    let label = if more_selected {
        SCENES[view.scene.min(SCENES.len() - 1)]
    } else {
        "更多场景"
    };
    let label_w = s.measure(label, &PILL);
    let width = label_w + if more_selected { 48.0 } else { 36.0 };
    let area = Area::new(WIDTH - MARGIN_CARD - width, y, width, PILL_HEIGHT);
    if more_selected {
        pill(s, area, label, true);
        s.icon(
            Icon::ChevronDown,
            area.right() - 22.0,
            y + 8.0,
            12.0,
            tokens::WINDOW_BG,
        );
    } else {
        s.text(area.x + 8.0, y + 6.0, label, &PILL);
        s.icon(
            Icon::ChevronDown,
            area.right() - 22.0,
            y + 8.0,
            12.0,
            tokens::PRIMARY,
        );
    }
    s.hit(area, CompanionHit::MoreScenes);
    area
}

/// 一个场景胶囊：选中实心主色，其余白底描边。
fn pill(s: &mut Surface<'_>, area: Area, label: &str, selected: bool) {
    let style = if selected {
        PILL.colored(tokens::WINDOW_BG).bolded()
    } else {
        PILL
    };
    if selected {
        s.fill(area, tokens::RADIUS_BUTTON, tokens::PRIMARY);
    } else {
        s.fill_bordered(
            area,
            tokens::RADIUS_BUTTON,
            tokens::WINDOW_BG,
            tokens::BORDER,
        );
    }
    let width = s.measure(label, &style);
    s.text(
        area.x + (area.width - width) / 2.0,
        area.y + 6.0,
        label,
        &style,
    );
}

/// 「更多场景」弹出菜单：后三个场景。点菜单外面关闭。
fn more_menu(s: &mut Surface<'_>, view: &CompanionView, anchor: Area, y: f32) {
    s.hit(
        Area::new(0.0, 0.0, WIDTH, 2000.0),
        CompanionHit::DismissMenu,
    );
    let width = 132.0;
    let area = Area::new(
        anchor.right() - width,
        y,
        width,
        8.0 + 32.0 * (SCENES.len() - PRIMARY) as f32,
    );
    s.fill_bordered(
        area,
        tokens::RADIUS_BUTTON,
        tokens::WINDOW_BG,
        tokens::BORDER,
    );
    for (offset, label) in SCENES.iter().enumerate().skip(PRIMARY) {
        let row = Area::new(
            area.x + 4.0,
            area.y + 4.0 + 32.0 * (offset - PRIMARY) as f32,
            width - 8.0,
            32.0,
        );
        let selected = view.scene == offset;
        if selected {
            s.fill(row, 6.0, tokens::SELECTED_BG);
        }
        let style = if selected {
            PILL.colored(tokens::PRIMARY).bolded()
        } else {
            PILL
        };
        s.text(row.x + 12.0, row.y + 8.0, label, &style);
        s.hit(row, CompanionHit::Scene(offset));
    }
}

/// 场景卡 + 表达亮点卡，返回下沿。
fn content(s: &mut Surface<'_>, view: &CompanionView, y: f32) -> f32 {
    let dialogue = &view.dialogue;
    // 先把每句的气泡尺寸量出来，场景卡高度才定得下来
    let sizes: Vec<(f32, f32)> = dialogue
        .turns
        .iter()
        .map(|turn| bubble_size(s, turn))
        .collect();
    let rows: f32 = sizes.iter().map(|(_, h)| h.max(AVATAR) + 12.0).sum();
    let card = card_area(y, 44.0 + rows + 2.0);
    s.fill(card, tokens::RADIUS_CARD, tokens::CARD_BLUE);
    let chip = Area::new(card.x + 10.0, y + 11.0, 22.0, 22.0);
    s.fill(chip, 6.0, tokens::ICON_CHIP_BLUE);
    s.icon(
        Icon::MessageCircle,
        chip.x + 4.0,
        chip.y + 4.0,
        14.0,
        tokens::PRIMARY,
    );
    s.text(card.x + 42.0, y + 13.0, &scene_title(view), &HEADING);
    let mut cy = y + 44.0;
    for (turn, (width, height)) in dialogue.turns.iter().zip(&sizes) {
        cy += draw_turn(s, card, cy, turn, *width, *height) + 12.0;
    }
    let mut bottom = card.bottom();
    if !dialogue.points.is_empty() {
        bottom = points_card(s, bottom + CARD_GAP, &dialogue.points);
    }
    bottom
}

/// 「场景 N：标题」；标题没到时只显示场景名。
fn scene_title(view: &CompanionView) -> String {
    let name = SCENES.get(view.scene).copied().unwrap_or("");
    let n = view.scene + 1;
    if view.dialogue.title.is_empty() {
        format!("场景 {n}：{name}")
    } else {
        format!("场景 {n}：{}", view.dialogue.title)
    }
}

/// 气泡的（宽，高）：宽随内容，最宽 [`BUBBLE_MAX`]。
fn bubble_size(s: &mut Surface<'_>, turn: &Turn) -> (f32, f32) {
    let inner = BUBBLE_MAX - 24.0;
    let en = s.measure_rich(&highlight_spans(&turn.en, &turn.hl), &EN, inner);
    let zh = s.measure_rich(&[RichSpan::plain(&turn.zh)], &ZH, inner);
    // B 的气泡右上角要放角标，多留一点宽度
    let label = if turn.speaker == Avatar::Girl {
        14.0
    } else {
        0.0
    };
    let width = (en.width.max(zh.width) + 26.0 + label).clamp(80.0, BUBBLE_MAX);
    (width, 7.0 + en.height + 2.0 + zh.height + 7.0)
}

/// 画一句对话，返回这一行的高度。
fn draw_turn(s: &mut Surface<'_>, card: Area, y: f32, turn: &Turn, width: f32, height: f32) -> f32 {
    let inner = BUBBLE_MAX - 24.0;
    let left_side = turn.speaker == Avatar::Boy;
    let avatar_x = if left_side {
        card.x + 8.0
    } else {
        card.right() - 8.0 - AVATAR
    };
    s.avatar(turn.speaker, avatar_x, y, AVATAR);
    let bubble_x = if left_side {
        card.x + 8.0 + AVATAR + 26.0
    } else {
        avatar_x - 6.0 - width
    };
    let bubble = Area::new(bubble_x, y, width, height);
    let (fill, speak_x) = if left_side {
        (BUBBLE_A, card.right() - 22.0)
    } else {
        (BUBBLE_B, bubble.x - 20.0)
    };
    s.fill(bubble, tokens::RADIUS_CARD, fill);
    let spans = highlight_spans(&turn.en, &turn.hl);
    let en_h = s.measure_rich(&spans, &EN, inner).height;
    s.rich(bubble.x + 12.0, y + 7.0, inner, &spans, &EN);
    s.paragraph(bubble.x + 12.0, y + 9.0 + en_h, inner, &turn.zh, &ZH);
    if left_side {
        s.text(
            card.x + 8.0 + AVATAR + 10.0,
            y + AVATAR / 2.0 - 8.0,
            "A",
            &SMALL,
        );
    } else {
        s.text(
            bubble.right() - 16.0,
            y + 7.0,
            "B",
            &CAPTION.colored(B_LABEL),
        );
    }
    round_button(
        s,
        speak_x,
        y + height.max(AVATAR) / 2.0,
        22.0,
        Icon::Volume2,
        CompanionHit::Speak(turn.en.clone()),
    );
    height.max(AVATAR)
}

/// 表达亮点卡，返回下沿。
fn points_card(s: &mut Surface<'_>, y: f32, points: &[String]) -> f32 {
    let probe = card_area(y, 0.0);
    let text_w = probe.width - 40.0;
    let heights: Vec<f32> = points
        .iter()
        .map(|p| s.measure_rich(&[RichSpan::plain(p)], &POINT, text_w).height)
        .collect();
    let card = card_area(y, 38.0 + heights.iter().map(|h| h + 5.0).sum::<f32>() + 4.0);
    s.fill_bordered(
        card,
        tokens::RADIUS_CARD,
        Color::hex(0xF5F9FF),
        tokens::ICON_CHIP_BLUE,
    );
    s.icon(
        Icon::Lightbulb,
        card.x + 12.0,
        y + 11.0,
        16.0,
        tokens::PRIMARY,
    );
    s.text(
        card.x + 34.0,
        y + 11.0,
        "表达亮点",
        &BODY_BOLD.colored(tokens::PRIMARY),
    );
    let mut cy = y + 36.0;
    for (point, height) in points.iter().zip(heights) {
        s.circle(card.x + 20.0, cy + 8.5, 2.0, tokens::TEXT);
        s.paragraph(card.x + 32.0, cy, text_w, point, &POINT);
        cy += height + 5.0;
    }
    card.bottom()
}
