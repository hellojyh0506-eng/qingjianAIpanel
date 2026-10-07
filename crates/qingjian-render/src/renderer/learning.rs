//! 轻量英语学习卡片（候选窗口第三层与渐进展开层）的尺寸计算与绘制。

use super::Metrics;
use super::Renderer;
use crate::canvas::Canvas;
use crate::color::Color;
use crate::frame::{Frame, LearningStatus};

impl Renderer {
    /// 计算轻量英语学习卡片需要的尺寸（若不存在卡片返回 0, 0）。
    pub(super) fn learning_card_size(&mut self, frame: &Frame, m: &Metrics) -> (f32, f32) {
        let Some(card) = &frame.learning_card else {
            return (0.0, 0.0);
        };
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        let style = m.annotation_style(m.theme.colors.gloss);
        let tag_style = m.style(m.theme.index_font, m.theme.colors.fresh);
        let small_style = m.style(m.theme.index_font, m.theme.colors.pos);
        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));

        // Active Recall 特殊阶段尺寸计算
        if let Some(stage) = card.drill_stage {
            match stage {
                crate::frame::DrillStage::Input => {
                    let title = format!("🎯 英文回忆练习 (中文: {})", card.target_text);
                    let title_w = self.measure(&title, &style).width;
                    let input_str = format!("你的尝试: {}|", card.drill_input);
                    let input_w = self.measure(&input_str, &m.annotation_style(m.theme.colors.text)).width;
                    let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                    let btns_w = self.measure(btns, &m.index_style()).width;

                    width = width.max(title_w).max(input_w).max(btns_w) + m.column_gap() * 2.0;
                    height += (line_height + m.row_padding()) * 3.0;
                    if card.drill_notice.is_some() {
                        height += small_style.line_height + m.row_padding();
                    }
                    return (width, height);
                }
                crate::frame::DrillStage::Comparing => {
                    let text = "⏳ 正在分析对比表达中...";
                    let w = self.measure(text, &style).width;
                    return (w + m.column_gap() * 2.0, line_height + m.row_padding() * 2.0);
                }
                crate::frame::DrillStage::Compared => {
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

                    let row1 = format!("🔴 你的表达: {}", user_expr);
                    let row2 = format!("🟢 地道表达: {}", natural_expr);
                    let row3 = format!("💡 关键建议: {}", advice);
                    let btns = "[🔄 再试一次 (Tab)]   [⭐ 保存 (Ctrl+S)]   [完成 (Esc)]";

                    let r1_w = self.measure(&row1, &style).width;
                    let r2_w = self.measure(&row2, &style).width;
                    let r3_w = self.measure(&row3, &small_style).width;
                    let btns_w = self.measure(btns, &m.index_style()).width;

                    width = width.max(r1_w).max(r2_w).max(r3_w).max(btns_w) + m.column_gap() * 2.0;
                    height += (line_height + m.row_padding()) * 3.0 + small_style.line_height + m.row_padding() * 2.0;
                    if card.drill_notice.is_some() {
                        height += small_style.line_height + m.row_padding();
                    }
                    return (width, height);
                }
            }
        }

        // 1. Layer 3: 主条目 (Flag + 状态/主要文本 + 展开按钮)
        let flag_prefix = "🇺🇸 ";
        let flag_width = self.measure(flag_prefix, &style).width;
        let main_text = match card.status {
            LearningStatus::Loading => "正在推荐地道口语...",
            LearningStatus::Failed => "暂无地道口语推荐",
            LearningStatus::Ready => {
                if let Some(prim) = &card.primary {
                    &prim.text
                } else if let Some(first) = card.alternatives.first() {
                    &first.text
                } else {
                    ""
                }
            }
        };
        let main_width = self.measure(main_text, &style).width;
        let drill_btn = "[Try English]";
        let drill_width = self.measure(drill_btn, &m.index_style()).width;
        let btn_text = if card.expanded { "收起 ▴" } else { "更多译法 ▾" };
        let btn_width = self.measure(btn_text, &m.index_style()).width;

        let row1_width = flag_width + main_width + m.column_gap() * 2.0 + drill_width + m.column_gap() + btn_width;
        width = width.max(row1_width);
        height += line_height + m.row_padding() * 2.0;

        // 2. Layer 4: 渐进展开列表 (2~5 个备选表达 + 标签)
        if card.expanded && !card.alternatives.is_empty() {
            height += m.px(4.0); // 分隔留白
            for alt in &card.alternatives {
                let tag_str = format!("[{}] ", alt.tag);
                let tag_w = self.measure(&tag_str, &tag_style).width;
                let text_w = self.measure(&alt.text, &style).width;
                width = width.max(tag_w + text_w + m.column_gap());
                height += line_height + m.row_padding();
            }

            // 3. 选中表达的详览卡片 (含义、语气、场景、频度、地道解析)
            if let Some(sel_idx) = card.selected_index {
                if let Some(alt) = card.alternatives.get(sel_idx) {
                    let small_style = m.style(m.theme.index_font, m.theme.colors.pos);
                    let detail_lines = [
                        format!("含义: {}", alt.meaning),
                        format!("语气: {}", alt.tone),
                        format!("场景: {}", alt.scenario),
                        format!("频度: {}", alt.commonality),
                        format!("为什么地道: {}", alt.why_natural),
                    ];
                    height += m.px(6.0); // 详情卡片顶部间距
                    for line in &detail_lines {
                        let lw = self.measure(line, &small_style).width;
                        width = width.max(lw + m.column_gap() * 2.0);
                        height += small_style.line_height + m.px(2.0);
                    }
                    height += m.px(6.0); // 详情卡片底部留白
                }
            }
        }

        (width, height)
    }

    /// 绘制轻量英语学习卡片。
    pub(super) fn draw_learning_card(
        &mut self,
        canvas: &mut Canvas,
        frame: &Frame,
        m: &Metrics,
        left: f32,
        mut y: f32,
        content_width: f32,
    ) {
        let Some(card) = &frame.learning_card else {
            return;
        };
        let style = m.annotation_style(m.theme.colors.gloss);
        let tag_style = m.style(m.theme.index_font, m.theme.colors.fresh);
        let active_text_style = m.annotation_style(m.theme.colors.text);
        let small_style = m.style(m.theme.index_font, m.theme.colors.pos);
        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));

        // 柔和顶部分隔细线，与第二层中文候选自然解耦
        let sep_y = y + m.row_padding() / 2.0;
        canvas.fill_rect(
            left + m.padding(),
            sep_y,
            content_width - m.padding() * 2.0,
            m.px(1.0).max(1.0),
            Color::rgba(128, 128, 128, 38),
        );
        y += m.row_padding();

        // Active Recall 特殊阶段绘制
        if let Some(stage) = card.drill_stage {
            match stage {
                crate::frame::DrillStage::Input => {
                    let title = format!("🎯 英文回忆练习 (中文: {})", card.target_text);
                    self.draw_text(canvas, &title, &tag_style, left + m.padding(), y);
                    y += line_height + m.row_padding() / 2.0;

                    if let Some(notice) = &card.drill_notice {
                        let notice_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                        self.draw_text(canvas, notice, &notice_style, left + m.padding(), y);
                        y += small_style.line_height + m.row_padding() / 2.0;
                    }

                    // 输入框背景
                    canvas.fill_round_rect(
                        left + m.padding(),
                        y,
                        content_width - m.padding() * 2.0,
                        line_height + m.px(6.0),
                        m.px(4.0),
                        m.theme.colors.highlight,
                    );
                    let input_text = format!("你的尝试: {}|", card.drill_input);
                    self.draw_text(
                        canvas,
                        &input_text,
                        &active_text_style,
                        left + m.padding() + m.px(4.0),
                        y + m.px(2.0),
                    );
                    y += line_height + m.px(8.0) + m.row_padding() / 2.0;

                    // 底部按钮
                    let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                    let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                    self.draw_text(canvas, btns, &btn_style, left + m.padding(), y);
                    return;
                }
                crate::frame::DrillStage::Comparing => {
                    self.draw_text(
                        canvas,
                        "⏳ 正在分析对比表达中...",
                        &tag_style,
                        left + m.padding(),
                        y,
                    );
                    return;
                }
                crate::frame::DrillStage::Compared => {
                    if let Some(notice) = &card.drill_notice {
                        let notice_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                        self.draw_text(canvas, notice, &notice_style, left + m.padding(), y);
                        y += small_style.line_height + m.row_padding() / 2.0;
                    }

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

                    let r1 = format!("🔴 你的表达: {}", user_expr);
                    self.draw_text(canvas, &r1, &active_text_style, left + m.padding(), y);
                    y += line_height + m.row_padding() / 2.0;

                    let r2 = format!("🟢 地道表达: {}", natural_expr);
                    self.draw_text(canvas, &r2, &tag_style, left + m.padding(), y);
                    y += line_height + m.row_padding() / 2.0;

                    let r3 = format!("💡 关键建议: {}", advice);
                    self.draw_text(canvas, &r3, &small_style, left + m.padding(), y);
                    y += small_style.line_height + m.row_padding();

                    let btns = "[🔄 再试一次 (Tab)]   [⭐ 保存 (Ctrl+S)]   [完成 (Esc)]";
                    let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                    self.draw_text(canvas, btns, &btn_style, left + m.padding(), y);
                    return;
                }
            }
        }

        // 1. Layer 3: 主条目 (Flag + 最佳推荐 + 展开按钮)
        let flag_prefix = "🇺🇸 ";
        let flag_style = m.annotation_style(m.theme.colors.fresh);
        let mut x = left + m.padding();
        x += self.draw_text(canvas, flag_prefix, &flag_style, x, y);

        let main_text = match card.status {
            LearningStatus::Loading => "正在推荐地道口语...",
            LearningStatus::Failed => "暂无地道口语推荐",
            LearningStatus::Ready => {
                if let Some(prim) = &card.primary {
                    &prim.text
                } else if let Some(first) = card.alternatives.first() {
                    &first.text
                } else {
                    ""
                }
            }
        };
        let text_color_style = if card.status == LearningStatus::Ready {
            &active_text_style
        } else {
            &style
        };
        self.draw_text(canvas, main_text, text_color_style, x, y);

        // [Try English] 与「更多译法 ▾ / 收起 ▴」
        let drill_btn = "[Try English]";
        let drill_style = m.style(m.theme.index_font, m.theme.colors.fresh);
        let drill_size = self.measure(drill_btn, &drill_style);

        let btn_text = if card.expanded { "收起 ▴" } else { "更多译法 ▾" };
        let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
        let btn_size = self.measure(btn_text, &btn_style);

        let btn_x = left + content_width - m.padding() - btn_size.width;
        let drill_x = btn_x - m.column_gap() - drill_size.width;

        self.draw_text(canvas, drill_btn, &drill_style, drill_x, y);
        self.draw_text(canvas, btn_text, &btn_style, btn_x, y);

        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));
        y += line_height + m.row_padding();

        // 2. Layer 4: 备选列表
        if card.expanded && !card.alternatives.is_empty() {
            for (idx, alt) in card.alternatives.iter().enumerate() {
                let is_selected = card.selected_index == Some(idx);
                let alt_y = y;
                let row_h = line_height + m.row_padding();

                if is_selected {
                    // 高亮选中条目底色（柔和浅色背景）
                    canvas.fill_round_rect(
                        left + m.padding() / 2.0,
                        alt_y - m.row_padding() / 4.0,
                        content_width - m.padding(),
                        row_h,
                        m.px(4.0),
                        m.theme.colors.highlight,
                    );
                }

                let mut ax = left + m.padding() + m.px(4.0);
                let tag_str = format!("[{}] ", alt.tag);
                ax += self.draw_text(canvas, &tag_str, &tag_style, ax, alt_y);
                self.draw_text(canvas, &alt.text, &active_text_style, ax, alt_y);

                y += row_h;
            }

            // 3. 选中表达的详览卡片
            if let Some(sel_idx) = card.selected_index {
                if let Some(alt) = card.alternatives.get(sel_idx) {
                    let small_style = m.style(m.theme.index_font, m.theme.colors.pos);
                    let card_box_y = y + m.px(2.0);
                    let detail_lines = [
                        format!("含义: {}", alt.meaning),
                        format!("语气: {}", alt.tone),
                        format!("场景: {}", alt.scenario),
                        format!("频度: {}", alt.commonality),
                        format!("为什么地道: {}", alt.why_natural),
                    ];
                    let card_h = (small_style.line_height + m.px(2.0)) * 5.0 + m.px(8.0);

                    // 卡片背景底色
                    canvas.fill_round_rect(
                        left + m.padding() / 2.0,
                        card_box_y,
                        content_width - m.padding(),
                        card_h,
                        m.px(6.0),
                        Color::rgba(128, 128, 128, 20),
                    );

                    let mut dy = card_box_y + m.px(4.0);
                    for line in &detail_lines {
                        self.draw_text(canvas, line, &small_style, left + m.padding() + m.px(6.0), dy);
                        dy += small_style.line_height + m.px(2.0);
                    }
                }
            }
        }
    }
}
