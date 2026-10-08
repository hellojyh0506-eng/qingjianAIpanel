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
use super::theme::Theme;

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
    if let Some((annotation_width, annotation_height)) = highlighted_annotation_size(hdc, data) {
        width = width.max(annotation_width);
        height += annotation_height;
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
    if data.learning_card.is_none() {
        if data.learning_mode == qingjian_platform::LearningMode::Daily && !data.rows.is_empty() {
            let prompt = "按 Ctrl+Shift+E 查看英文";
            let prompt_w = measure(hdc, data.theme.index_font, prompt).cx;
            let small_h = line_height(hdc, data.theme.index_font);
            let w = 28 + 6 + prompt_w + data.theme.padding * 2;
            let h = small_h + data.theme.row_padding;
            return (w, h);
        }
        return (0, 0);
    }
    let Some(card) = &data.learning_card else {
        return (0, 0);
    };
    let theme = &data.theme;
    let mut width = 0;
    let mut height = 0;
    let line_h = line_height(hdc, theme.annotation_font);
    let small_h = line_height(hdc, theme.index_font);

    // Active Recall 特殊阶段
    if let Some(stage) = card.drill_stage {
        match stage {
            qingjian_render::DrillStage::Input => {
                let title = format!("💡 请用英语表达：{}", card.target_text);
                let title_w = measure(hdc, theme.annotation_font, &title).cx;
                let placeholder = "在这里输入你的英文表达...";
                let input_str = if card.drill_input.is_empty() {
                    format!("{}|", placeholder)
                } else {
                    format!("{}|", card.drill_input)
                };
                let input_w = measure(hdc, theme.text_font, &input_str).cx + 28;
                let btns_w = 190;

                width = width.max(title_w).max(input_w).max(btns_w).max(280) + theme.column_gap * 2;
                height += (line_h + theme.row_padding) * 3;
                if card.drill_notice.is_some() {
                    height += small_h + theme.row_padding;
                }
                return (width, height);
            }
            qingjian_render::DrillStage::Comparing => {
                let text = "⏳ 正在分析对比表达中...";
                let w = measure(hdc, theme.annotation_font, text).cx;
                return (w + theme.column_gap * 2, line_h + theme.row_padding * 2);
            }
            qingjian_render::DrillStage::Compared => {
                let user_expr = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.user_attempt.as_str())
                    .unwrap_or(card.drill_input.as_str());
                let natural_expr = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.comparison.natural_expression.as_str())
                    .unwrap_or("");
                let advice = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.comparison.key_advice.as_str())
                    .unwrap_or("");

                let row1 = format!("❌ 你的表达: {}", user_expr);
                let row2 = format!("✅ 更自然的表达: {} 🔊", natural_expr);
                let btns = "[🔄 再试一次 (R)]   [⭐ 保存到表达本 (S)]   [完成 (Enter)]";

                let r1_w = measure(hdc, theme.annotation_font, &row1).cx;
                let r2_w = measure(hdc, theme.annotation_font, &row2).cx;
                let advice_w = measure(hdc, theme.index_font, &format!("ⓘ 关键建议: {}", advice)).cx;
                let btns_w = measure(hdc, theme.index_font, btns).cx;

                width = width.max(r1_w + r2_w + theme.column_gap * 2).max(btns_w).max(advice_w).max(330);
                height += (line_h * 2 + 14) + (small_h * 2 + 8) + (line_h + 4) + theme.row_padding * 2;

                if card.drill_notice.is_some() {
                    height += small_h + theme.row_padding;
                }
                return (width, height);
            }
            qingjian_render::DrillStage::Profile => {
                let title = "📊 个人英语学习画像 (B2 → C1)";
                let title_w = measure(hdc, theme.annotation_font, title).cx;
                let btns = "[🗑 清空画像数据]   [返回 (Esc)]";
                let btns_w = measure(hdc, theme.index_font, btns).cx;
                width = width.max(title_w).max(btns_w);
                height += (line_h + theme.row_padding) * 2;

                if let Some(text) = &card.drill_notice {
                    for line in text.lines() {
                        let lw = measure(hdc, theme.index_font, line).cx;
                        width = width.max(lw);
                        height += small_h + theme.row_padding / 2;
                    }
                } else {
                    height += (small_h + theme.row_padding / 2) * 8;
                }

                width += theme.column_gap * 2;
                height += theme.row_padding * 2;
                return (width, height);
            }
            qingjian_render::DrillStage::Review => {
                let total = card.review_total_count.max(1);
                let current = card.review_current_index.max(1);
                let title = format!("📚 今日待复习 ({}/{}) [中文: {}]", current, total, card.target_text);
                let title_w = measure(hdc, theme.annotation_font, &title).cx;

                if card.drill_attempt.is_none() {
                    let input_str = format!("你的尝试: {}|", card.drill_input);
                    let input_w = measure(hdc, theme.annotation_font, &input_str).cx;
                    let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                    let btns_w = measure(hdc, theme.index_font, btns).cx;
                    width = width.max(title_w).max(input_w).max(btns_w) + theme.column_gap * 2;
                    height += (line_h + theme.row_padding) * 3;
                } else {
                    let user_expr = card.drill_attempt.as_ref().map(|a| a.user_attempt.as_str()).unwrap_or(card.drill_input.as_str());
                    let natural_expr = card.drill_attempt.as_ref().map(|a| a.comparison.natural_expression.as_str()).unwrap_or("");
                    let r1 = format!("🔴 你的表达: {}", user_expr);
                    let r2 = format!("🟢 地道表达: {}", natural_expr);
                    let r1_w = measure(hdc, theme.annotation_font, &r1).cx;
                    let r2_w = measure(hdc, theme.annotation_font, &r2).cx;
                    let btns = "[1. Again (重来)]   [2. Good (掌握)]   [3. Easy (轻松)]   [退出 (Esc)]";
                    let btns_w = measure(hdc, theme.index_font, btns).cx;
                    width = width.max(title_w).max(r1_w).max(r2_w).max(btns_w) + theme.column_gap * 2;
                    height += (line_h + theme.row_padding) * 3 + theme.row_padding;
                }
                if card.drill_notice.is_some() {
                    height += small_h + theme.row_padding;
                }
                return (width, height);
            }
        }
    }

    let flag = "🇺🇸 ";
    let flag_w = measure(hdc, theme.annotation_font, flag).cx;
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
    let drill_btn = "Try English";
    let drill_w = measure(hdc, theme.index_font, drill_btn).cx + 16;
    let btn_text = if card.expanded { "收起 ∧" } else { "更多译法 ∨" };
    let btn_w = measure(hdc, theme.index_font, btn_text).cx + 16;

    let row1_w = flag_w + main_w + theme.column_gap * 3 + drill_w + theme.column_gap + btn_w;
    width = width.max(row1_w);
    height += line_h + theme.row_padding * 2;

    if card.expanded && !card.alternatives.is_empty() {
        height += 4;
        for (idx, alt) in card.alternatives.iter().take(5).enumerate() {
            let idx_str = format!("{}. ", idx + 1);
            let idx_w = measure(hdc, theme.index_font, &idx_str).cx;
            let text_w = measure(hdc, theme.annotation_font, &alt.text).cx;
            let tag_raw = if alt.tag.is_empty() { "Natural" } else { alt.tag.as_str() };
            let tag_w = measure(hdc, theme.index_font, tag_raw).cx + 12;
            let meaning_str = if alt.meaning.is_empty() {
                String::new()
            } else {
                format!("› {}", alt.meaning)
            };
            let note_w = if meaning_str.is_empty() {
                0
            } else {
                measure(hdc, theme.index_font, &meaning_str).cx + theme.column_gap
            };
            let audio_w = measure(hdc, theme.annotation_font, " 🔊").cx;
            let item_w = theme.padding + idx_w + text_w + theme.column_gap + tag_w + note_w + theme.column_gap + audio_w + theme.padding;
            width = width.max(item_w);
            height += line_h + theme.row_padding;
        }
    }
    (width, height)
}

/// GDI 下绘制轻量英语学习卡片。
fn draw_learning_card(hdc: HDC, data: &RenderData, mut y: i32, content_width: i32) {
    if data.learning_mode == qingjian_platform::LearningMode::Daily {
        if !data.rows.is_empty() {
            let bx = data.theme.padding;
            let by = y + data.theme.row_padding / 2;
            let badge_w = 20;
            let badge_h = 13;
            let badge_rect = RECT {
                left: bx,
                top: by + 1,
                right: bx + badge_w,
                bottom: by + 1 + badge_h,
            };
            fill_round_rect(hdc, badge_rect, COLORREF(0x001673f9), 3);
            draw_text(hdc, data.theme.index_font, COLORREF(0x00FFFFFF), bx + 4, by, "us");
            draw_text(hdc, data.theme.index_font, data.theme.gloss_color, bx + badge_w + 6, by, "按 Ctrl+Shift+E 查看英文");
        }
        return;
    }
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
    let small_h = line_height(hdc, theme.index_font);

    // Active Recall 特殊阶段绘制
    if let Some(stage) = card.drill_stage {
        match stage {
            qingjian_render::DrillStage::Input => {
                let title = format!("💡 请用英语表达：{}", card.target_text);
                draw_text(
                    hdc,
                    theme.annotation_font,
                    theme.text_color,
                    theme.padding,
                    y,
                    &title,
                );
                y += line_h + theme.row_padding / 2;

                let box_rect = RECT {
                    left: theme.padding,
                    top: y,
                    right: content_width - theme.padding,
                    bottom: y + line_h + 8,
                };
                fill_round_rect(hdc, box_rect, theme.highlight, theme.corner_radius / 2);
                if card.drill_input.is_empty() {
                    draw_text(
                        hdc,
                        theme.index_font,
                        theme.gloss_color,
                        theme.padding + 6,
                        y + 3,
                        "在这里输入你的英文表达...",
                    );
                } else {
                    let input_text = format!("{}|", card.drill_input);
                    draw_text(
                        hdc,
                        theme.text_font,
                        theme.text_color,
                        theme.padding + 6,
                        y + 3,
                        &input_text,
                    );
                }
                y += line_h + 10 + theme.row_padding / 2;

                // 底部操作按钮：[对比 (Enter)] 与 [显示答案]
                let btn_enter_text = "对比 (Enter)";
                let enter_w = measure(hdc, theme.index_font, btn_enter_text).cx + 16;
                let enter_rect = RECT {
                    left: theme.padding,
                    top: y,
                    right: theme.padding + enter_w,
                    bottom: y + line_h + 2,
                };
                fill_round_rect(hdc, enter_rect, COLORREF(0x0099A600), 4);
                draw_text(hdc, theme.index_font, COLORREF(0x00FFFFFF), theme.padding + 8, y + 1, btn_enter_text);

                let show_x = theme.padding + enter_w + theme.column_gap;
                let btn_show_text = "显示答案";
                let show_w = measure(hdc, theme.index_font, btn_show_text).cx + 16;
                let show_rect = RECT {
                    left: show_x,
                    top: y,
                    right: show_x + show_w,
                    bottom: y + line_h + 2,
                };
                fill_round_rect(hdc, show_rect, theme.highlight, 4);
                draw_text(hdc, theme.index_font, theme.text_color, show_x + 8, y + 1, btn_show_text);

                if let Some(notice) = &card.drill_notice {
                    y += line_h + 4 + theme.row_padding / 2;
                    draw_text(hdc, theme.index_font, COLORREF(0x001673f9), theme.padding, y, notice);
                }
                return;
            }
            qingjian_render::DrillStage::Comparing => {
                draw_text(
                    hdc,
                    theme.annotation_font,
                    theme.fresh_color,
                    theme.padding,
                    y,
                    "⏳ 正在分析对比表达中...",
                );
                return;
            }
            qingjian_render::DrillStage::Compared => {
                let user_expr = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.user_attempt.as_str())
                    .unwrap_or(card.drill_input.as_str());
                let natural_expr = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.comparison.natural_expression.as_str())
                    .unwrap_or("");
                let advice = card
                    .drill_attempt
                    .as_ref()
                    .map(|a| a.comparison.key_advice.as_str())
                    .unwrap_or("");

                // 1. 你的表达
                let r1 = format!("❌ 你的表达: {}", user_expr);
                draw_text(hdc, theme.annotation_font, theme.text_color, theme.padding, y, &r1);
                y += line_h + theme.row_padding / 2;

                // 2. 更地道表达
                let r2 = format!("✅ 更自然的表达: {} 🔊", natural_expr);
                draw_text(hdc, theme.annotation_font, theme.fresh_color, theme.padding, y, &r2);
                y += line_h + theme.row_padding / 2;

                // 3. 关键建议
                if !advice.is_empty() {
                    let r3 = format!("ⓘ 关键建议: {}", advice);
                    draw_text(hdc, theme.index_font, theme.pos_color, theme.padding, y, &r3);
                    y += small_h + theme.row_padding / 2;
                }

                // 4. 底部三按钮：[🔄 再试一次 (R)] [⭐ 保存到表达本 (S)] [完成 (Enter)]
                let mut bx = theme.padding;
                let btn_h = line_h + 2;

                let retry_text = "🔄 再试一次 (R)";
                let retry_w = measure(hdc, theme.index_font, retry_text).cx + 16;
                let retry_rect = RECT { left: bx, top: y, right: bx + retry_w, bottom: y + btn_h };
                fill_round_rect(hdc, retry_rect, theme.highlight, 4);
                draw_text(hdc, theme.index_font, theme.text_color, bx + 8, y + 1, retry_text);
                bx += retry_w + theme.column_gap;

                let save_text = "⭐ 保存到表达本 (S)";
                let save_w = measure(hdc, theme.index_font, save_text).cx + 16;
                let save_rect = RECT { left: bx, top: y, right: bx + save_w, bottom: y + btn_h };
                fill_round_rect(hdc, save_rect, theme.highlight, 4);
                draw_text(hdc, theme.index_font, theme.text_color, bx + 8, y + 1, save_text);
                bx += save_w + theme.column_gap;

                let done_text = "完成 (Enter)";
                let done_w = measure(hdc, theme.index_font, done_text).cx + 16;
                let done_rect = RECT { left: bx, top: y, right: bx + done_w, bottom: y + btn_h };
                fill_round_rect(hdc, done_rect, COLORREF(0x0099A600), 4);
                draw_text(hdc, theme.index_font, COLORREF(0x00FFFFFF), bx + 8, y + 1, done_text);

                if let Some(notice) = &card.drill_notice {
                    y += btn_h + theme.row_padding / 2;
                    draw_text(hdc, theme.index_font, COLORREF(0x001673f9), theme.padding, y, notice);
                }
                return;
            }
            qingjian_render::DrillStage::Profile => {
                let title = "📊 个人英语学习画像 (B2 → C1)";
                draw_text(hdc, theme.annotation_font, theme.fresh_color, theme.padding, y, title);
                y += line_h + theme.row_padding / 2;

                if let Some(text) = &card.drill_notice {
                    for line in text.lines() {
                        draw_text(hdc, theme.index_font, theme.text_color, theme.padding, y, line);
                        y += small_h + theme.row_padding / 2;
                    }
                }

                let btns = "[🗑 清空画像数据]   [返回 (Esc)]";
                draw_text(hdc, theme.index_font, theme.cloud_color, theme.padding, y + theme.row_padding / 2, btns);
                return;
            }
            qingjian_render::DrillStage::Review => {
                let total = card.review_total_count.max(1);
                let current = card.review_current_index.max(1);
                let title = format!("📚 今日待复习 ({}/{}) [中文: {}]", current, total, card.target_text);
                draw_text(hdc, theme.annotation_font, theme.fresh_color, theme.padding, y, &title);
                y += line_h + theme.row_padding / 2;

                if let Some(notice) = &card.drill_notice {
                    draw_text(hdc, theme.index_font, theme.cloud_color, theme.padding, y, notice);
                    y += small_h + theme.row_padding / 2;
                }

                if card.drill_attempt.is_none() {
                    let input_rect = RECT {
                        left: theme.padding,
                        top: y,
                        right: content_width - theme.padding,
                        bottom: y + line_h + 6,
                    };
                    fill_round_rect(hdc, input_rect, theme.highlight, theme.corner_radius / 2);
                    let input_str = format!("你的尝试: {}|", card.drill_input);
                    draw_text(hdc, theme.annotation_font, theme.text_color, theme.padding + 4, y + 2, &input_str);
                    y += line_h + 8 + theme.row_padding / 2;

                    let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                    draw_text(hdc, theme.index_font, theme.cloud_color, theme.padding, y, btns);
                } else {
                    let user_expr = card.drill_attempt.as_ref().map(|a| a.user_attempt.as_str()).unwrap_or(card.drill_input.as_str());
                    let natural_expr = card.drill_attempt.as_ref().map(|a| a.comparison.natural_expression.as_str()).unwrap_or("");
                    let r1 = format!("🔴 你的表达: {}", user_expr);
                    draw_text(hdc, theme.annotation_font, theme.text_color, theme.padding, y, &r1);
                    y += line_h + theme.row_padding / 2;

                    let r2 = format!("🟢 地道表达: {}", natural_expr);
                    draw_text(hdc, theme.annotation_font, theme.fresh_color, theme.padding, y, &r2);
                    y += line_h + theme.row_padding / 2;

                    let btns = "[1. Again (重来)]   [2. Good (掌握)]   [3. Easy (轻松)]   [退出 (Esc)]";
                    draw_text(hdc, theme.index_font, theme.fresh_color, theme.padding, y, btns);
                }
                return;
            }
        }
    }

    let flag = "🇺🇸 ";
    let mut x = theme.padding;
    x += draw_text(hdc, theme.annotation_font, theme.fresh_color, x, y, flag);

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

    let drill_btn = "Try English";
    let drill_w = measure(hdc, theme.index_font, drill_btn).cx + 12;
    let btn_text = if card.expanded { "收起 ∧" } else { "更多译法 ∨" };
    let btn_w = measure(hdc, theme.index_font, btn_text).cx + 12;
    let drill_x = content_width - theme.padding - drill_w;
    let btn_x = drill_x - theme.column_gap - btn_w;

    let btn_rect = RECT { left: btn_x, top: y - 1, right: btn_x + btn_w, bottom: y + line_h };
    fill_round_rect(hdc, btn_rect, theme.highlight, 4);
    draw_text(hdc, theme.index_font, theme.cloud_color, btn_x + 6, y, btn_text);

    let drill_rect = RECT { left: drill_x, top: y - 1, right: drill_x + drill_w, bottom: y + line_h };
    fill_round_rect(hdc, drill_rect, theme.highlight, 4);
    draw_text(hdc, theme.index_font, theme.fresh_color, drill_x + 6, y, drill_btn);

    y += line_h + theme.row_padding;

    if card.expanded && !card.alternatives.is_empty() {
        for (idx, alt) in card.alternatives.iter().take(5).enumerate() {
            let row_h = line_h + theme.row_padding;
            if card.selected_index == Some(idx) {
                let rect = RECT {
                    left: theme.padding / 2,
                    top: y - theme.row_padding / 4,
                    right: content_width - theme.padding / 2,
                    bottom: y - theme.row_padding / 4 + row_h,
                };
                fill_round_rect(hdc, rect, theme.highlight, theme.corner_radius / 2);
            }
            let mut ax = theme.padding + 4;
            let idx_str = format!("{}. ", idx + 1);
            ax += draw_text(hdc, theme.index_font, theme.gloss_color, ax, y, &idx_str);
            ax += draw_text(hdc, theme.annotation_font, theme.text_color, ax, y, &alt.text);
            ax += theme.column_gap;
            let tag_raw = if alt.tag.is_empty() { "Natural" } else { alt.tag.as_str() };
            let tag_str = format!("[{}]", tag_raw);
            ax += draw_text(hdc, theme.index_font, theme.fresh_color, ax, y, &tag_str);
            if !alt.meaning.is_empty() {
                ax += theme.column_gap;
                let meaning_str = format!("› {}", alt.meaning);
                ax += draw_text(hdc, theme.index_font, theme.pos_color, ax, y, &meaning_str);
            }
            draw_text(hdc, theme.index_font, theme.cloud_color, ax + theme.column_gap, y, "🔊");
            y += row_h;
        }
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
    let theme = &data.theme;
    let top_h = top_line_size(hdc, data).1;
    let body_h = match data.layout {
        LayoutMode::Vertical => vertical_size(hdc, data).1,
        LayoutMode::Horizontal => horizontal_size(hdc, data).1,
    };
    let card_top = theme.padding + top_h + body_h;

    // 日常模式下点击底部「按 Ctrl+Shift+E 查看英文」提示行
    if data.learning_mode == qingjian_platform::LearningMode::Daily {
        if !data.rows.is_empty() && y >= card_top.saturating_sub(6) {
            return Some(CandidateAction::ToggleExpand);
        }
        return None;
    }

    let card = data.learning_card.as_ref()?;
    let line_h = line_height(hdc, theme.annotation_font);

    // Active Recall 特殊阶段点击判定
    if let Some(stage) = card.drill_stage {
        match stage {
            qingjian_render::DrillStage::Input => {
                let btn_y = card_top + (line_h + theme.row_padding) * 2;
                if y >= btn_y.saturating_sub(8) && y <= btn_y + line_h + 16 {
                    let enter_w = measure(hdc, theme.index_font, "对比 (Enter)").cx + 16;
                    let show_x = theme.padding + enter_w + theme.column_gap;
                    let show_w = measure(hdc, theme.index_font, "显示答案").cx + 16;
                    let split_x = (theme.padding + enter_w + show_x) / 2;
                    if x >= theme.padding.saturating_sub(10) && x < split_x {
                        return Some(CandidateAction::Compare);
                    } else if x >= split_x && x <= show_x + show_w + 16 {
                        return Some(CandidateAction::Reveal);
                    }
                }
                return None;
            }
            qingjian_render::DrillStage::Comparing => {
                return None;
            }
            qingjian_render::DrillStage::Compared => {
                let btn_y = card_top + (line_h + theme.row_padding) * 3;
                if y >= btn_y.saturating_sub(8) && y <= btn_y + line_h + 16 {
                    let retry_w = measure(hdc, theme.index_font, "🔄 再试一次 (R)").cx + 16;
                    let save_w = measure(hdc, theme.index_font, "⭐ 保存到表达本 (S)").cx + 16;
                    let done_w = measure(hdc, theme.index_font, "完成 (Enter)").cx + 16;
                    let b1_start = theme.padding;
                    let b1_end = b1_start + retry_w;
                    let b2_start = b1_end + theme.column_gap;
                    let b2_end = b2_start + save_w;
                    let b3_start = b2_end + theme.column_gap;
                    let b3_end = b3_start + done_w;
                    let split1 = (b1_end + b2_start) / 2;
                    let split2 = (b2_end + b3_start) / 2;
                    if x >= b1_start.saturating_sub(8) && x < split1 {
                        return Some(CandidateAction::Retry);
                    } else if x >= split1 && x < split2 {
                        return Some(CandidateAction::Save);
                    } else if x >= split2 && x <= b3_end + 16 {
                        return Some(CandidateAction::Finish);
                    }
                }
                return None;
            }
            qingjian_render::DrillStage::Profile => {
                let btn_y = card_top + (line_h + theme.row_padding) * 2;
                if y >= btn_y.saturating_sub(8) {
                    if x < theme.padding + 160 {
                        return Some(CandidateAction::ClearVault);
                    } else {
                        return Some(CandidateAction::Finish);
                    }
                }
                return None;
            }
            qingjian_render::DrillStage::Review => {
                if card.drill_attempt.is_none() {
                    let btn_y = card_top + (line_h + theme.row_padding) * 2;
                    if y >= btn_y.saturating_sub(8) {
                        if x < theme.padding + 130 {
                            return Some(CandidateAction::Compare);
                        } else if x < theme.padding + 260 {
                            return Some(CandidateAction::Reveal);
                        } else {
                            return Some(CandidateAction::Finish);
                        }
                    }
                } else {
                    let btn_y = card_top + (line_h + theme.row_padding) * 2 + line_h;
                    if y >= btn_y.saturating_sub(8) {
                        if x < theme.padding + 140 {
                            return Some(CandidateAction::RateAgain);
                        } else if x < theme.padding + 270 {
                            return Some(CandidateAction::RateGood);
                        } else if x < theme.padding + 400 {
                            return Some(CandidateAction::RateEasy);
                        } else {
                            return Some(CandidateAction::Finish);
                        }
                    }
                }
                return None;
            }
        }
    }

    let row1_h = line_h + theme.row_padding * 2;
    let btn_y_min = card_top.saturating_sub(10);
    let btn_y_max = card_top + row1_h + 10;
    if y >= btn_y_min && y <= btn_y_max {
        let content_w = if content_w > 0 {
            content_w
        } else {
            preferred_size(hdc, data).cx
        };
        let drill_btn = "Try English";
        let drill_w = measure(hdc, theme.index_font, drill_btn).cx + 12;
        let btn_text = if card.expanded { "收起 ∧" } else { "更多译法 ∨" };
        let btn_w = measure(hdc, theme.index_font, btn_text).cx + 12;
        let drill_x = content_w - theme.padding - drill_w;
        let btn_x = drill_x - theme.column_gap - btn_w;
        let split_x = (btn_x + btn_w + drill_x) / 2;

        if x >= btn_x - 16 && x < split_x {
            return Some(CandidateAction::ToggleExpand);
        } else if x >= split_x && x <= content_w + 16 {
            return Some(CandidateAction::StartDrill);
        }
    }

    if card.expanded && !card.alternatives.is_empty() && y > card_top + row1_h.saturating_sub(4) {
        let alt_start = card_top + row1_h;
        let alt_row_h = line_h + theme.row_padding;
        let alt_idx = (y - alt_start) / alt_row_h;
        if alt_idx >= 0 && (alt_idx as usize) < card.alternatives.len().min(5) {
            return Some(CandidateAction::ToggleAlternative(alt_idx as usize));
        }
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
        DrillStage, Frame, LearningCardFrame, LearningExpression, LearningState,
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
    fn test_hit_test_daily_mode_click_to_learn() {
        let dc = unsafe { CreateCompatibleDC(None) };
        let data = make_test_data(LearningMode::Daily, None);
        // Daily mode with candidate rows displays bottom prompt: "us 按 Ctrl+Shift+E 查看英文"
        let action = hit_test_learning_card(dc, &data, 50, 60, 300);
        assert_eq!(action, Some(CandidateAction::ToggleExpand));
        unsafe { let _ = DeleteDC(dc); }
    }

    #[test]
    fn test_hit_test_learn_mode_expand_and_drill() {
        let dc = unsafe { CreateCompatibleDC(None) };
        let card = LearningCardFrame {
            target_text: "你好".to_string(),
            state: LearningState::Ready,
            primary: Some(LearningExpression {
                text: "Hello there".to_string(),
                ..Default::default()
            }),
            alternatives: vec![
                LearningExpression {
                    text: "Hi there".to_string(),
                    meaning: "嗨".to_string(),
                    ..Default::default()
                },
            ],
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

        // Click on 更多译法 button (around x=180..240, y in button row)
        let action_expand = hit_test_learning_card(dc, &data, 200, 50, content_w);
        assert_eq!(action_expand, Some(CandidateAction::ToggleExpand));

        // Click on Try English button (around x=280..340, y in button row)
        let action_drill = hit_test_learning_card(dc, &data, 320, 50, content_w);
        assert_eq!(action_drill, Some(CandidateAction::StartDrill));

        unsafe { let _ = DeleteDC(dc); }
    }

    #[test]
    fn test_hit_test_drill_stage_input_buttons() {
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
            drill_stage: Some(DrillStage::Input),
            drill_input: "hello".to_string(),
            drill_attempt: None,
            drill_notice: None,
            today_due_count: 0,
            review_current_index: 0,
            review_total_count: 0,
            upgrade: None,
        };
        let data = make_test_data(LearningMode::Drill, Some(card));
        let content_w = 360;

        // Compare button is at left
        let action_comp = hit_test_learning_card(dc, &data, 30, 95, content_w);
        assert_eq!(action_comp, Some(CandidateAction::Compare));

        // Reveal button is to the right
        let action_reveal = hit_test_learning_card(dc, &data, 160, 95, content_w);
        assert_eq!(action_reveal, Some(CandidateAction::Reveal));

        unsafe { let _ = DeleteDC(dc); }
    }
}

