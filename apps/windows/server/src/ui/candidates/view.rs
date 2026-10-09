//! 候选窗口的绘制与测量。布局镜像 macOS 端：竖排 `序号  候选词   读音·词性 译文`，
//! 横排 `序号 候选词` 左右排、高亮项的译文另起一行；顶部一行拼音，右侧整句补全。

use qingjian_platform::LayoutMode;
use qingjian_platform::protocol::PreeditKind;
use qingjian_render::{Row, Tone};
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, CreateSolidBrush, DeleteObject, FillRect, FillRgn, GetTextExtentPoint32W,
    HDC, HFONT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};

use super::RenderData;
use super::theme::{Theme, rgb};

/// 云端候选词前的小云朵（macOS 用 SF Symbol `cloud`）。
const CLOUD_GLYPH: &str = "☁";

/// 竖排的列宽与统一行高。
struct Columns {
    index_width: i32,
    text_width: i32,
    annotation_width: i32,
    row_height: i32,
}

/// 内容需要的大小（含内边距）。
pub(super) fn preferred_size(hdc: HDC, data: &RenderData) -> SIZE {
    let theme = &data.theme;
    let (top_width, top_height) = top_line_size(hdc, data);
    let (notice_width, notice_height) = notice_line_size(hdc, data);
    let (body_width, body_height) = match data.layout {
        LayoutMode::Vertical => vertical_size(hdc, data),
        LayoutMode::Horizontal => horizontal_size(hdc, data),
    };
    let (learning_width, learning_height) = learning_card_size(hdc, data);
    SIZE {
        cx: top_width.max(body_width).max(notice_width).max(learning_width) + theme.padding * 2,
        cy: top_height + notice_height + body_height + learning_height + theme.padding * 2,
    }
}

fn vertical_size(hdc: HDC, data: &RenderData) -> (i32, i32) {
    let theme = &data.theme;
    let columns = columns(hdc, theme, &data.rows);
    let mut body_width = columns.index_width + theme.column_gap + columns.text_width;
    if columns.annotation_width > 0 {
        body_width += theme.column_gap + columns.annotation_width;
    }
    let mut body_height = columns.row_height * data.rows.len() as i32;
    if let Some(footer) = &data.footer {
        let size = measure(hdc, theme.index_font, footer);
        body_width = body_width.max(size.cx);
        body_height += size.cy + theme.row_padding;
    }
    (body_width, body_height)
}

fn horizontal_size(hdc: HDC, data: &RenderData) -> (i32, i32) {
    let theme = &data.theme;
    if data.rows.is_empty() {
        return (0, 0);
    }
    let index_gap = theme.column_gap / 2;
    let highlight_inset = theme.padding / 2;
    let mut row_height = 0;
    let mut width = 0;
    for row in &data.rows {
        let index = measure(hdc, theme.index_font, &row.index);
        let text = measure(hdc, theme.text_font, &row.text);
        width += index.cx
            + index_gap
            + cloud_prefix_width(hdc, theme, row)
            + text.cx
            + code_width(hdc, theme, row);
        row_height = row_height.max(text.cy + theme.row_padding * 2);
    }
    width += theme.column_gap * (data.rows.len().saturating_sub(1)) as i32 + highlight_inset * 2;
    if let Some(footer) = &data.footer {
        width += theme.column_gap + measure(hdc, theme.index_font, footer).cx;
    }
    let mut height = row_height;
    if data.learning_card.is_none() {
        if let Some((annotation_width, annotation_height)) = highlighted_annotation_size(hdc, data) {
            width = width.max(annotation_width);
            height += annotation_height;
        }
    }
    (width, height)
}

/// 横排时高亮候选的译文行尺寸；没有译文 / 没有高亮为 `None`。
fn highlighted_annotation_size(hdc: HDC, data: &RenderData) -> Option<(i32, i32)> {
    let theme = &data.theme;
    let row = data.rows.get(data.highlight)?;
    if row.annotation.is_empty() {
        return None;
    }
    let width: i32 = row
        .annotation
        .iter()
        .map(|(s, _)| measure(hdc, theme.annotation_font, s).cx)
        .sum();
    let height = line_height(hdc, theme.annotation_font) + theme.row_padding;
    Some((width, height))
}

/// 画整帧；背景与阴影已由合成器铺好，`client` 是内容区。
pub(super) fn paint(hdc: HDC, data: &RenderData, client: RECT) {
    let theme = &data.theme;
    unsafe { SetBkMode(hdc, TRANSPARENT) };

    let mut y = theme.padding;
    y += draw_top_line(hdc, data, y);
    y += draw_notice(hdc, data, y);
    let rows_height = match data.layout {
        LayoutMode::Vertical => {
            draw_rows(hdc, data, y, client.right - client.left);
            vertical_size(hdc, data).1
        }
        LayoutMode::Horizontal => {
            draw_horizontal(hdc, data, y, client.right - client.left);
            horizontal_size(hdc, data).1
        }
    };
    y += rows_height;
    draw_learning_card(hdc, data, y, client.right - client.left);
}

/// 顶部拼音行：各段按样式画、自己画光标、右侧整句补全。返回占用高度。
/// 「只在行内」时没有拼音行，但整句补全仍要画（占用同一条线）。
fn draw_top_line(hdc: HDC, data: &RenderData, y: i32) -> i32 {
    if data.preedit.is_empty() && data.sentence.is_none() {
        return 0;
    }
    let theme = &data.theme;
    let height = line_height(hdc, theme.annotation_font);
    let top = y + theme.row_padding;
    let mut x = theme.padding;
    for (text, kind) in &data.preedit {
        let (color, strike, underline) = match kind {
            PreeditKind::Typed => (theme.gloss_color, false, false),
            PreeditKind::Rest => (theme.pos_color, false, false),
            PreeditKind::Corrected => (theme.pos_color, true, false),
            // 辅码码段：与剩余拼音同一个淡色，再压一道下划线把它们区分开
            PreeditKind::AuxCode => (theme.pos_color, false, true),
        };
        let width = draw_text(hdc, theme.annotation_font, color, x, top, text);
        let weight = scale_line(theme);
        if strike {
            let line = RECT {
                left: x,
                top: top + height / 2,
                right: x + width,
                bottom: top + height / 2 + weight,
            };
            fill_rect(hdc, line, theme.pos_color);
        }
        if underline {
            let line = RECT {
                left: x,
                top: top + height,
                right: x + width,
                bottom: top + height + weight,
            };
            fill_rect(hdc, line, theme.pos_color);
        }
        x += width;
    }
    if !data.preedit.is_empty() {
        let before = concat_before_cursor(&data.preedit, data.cursor);
        let caret_x = theme.padding + measure(hdc, theme.annotation_font, &before).cx;
        let caret = RECT {
            left: caret_x,
            top,
            right: caret_x + scale_line(theme),
            bottom: top + height,
        };
        fill_rect(hdc, caret, theme.text_color);
    }
    if let Some(sentence) = &data.sentence {
        let sentence_x = x + theme.column_gap;
        let cloud = cloud_glyph_width(hdc, theme);
        draw_text(
            hdc,
            theme.annotation_font,
            theme.cloud_color,
            sentence_x,
            top,
            CLOUD_GLYPH,
        );
        draw_text(
            hdc,
            theme.annotation_font,
            theme.gloss_color,
            sentence_x + cloud,
            top,
            sentence,
        );
    }
    height + theme.row_padding * 2
}

/// 屏幕提示行：拼音行下方、候选行上方，淡色小字。返回占用高度。
fn draw_notice(hdc: HDC, data: &RenderData, y: i32) -> i32 {
    let Some(notice) = &data.notice else {
        return 0;
    };
    let theme = &data.theme;
    draw_text(
        hdc,
        theme.annotation_font,
        theme.pos_color,
        theme.padding,
        y,
        notice,
    );
    line_height(hdc, theme.annotation_font) + theme.row_padding
}

fn notice_line_size(hdc: HDC, data: &RenderData) -> (i32, i32) {
    let Some(notice) = &data.notice else {
        return (0, 0);
    };
    let theme = &data.theme;
    let width = measure(hdc, theme.annotation_font, notice).cx;
    (
        width,
        line_height(hdc, theme.annotation_font) + theme.row_padding,
    )
}

fn draw_rows(hdc: HDC, data: &RenderData, mut y: i32, width: i32) {
    let theme = &data.theme;
    let columns = columns(hdc, theme, &data.rows);
    let text_x = theme.padding + columns.index_width + theme.column_gap;
    let annotation_x = text_x + columns.text_width + theme.column_gap;
    for (i, row) in data.rows.iter().enumerate() {
        if i == data.highlight {
            let rect = RECT {
                left: theme.padding / 2,
                top: y,
                right: width - theme.padding / 2,
                bottom: y + columns.row_height,
            };
            fill_round_rect(hdc, rect, theme.highlight, theme.corner_radius / 2);
        }
        let baseline = y + theme.row_padding;
        let text_size = measure(hdc, theme.text_font, &row.text);
        let small_offset = small_offset(hdc, theme, text_size.cy);
        draw_text(
            hdc,
            theme.index_font,
            theme.index_color,
            theme.padding,
            baseline + small_offset,
            &row.index,
        );
        draw_word(hdc, theme, row, text_x, baseline, small_offset);
        let mut x = annotation_x;
        for (segment, tone) in &row.annotation {
            x += draw_text(
                hdc,
                theme.annotation_font,
                tone_color(theme, *tone),
                x,
                baseline + small_offset,
                segment,
            );
        }
        y += columns.row_height;
    }
    if let Some(footer) = &data.footer {
        let size = measure(hdc, theme.index_font, footer);
        draw_text(
            hdc,
            theme.index_font,
            theme.index_color,
            width - theme.padding - size.cx,
            y + theme.row_padding,
            footer,
        );
    }
}

fn draw_horizontal(hdc: HDC, data: &RenderData, y: i32, width: i32) {
    if data.rows.is_empty() {
        return;
    }
    let theme = &data.theme;
    let index_gap = theme.column_gap / 2;
    let highlight_inset = theme.padding / 2;
    let row_height = data
        .rows
        .iter()
        .map(|row| measure(hdc, theme.text_font, &row.text).cy + theme.row_padding * 2)
        .max()
        .unwrap_or(0);
    let baseline = y + theme.row_padding;
    let mut x = theme.padding + highlight_inset;
    for (i, row) in data.rows.iter().enumerate() {
        let index_width = measure(hdc, theme.index_font, &row.index).cx;
        let text_size = measure(hdc, theme.text_font, &row.text);
        let item_width = index_width
            + index_gap
            + cloud_prefix_width(hdc, theme, row)
            + text_size.cx
            + code_width(hdc, theme, row);
        if i == data.highlight {
            let rect = RECT {
                left: x - highlight_inset,
                top: y,
                right: x + item_width + highlight_inset,
                bottom: y + row_height,
            };
            fill_round_rect(hdc, rect, theme.highlight, theme.corner_radius / 2);
        }
        let small_offset = small_offset(hdc, theme, text_size.cy);
        draw_text(
            hdc,
            theme.index_font,
            theme.index_color,
            x,
            baseline + small_offset,
            &row.index,
        );
        draw_word(
            hdc,
            theme,
            row,
            x + index_width + index_gap,
            baseline,
            small_offset,
        );
        x += item_width + theme.column_gap;
    }
    if let Some(footer) = &data.footer {
        let size = measure(hdc, theme.index_font, footer);
        let offset = small_offset(hdc, theme, line_height(hdc, theme.text_font));
        draw_text(
            hdc,
            theme.index_font,
            theme.index_color,
            width - theme.padding - size.cx,
            baseline + offset,
            footer,
        );
    }
    if data.learning_card.is_none() {
        if let Some(row) = data.rows.get(data.highlight) {
            let mut x = theme.padding + highlight_inset;
            let top = y + row_height + theme.row_padding / 2;
            for (segment, tone) in &row.annotation {
                x += draw_text(
                    hdc,
                    theme.annotation_font,
                    tone_color(theme, *tone),
                    x,
                    top,
                    segment,
                );
            }
        }
    }
}

fn top_line_size(hdc: HDC, data: &RenderData) -> (i32, i32) {
    if data.preedit.is_empty() && data.sentence.is_none() {
        return (0, 0);
    }
    let theme = &data.theme;
    let height = line_height(hdc, theme.annotation_font);
    // 没有拼音行时那段宽度为 0，但整句补全前面的间隔照旧。
    let mut width = if data.preedit.is_empty() {
        0
    } else {
        let full: String = data.preedit.iter().map(|(t, _)| t.as_str()).collect();
        measure(hdc, theme.annotation_font, &full).cx + scale_line(theme)
    };
    if let Some(sentence) = &data.sentence {
        width += theme.column_gap
            + cloud_glyph_width(hdc, theme)
            + measure(hdc, theme.annotation_font, sentence).cx;
    }
    (width, height + theme.row_padding * 2)
}

fn columns(hdc: HDC, theme: &Theme, rows: &[Row]) -> Columns {
    let mut columns = Columns {
        index_width: 0,
        text_width: 0,
        annotation_width: 0,
        row_height: 0,
    };
    for row in rows {
        let index = measure(hdc, theme.index_font, &row.index);
        let text = measure(hdc, theme.text_font, &row.text);
        let annotation: i32 = row
            .annotation
            .iter()
            .map(|(s, _)| measure(hdc, theme.annotation_font, s).cx)
            .sum();
        columns.index_width = columns.index_width.max(index.cx);
        columns.text_width = columns
            .text_width
            .max(text.cx + cloud_prefix_width(hdc, theme, row) + code_width(hdc, theme, row));
        columns.annotation_width = columns.annotation_width.max(annotation);
        columns.row_height = columns.row_height.max(text.cy + theme.row_padding * 2);
    }
    columns
}

/// 小字相对候选词往下挪多少才纵向居中（GDI y 向下，取一半差）。
fn small_offset(hdc: HDC, theme: &Theme, text_height: i32) -> i32 {
    ((text_height - line_height(hdc, theme.annotation_font)) / 2).max(0)
}

/// 云朵字形加它与后面文字的间隔。
fn cloud_glyph_width(hdc: HDC, theme: &Theme) -> i32 {
    measure(hdc, theme.annotation_font, CLOUD_GLYPH).cx + theme.column_gap / 2
}

fn cloud_prefix_width(hdc: HDC, theme: &Theme, row: &Row) -> i32 {
    if row.cloud {
        cloud_glyph_width(hdc, theme)
    } else {
        0
    }
}

/// 候选词后面那段辅码的宽度；没有码是 0。
fn code_width(hdc: HDC, theme: &Theme, row: &Row) -> i32 {
    row.code
        .as_ref()
        .map_or(0, |code| measure(hdc, theme.annotation_font, code).cx)
}

/// 画候选词本体，云端词前带小云朵、后面紧跟辅码。返回占用宽度。
fn draw_word(hdc: HDC, theme: &Theme, row: &Row, x: i32, baseline: i32, small_offset: i32) -> i32 {
    let prefix = cloud_prefix_width(hdc, theme, row);
    let color = if row.cloud {
        draw_text(
            hdc,
            theme.annotation_font,
            theme.cloud_color,
            x,
            baseline + small_offset,
            CLOUD_GLYPH,
        );
        theme.cloud_color
    } else {
        theme.text_color
    };
    let mut width =
        prefix + draw_text(hdc, theme.text_font, color, x + prefix, baseline, &row.text);
    if let Some(code) = &row.code {
        width += draw_text(
            hdc,
            theme.annotation_font,
            tone_color(theme, Tone::Code),
            x + width,
            baseline + small_offset,
            code,
        );
    }
    width
}

fn tone_color(theme: &Theme, tone: Tone) -> COLORREF {
    match tone {
        Tone::Gloss => theme.gloss_color,
        Tone::Fresh => theme.fresh_color,
        Tone::Faint => theme.pos_color,
        Tone::Code => theme.gloss_color,
    }
}

/// 拼音行光标前 `cursor` 个字符。
fn concat_before_cursor(preedit: &[(String, PreeditKind)], cursor: usize) -> String {
    preedit
        .iter()
        .flat_map(|(text, _)| text.chars())
        .take(cursor)
        .collect()
}

/// 画一段文字，`(x, y)` 是左上角，返回宽度。调用方先 `SetBkMode(TRANSPARENT)`。
pub(crate) fn draw_text(hdc: HDC, font: HFONT, color: COLORREF, x: i32, y: i32, text: &str) -> i32 {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        let _ = TextOutW(hdc, x, y, &utf16);
        let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
    }
    size.cx
}

pub(crate) fn measure(hdc: HDC, font: HFONT, text: &str) -> SIZE {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        SelectObject(hdc, font.into());
        let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
    }
    size
}

fn line_height(hdc: HDC, font: HFONT) -> i32 {
    measure(hdc, font, "x").cy
}

/// 细线 / 光标的粗细，随 DPI 放大。
fn scale_line(theme: &Theme) -> i32 {
    (theme.padding / 8).max(1)
}

pub(crate) fn fill_rect(hdc: HDC, rect: RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

pub(crate) fn fill_round_rect(hdc: HDC, rect: RECT, color: COLORREF, radius: i32) {
    let diameter = (radius * 2).max(1);
    unsafe {
        let region = CreateRoundRectRgn(
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            diameter,
            diameter,
        );
        let brush = CreateSolidBrush(color);
        let _ = FillRgn(hdc, region, brush);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(region.into());
    }
}

/// GDI 下轻量英语学习卡片的尺寸计算。
fn learning_card_size(hdc: HDC, data: &RenderData) -> (i32, i32) {
    let Some(card) = &data.learning_card else {
        return (0, 0);
    };
    let theme = &data.theme;
    let line_h = line_height(hdc, theme.annotation_font);

    let flag_w = 22;
    let (main_text, has_audio) = match card.status {
        qingjian_render::LearningStatus::Loading => ("正在推荐地道口语……", false),
        qingjian_render::LearningStatus::Failed => ("暂时没有译文", false),
        qingjian_render::LearningStatus::Ready => {
            if let Some(prim) = &card.primary {
                (prim.text.as_str(), true)
            } else if let Some(first) = card.alternatives.first() {
                (first.text.as_str(), true)
            } else {
                ("暂时没有译文", false)
            }
        }
    };
    let audio_w = if has_audio { measure(hdc, theme.annotation_font, " 🔊").cx } else { 0 };
    let main_w = measure(hdc, theme.annotation_font, main_text).cx + audio_w;

    let is_learn = data.learning_mode == qingjian_platform::LearningMode::Learn;

    let width = if is_learn {
        let btn_text = "📖 学习助手(Ctrl+Alt+E)";
        let btn_w = measure(hdc, theme.index_font, btn_text).cx + 16;
        flag_w + main_w + theme.column_gap * 3 + btn_w
    } else {
        flag_w + main_w + theme.column_gap * 2
    };

    let height = line_h + theme.row_padding * 2;
    (width, height)
}

fn draw_mini_flag_gdi(hdc: HDC, x: i32, y: i32) -> i32 {
    let w = 16;
    let h = 11;
    let flag_rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    fill_round_rect(hdc, flag_rect, rgb(179, 25, 66), 2);
    // White stripe 1
    let s1 = RECT { left: x, top: y + 3, right: x + w, bottom: y + 5 };
    fill_rect(hdc, s1, rgb(255, 255, 255));
    // White stripe 2
    let s2 = RECT { left: x, top: y + 6, right: x + w, bottom: y + 8 };
    fill_rect(hdc, s2, rgb(255, 255, 255));
    // Blue canton
    let canton = RECT { left: x, top: y, right: x + 7, bottom: y + 6 };
    fill_round_rect(hdc, canton, rgb(10, 49, 97), 2);
    w + 6
}

/// GDI 下绘制轻量英语学习卡片。
fn draw_learning_card(hdc: HDC, data: &RenderData, mut y: i32, content_width: i32) {
    let Some(card) = &data.learning_card else {
        return;
    };
    let theme = &data.theme;
    let sep = RECT {
        left: theme.padding,
        top: y + theme.row_padding / 2,
        right: content_width - theme.padding,
        bottom: y + theme.row_padding / 2 + 1,
    };
    fill_rect(hdc, sep, theme.pos_color);
    y += theme.row_padding;

    let line_h = line_height(hdc, theme.annotation_font);
    let mut x = theme.padding;

    let is_learn = data.learning_mode == qingjian_platform::LearningMode::Learn;

    // 绘制精致矢量微标，彻底消除 "us" 退化
    x += draw_mini_flag_gdi(hdc, x, y + (line_h - 11) / 2);

    // 核心口语表达
    let (main_text, has_audio) = match card.status {
        qingjian_render::LearningStatus::Loading => ("正在推荐地道口语……", false),
        qingjian_render::LearningStatus::Failed => ("暂时没有译文", false),
        qingjian_render::LearningStatus::Ready => {
            if let Some(prim) = &card.primary {
                (prim.text.as_str(), true)
            } else if let Some(first) = card.alternatives.first() {
                (first.text.as_str(), true)
            } else {
                ("暂时没有译文", false)
            }
        }
    };
    let text_color = if card.status == qingjian_render::LearningStatus::Ready {
        theme.text_color
    } else {
        theme.gloss_color
    };
    x += draw_text(hdc, theme.annotation_font, text_color, x, y, main_text);
    if has_audio {
        draw_text(hdc, theme.index_font, theme.cloud_color, x, y, " 🔊");
    }

    if is_learn {
        let btn_text = "📖 学习助手(Ctrl+Alt+E)";
        let btn_w = measure(hdc, theme.index_font, btn_text).cx + 16;
        let btn_x = content_width - theme.padding - btn_w;
        let btn_rect = RECT { left: btn_x, top: y - 1, right: btn_x + btn_w, bottom: y + line_h };
        fill_round_rect(hdc, btn_rect, theme.highlight, 4);
        draw_text(hdc, theme.index_font, theme.gloss_color, btn_x + 8, y, btn_text);
    }
}

use crate::dispatch::CandidateAction;

/// 命中测试：判断鼠标点击是否落在学习卡片及其内部交互控件上。
pub(super) fn hit_test_learning_card(
    hdc: HDC,
    data: &RenderData,
    x: i32,
    y: i32,
    content_w: i32,
) -> Option<CandidateAction> {
    let _card = data.learning_card.as_ref()?;
    let theme = &data.theme;
    let top_h = top_line_size(hdc, data).1;
    let body_h = match data.layout {
        LayoutMode::Vertical => vertical_size(hdc, data).1,
        LayoutMode::Horizontal => horizontal_size(hdc, data).1,
    };
    let card_top = theme.padding + top_h + body_h;
    let line_h = line_height(hdc, theme.annotation_font);
    let row1_h = line_h + theme.row_padding * 2;

    if y < card_top.saturating_sub(6) || y > card_top + row1_h + 6 {
        return None;
    }

    let content_w = if content_w > 0 {
        content_w
    } else {
        preferred_size(hdc, data).cx
    };
    let is_learn = data.learning_mode == qingjian_platform::LearningMode::Learn;
    let is_drill = data.learning_mode == qingjian_platform::LearningMode::Drill;

    if is_learn {
        let btn_text = "📖 学习助手(Ctrl+Alt+E)";
        let btn_w = measure(hdc, theme.index_font, btn_text).cx + 16;
        let btn_x = content_w - theme.padding - btn_w;
        if x >= btn_x - 8 && x <= content_w {
            return Some(CandidateAction::OpenAssistant);
        }
        return Some(CandidateAction::OpenAssistant);
    } else if is_drill {
        let btn_text = "💡 进入练习 (Ctrl+Alt+D)";
        let btn_w = measure(hdc, theme.index_font, btn_text).cx + 16;
        if x >= theme.padding - 8 && x <= theme.padding + btn_w + 16 {
            return Some(CandidateAction::StartDrillSession);
        }
        return Some(CandidateAction::StartDrillSession);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC};
    use qingjian_core::{Candidate, CandidateList};
    use qingjian_platform::LearningMode;
    use qingjian_platform::protocol::{
        Frame, LearningCardFrame, LearningExpression, LearningState,
    };

    fn make_test_data(mode: LearningMode, card: Option<LearningCardFrame>) -> RenderData {
        let theme = Rc::new(Theme::new(96, false));
        let frame = Frame {
            candidates: CandidateList {
                items: vec![Candidate {
                    text: "你好".to_string(),
                    kind: qingjian_core::CandidateKind::Chinese,
                    syllables: Vec::new(),
                    reading: None,
                    translation: None,
                    aux_code: None,
                }],
            },
            learning_mode: mode,
            learning_card: card,
            ..Frame::default()
        };
        let mut data = RenderData::empty(theme);
        data.set(&frame);
        data
    }

    #[test]
    fn test_hit_test_daily_mode_no_interfere() {
        let dc = unsafe { CreateCompatibleDC(None) };
        let data = make_test_data(LearningMode::Daily, None);
        // Daily mode maintains clean input experience, no interfering buttons
        let action = hit_test_learning_card(dc, &data, 50, 60, 300);
        assert_eq!(action, None);
        unsafe { let _ = DeleteDC(dc); }
    }

    #[test]
    fn test_hit_test_learn_mode_open_assistant() {
        let dc = unsafe { CreateCompatibleDC(None) };
        let card = LearningCardFrame {
            target_text: "你好".to_string(),
            state: LearningState::Ready,
            primary: Some(LearningExpression {
                text: "Hello there".to_string(),
                ..Default::default()
            }),
            alternatives: vec![],
            expanded: false,
            selected_index: None,
            drill_stage: None,
            drill_input: String::new(),
            drill_attempt: None,
            drill_notice: None,
            today_due_count: 0,
            review_current_index: 0,
            review_total_count: 0,
            upgrade: None,
        };
        let data = make_test_data(LearningMode::Learn, Some(card));
        let content_w = 360;
        let target_y = data.theme.padding + top_line_size(dc, &data).1 + horizontal_size(dc, &data).1 + 6;

        // Click on 学习助手 button / line
        let action = hit_test_learning_card(dc, &data, 300, target_y, content_w);
        assert_eq!(action, Some(CandidateAction::OpenAssistant));

        unsafe { let _ = DeleteDC(dc); }
    }

    #[test]
    fn test_hit_test_drill_mode_start_drill_session() {
        let dc = unsafe { CreateCompatibleDC(None) };
        let card = LearningCardFrame {
            target_text: "你好".to_string(),
            state: LearningState::Ready,
            primary: Some(LearningExpression {
                text: "Hello there".to_string(),
                ..Default::default()
            }),
            alternatives: vec![],
            expanded: false,
            selected_index: None,
            drill_stage: None,
            drill_input: String::new(),
            drill_attempt: None,
            drill_notice: None,
            today_due_count: 0,
            review_current_index: 0,
            review_total_count: 0,
            upgrade: None,
        };
        let data = make_test_data(LearningMode::Drill, Some(card));
        let content_w = 360;
        let target_y = data.theme.padding + top_line_size(dc, &data).1 + horizontal_size(dc, &data).1 + 6;

        // Click on 进入练习 button (located on left)
        let action = hit_test_learning_card(dc, &data, 50, target_y, content_w);
        assert_eq!(action, Some(CandidateAction::StartDrillSession));

        unsafe { let _ = DeleteDC(dc); }
    }
}

