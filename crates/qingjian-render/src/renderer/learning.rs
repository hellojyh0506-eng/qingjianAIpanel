//! 轻量英语学习卡片（候选窗口第三层与渐进展开层）的尺寸计算与绘制。

use super::Metrics;
use super::Renderer;
use crate::canvas::Canvas;
use crate::color::Color;
use crate::frame::{Frame, LearningStatus};

impl Renderer {
    /// 计算轻量英语学习卡片需要的尺寸（若不存在卡片返回 0, 0）。
    pub(super) fn learning_card_size(&mut self, frame: &Frame, m: &Metrics) -> (f32, f32) {
        if frame.learning_card.is_none() {
            if frame.learning_mode == crate::frame::LearningMode::Daily && !frame.rows.is_empty() {
                let text_style = m.style(m.theme.index_font, m.theme.colors.gloss);
                let w = m.px(28.0) + m.px(6.0) + self.measure("按 Ctrl+Shift+E 查看英文", &text_style).width + m.padding() * 2.0;
                let h = text_style.line_height + m.row_padding();
                return (w, h);
            }
            return (0.0, 0.0);
        }
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
                    let title = format!("💡 请用英语表达：{}", card.target_text);
                    let title_w = self.measure(&title, &style).width;
                    let placeholder = "在这里输入你的英文表达...";
                    let input_str = if card.drill_input.is_empty() {
                        format!("{}|", placeholder)
                    } else {
                        format!("{}|", card.drill_input)
                    };
                    let input_w = self.measure(&input_str, &m.annotation_style(m.theme.colors.text)).width + m.px(28.0);
                    let btns_w = m.px(190.0);

                    width = width.max(title_w).max(input_w).max(btns_w).max(m.px(280.0)) + m.column_gap() * 2.0;
                    height += (line_height + m.row_padding()) * 3.5;
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

                    let r1 = format!("❌ 你的表达: {}", user_expr);
                    let r2 = format!("✅ 更自然的表达: {} 🔊", natural_expr);
                    let btns_w = m.px(320.0);

                    let r1_w = self.measure(&r1, &style).width;
                    let r2_w = self.measure(&r2, &style).width;
                    let advice_w = self.measure(&format!("ⓘ 关键建议: {}", advice), &small_style).width;

                    width = width.max(r1_w + r2_w + m.column_gap() * 2.0).max(btns_w).max(advice_w).max(m.px(330.0));
                    // 两卡片高度 + 关键建议高度 + 按钮高度 + 留白
                    height += (line_height * 2.0 + m.px(14.0)) + (small_style.line_height * 2.0 + m.px(8.0)) + (line_height + m.px(4.0)) + m.row_padding() * 2.5;

                    if card.drill_notice.is_some() {
                        height += small_style.line_height + m.row_padding();
                    }
                    return (width, height);
                }
                crate::frame::DrillStage::Profile => {
                    let title = "📊 个人英语学习画像 (B2 → C1)";
                    let title_w = self.measure(title, &style).width;
                    let btns = "[🗑 清空画像数据]   [返回 (Esc)]";
                    let btns_w = self.measure(btns, &m.index_style()).width;
                    width = width.max(title_w).max(btns_w);
                    height += (line_height + m.row_padding()) * 2.0;

                    if let Some(text) = &card.drill_notice {
                        for line in text.lines() {
                            let lw = self.measure(line, &small_style).width;
                            width = width.max(lw);
                            height += small_style.line_height + m.row_padding() / 2.0;
                        }
                    } else {
                        height += (small_style.line_height + m.row_padding() / 2.0) * 8.0;
                    }

                    width += m.column_gap() * 2.0;
                    height += m.row_padding() * 2.0;
                    return (width, height);
                }
                crate::frame::DrillStage::Review => {
                    let total = card.review_total_count.max(1);
                    let current = card.review_current_index.max(1);
                    let title = format!("📚 今日待复习 ({}/{}) [中文: {}]", current, total, card.target_text);
                    let title_w = self.measure(&title, &style).width;

                    if card.drill_attempt.is_none() {
                        let input_str = format!("你的尝试: {}|", card.drill_input);
                        let input_w = self.measure(&input_str, &m.annotation_style(m.theme.colors.text)).width;
                        let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                        let btns_w = self.measure(btns, &m.index_style()).width;
                        width = width.max(title_w).max(input_w).max(btns_w) + m.column_gap() * 2.0;
                        height += (line_height + m.row_padding()) * 3.0;
                    } else {
                        let user_expr = card.drill_attempt.as_ref().map(|a| a.user_attempt.as_str()).unwrap_or(card.drill_input.as_str());
                        let natural_expr = card.drill_attempt.as_ref().map(|a| a.comparison.natural_expression.as_str()).unwrap_or("");
                        let r1 = format!("🔴 你的表达: {}", user_expr);
                        let r2 = format!("🟢 地道表达: {}", natural_expr);
                        let r1_w = self.measure(&r1, &style).width;
                        let r2_w = self.measure(&r2, &style).width;
                        let btns = "[1. Again (重来)]   [2. Good (掌握)]   [3. Easy (轻松)]   [退出 (Esc)]";
                        let btns_w = self.measure(btns, &m.index_style()).width;
                        width = width.max(title_w).max(r1_w).max(r2_w).max(btns_w) + m.column_gap() * 2.0;
                        height += (line_height + m.row_padding()) * 3.5;
                    }
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
        let (main_text, has_audio) = match card.status {
            LearningStatus::Loading => ("正在推荐地道口语……", false),
            LearningStatus::Failed => ("暂时没有译文", false),
            LearningStatus::Ready => {
                if let Some(prim) = &card.primary {
                    (prim.text.as_str(), true)
                } else if let Some(first) = card.alternatives.first() {
                    (first.text.as_str(), true)
                } else {
                    ("暂时没有译文", false)
                }
            }
        };
        let audio_width = if has_audio {
            self.measure(" 🔊", &style).width
        } else {
            0.0
        };
        let main_width = self.measure(main_text, &style).width + audio_width;
        let drill_btn = "Try English";
        let drill_width = self.measure(drill_btn, &m.index_style()).width + m.px(16.0);
        let btn_text = if card.expanded { "收起 ∧" } else { "更多译法 ∨" };
        let btn_width = self.measure(btn_text, &m.index_style()).width + m.px(16.0);

        let row1_width = flag_width
            + main_width
            + m.column_gap() * 3.0
            + drill_width
            + m.column_gap()
            + btn_width;
        width = width.max(row1_width);
        height += line_height + m.row_padding() * 2.0;

        // 2. Layer 4: 渐进展开列表 (更多译法: 1–5 条紧凑条目)
        if card.expanded && !card.alternatives.is_empty() {
            height += m.px(4.0); // 分隔留白
            for (idx, alt) in card.alternatives.iter().take(5).enumerate() {
                let idx_str = format!("{}. ", idx + 1);
                let idx_w = self.measure(&idx_str, &m.index_style()).width;
                let text_w = self.measure(&alt.text, &style).width;
                let tag_raw = if alt.tag.is_empty() { "Natural" } else { alt.tag.as_str() };
                let tag_w = self.measure(tag_raw, &tag_style).width + m.px(12.0);
                let meaning_str = if alt.meaning.is_empty() {
                    String::new()
                } else {
                    format!("› {}", alt.meaning)
                };
                let note_w = if meaning_str.is_empty() {
                    0.0
                } else {
                    self.measure(&meaning_str, &small_style).width + m.column_gap()
                };
                let audio_w = self.measure(" 🔊", &style).width;
                let item_w = m.padding()
                    + idx_w
                    + text_w
                    + m.column_gap()
                    + tag_w
                    + note_w
                    + m.column_gap()
                    + audio_w
                    + m.padding();
                width = width.max(item_w);
                height += line_height + m.row_padding();
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
        if frame.learning_mode == crate::frame::LearningMode::Daily {
            if !frame.rows.is_empty() {
                let badge_style = m.style(m.theme.index_font, Color::rgb(255, 255, 255));
                let prompt_style = m.style(m.theme.index_font, m.theme.colors.gloss);
                let bx = left + m.padding();
                let by = y + m.row_padding() / 2.0;
                let badge_w = m.px(20.0);
                let badge_h = m.px(13.0);
                canvas.fill_round_rect(bx, by + m.px(1.0), badge_w, badge_h, m.px(3.0), Color::rgb(249, 115, 22));
                self.draw_text(canvas, "us", &badge_style, bx + m.px(4.0), by);
                self.draw_text(canvas, "按 Ctrl+Shift+E 查看英文", &prompt_style, bx + badge_w + m.px(6.0), by);
            }
            return;
        }
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
                    let title = format!("💡 请用英语表达：{}", card.target_text);
                    self.draw_text(canvas, &title, &active_text_style, left + m.padding(), y);
                    y += line_height + m.row_padding();

                    let box_x = left + m.padding();
                    let box_w = content_width - m.padding() * 2.0;
                    let box_h = line_height + m.px(10.0);
                    let border_color = Color::rgba(128, 128, 128, 60);
                    let bg_color = Color::rgba(255, 255, 255, 255);
                    canvas.fill_round_rect(box_x, y, box_w, box_h, m.px(4.0), border_color);
                    canvas.fill_round_rect(box_x + 1.0, y + 1.0, box_w - 2.0, box_h - 2.0, m.px(3.5), bg_color);

                    let placeholder_style = m.style(m.theme.index_font, m.theme.colors.gloss);
                    if card.drill_input.is_empty() {
                        self.draw_text(
                            canvas,
                            "在这里输入你的英文表达...",
                            &placeholder_style,
                            box_x + m.px(8.0),
                            y + m.px(4.0),
                        );
                    } else {
                        let text_with_caret = format!("{}|", card.drill_input);
                        self.draw_text(
                            canvas,
                            &text_with_caret,
                            &active_text_style,
                            box_x + m.px(8.0),
                            y + m.px(4.0),
                        );
                    }
                    y += box_h + m.row_padding();

                    // 底部按钮：[对比 (Enter)] (青色实心) 与 [显示答案] (灰色边框)
                    let btn_enter_text = "对比 (Enter)";
                    let btn_enter_style = m.style(m.theme.index_font, Color::rgb(255, 255, 255));
                    let btn_enter_w = self.measure(btn_enter_text, &btn_enter_style).width + m.px(20.0);
                    let btn_h = line_height + m.px(2.0);

                    canvas.fill_round_rect(box_x, y, btn_enter_w, btn_h, m.px(4.0), Color::rgb(0, 166, 153));
                    self.draw_text(canvas, btn_enter_text, &btn_enter_style, box_x + m.px(10.0), y + m.px(2.0));

                    let btn_show_x = box_x + btn_enter_w + m.column_gap();
                    let btn_show_text = "显示答案";
                    let btn_show_style = m.style(m.theme.index_font, m.theme.colors.text);
                    let btn_show_w = self.measure(btn_show_text, &btn_show_style).width + m.px(20.0);
                    canvas.fill_round_rect(btn_show_x, y, btn_show_w, btn_h, m.px(4.0), Color::rgba(128, 128, 128, 40));
                    canvas.fill_round_rect(btn_show_x + 1.0, y + 1.0, btn_show_w - 2.0, btn_h - 2.0, m.px(3.5), Color::rgba(250, 250, 250, 255));
                    self.draw_text(canvas, btn_show_text, &btn_show_style, btn_show_x + m.px(10.0), y + m.px(2.0));

                    if let Some(notice) = &card.drill_notice {
                        y += btn_h + m.row_padding() / 2.0;
                        let notice_style = m.style(m.theme.index_font, Color::rgb(250, 140, 22));
                        self.draw_text(canvas, notice, &notice_style, box_x, y);
                    }
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

                    let total_w = content_width - m.padding() * 2.0;
                    let card_gap = m.column_gap();
                    let card_w = (total_w - card_gap) / 2.0;
                    let card_h = line_height * 2.0 + m.px(12.0);

                    let left_card_x = left + m.padding();
                    let right_card_x = left_card_x + card_w + card_gap;

                    // 1. 左卡片：你的表达 (淡红底 #FFF1F0 + 浅红边 #FFCCC7)
                    canvas.fill_round_rect(left_card_x, y, card_w, card_h, m.px(4.0), Color::rgb(255, 204, 199));
                    canvas.fill_round_rect(left_card_x + 1.0, y + 1.0, card_w - 2.0, card_h - 2.0, m.px(3.5), Color::rgb(255, 241, 240));
                    let red_title_style = m.style(m.theme.index_font, Color::rgb(207, 19, 34));
                    self.draw_text(canvas, "❌ 你的表达", &red_title_style, left_card_x + m.px(8.0), y + m.px(4.0));
                    self.draw_text(canvas, user_expr, &active_text_style, left_card_x + m.px(8.0), y + line_height + m.px(4.0));

                    // 2. 右卡片：更自然的表达 (淡绿底 #F6FFED + 浅绿边 #B7EB8F)
                    canvas.fill_round_rect(right_card_x, y, card_w, card_h, m.px(4.0), Color::rgb(183, 235, 143));
                    canvas.fill_round_rect(right_card_x + 1.0, y + 1.0, card_w - 2.0, card_h - 2.0, m.px(3.5), Color::rgb(246, 255, 237));
                    let green_title_style = m.style(m.theme.index_font, Color::rgb(56, 158, 13));
                    self.draw_text(canvas, "✅ 更自然的表达", &green_title_style, right_card_x + m.px(8.0), y + m.px(4.0));
                    let nat_display = format!("{} 🔊", natural_expr);
                    self.draw_text(canvas, &nat_display, &active_text_style, right_card_x + m.px(8.0), y + line_height + m.px(4.0));

                    y += card_h + m.row_padding();

                    // 3. 关键建议
                    let tip_title_style = m.style(m.theme.index_font, Color::rgb(24, 144, 255));
                    self.draw_text(canvas, "ⓘ 关键建议", &tip_title_style, left_card_x, y);
                    y += small_style.line_height + m.px(2.0);

                    let advice_text = if !advice.is_empty() {
                        advice
                    } else {
                        "表达地道，用词准确。"
                    };
                    self.draw_text(canvas, advice_text, &small_style, left_card_x, y);
                    y += small_style.line_height + m.row_padding();

                    // 4. 底部按钮：[🔄 再试一次 (R)]  [⭐ 保存到表达本 (S)]  [完成 (Enter)]
                    let btn_h = line_height + m.px(2.0);
                    let mut bx = left_card_x;

                    let retry_text = "🔄 再试一次 (R)";
                    let retry_w = self.measure(retry_text, &small_style).width + m.px(16.0);
                    canvas.fill_round_rect(bx, y, retry_w, btn_h, m.px(4.0), Color::rgba(128, 128, 128, 40));
                    canvas.fill_round_rect(bx + 1.0, y + 1.0, retry_w - 2.0, btn_h - 2.0, m.px(3.5), Color::rgba(250, 250, 250, 255));
                    self.draw_text(canvas, retry_text, &small_style, bx + m.px(8.0), y + m.px(2.0));
                    bx += retry_w + m.column_gap();

                    let save_text = "⭐ 保存到表达本 (S)";
                    let save_w = self.measure(save_text, &small_style).width + m.px(16.0);
                    canvas.fill_round_rect(bx, y, save_w, btn_h, m.px(4.0), Color::rgba(128, 128, 128, 40));
                    canvas.fill_round_rect(bx + 1.0, y + 1.0, save_w - 2.0, btn_h - 2.0, m.px(3.5), Color::rgba(250, 250, 250, 255));
                    self.draw_text(canvas, save_text, &small_style, bx + m.px(8.0), y + m.px(2.0));
                    bx += save_w + m.column_gap();

                    let done_text = "完成 (Enter)";
                    let done_style = m.style(m.theme.index_font, Color::rgb(255, 255, 255));
                    let done_w = self.measure(done_text, &done_style).width + m.px(16.0);
                    canvas.fill_round_rect(bx, y, done_w, btn_h, m.px(4.0), Color::rgb(0, 166, 153));
                    self.draw_text(canvas, done_text, &done_style, bx + m.px(8.0), y + m.px(2.0));

                    if let Some(notice) = &card.drill_notice {
                        y += btn_h + m.row_padding() / 2.0;
                        let notice_style = m.style(m.theme.index_font, Color::rgb(250, 140, 22));
                        self.draw_text(canvas, notice, &notice_style, left_card_x, y);
                    }
                    return;
                }
                crate::frame::DrillStage::Profile => {
                    let title = "📊 个人英语学习画像 (B2 → C1)";
                    self.draw_text(canvas, title, &tag_style, left + m.padding(), y);
                    y += line_height + m.row_padding() / 2.0;

                    if let Some(text) = &card.drill_notice {
                        for line in text.lines() {
                            self.draw_text(canvas, line, &small_style, left + m.padding(), y);
                            y += small_style.line_height + m.row_padding() / 2.0;
                        }
                    }

                    let btns = "[🗑 清空画像数据]   [返回 (Esc)]";
                    let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                    self.draw_text(canvas, btns, &btn_style, left + m.padding(), y + m.row_padding() / 2.0);
                    return;
                }
                crate::frame::DrillStage::Review => {
                    let total = card.review_total_count.max(1);
                    let current = card.review_current_index.max(1);
                    let title = format!("📚 今日待复习 ({}/{}) [中文: {}]", current, total, card.target_text);
                    self.draw_text(canvas, &title, &tag_style, left + m.padding(), y);
                    y += line_height + m.row_padding() / 2.0;

                    if let Some(notice) = &card.drill_notice {
                        let notice_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                        self.draw_text(canvas, notice, &notice_style, left + m.padding(), y);
                        y += small_style.line_height + m.row_padding() / 2.0;
                    }

                    if card.drill_attempt.is_none() {
                        // 输入回忆阶段
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

                        let btns = "[对比 (Enter)]   [看答案 (Tab)]   [退出 (Esc)]";
                        let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                        self.draw_text(canvas, btns, &btn_style, left + m.padding(), y);
                    } else {
                        // 评级推进阶段
                        let user_expr = card.drill_attempt.as_ref().map(|a| a.user_attempt.as_str()).unwrap_or(card.drill_input.as_str());
                        let natural_expr = card.drill_attempt.as_ref().map(|a| a.comparison.natural_expression.as_str()).unwrap_or("");
                        let r1 = format!("🔴 你的表达: {}", user_expr);
                        self.draw_text(canvas, &r1, &active_text_style, left + m.padding(), y);
                        y += line_height + m.row_padding() / 2.0;

                        let r2 = format!("🟢 地道表达: {}", natural_expr);
                        self.draw_text(canvas, &r2, &tag_style, left + m.padding(), y);
                        y += line_height + m.row_padding() / 2.0;

                        let btns = "[1. Again (重来)]   [2. Good (掌握)]   [3. Easy (轻松)]   [退出 (Esc)]";
                        let btn_style = m.style(m.theme.index_font, m.theme.colors.fresh);
                        self.draw_text(canvas, btns, &btn_style, left + m.padding(), y);
                    }
                    return;
                }
            }
        }

        // 1. Layer 3: 主条目 (Flag + 最佳推荐 + 展开按钮)
        let flag_prefix = "🇺🇸 ";
        let flag_style = m.annotation_style(m.theme.colors.fresh);
        let mut x = left + m.padding();
        x += self.draw_text(canvas, flag_prefix, &flag_style, x, y);

        let (main_text, has_audio) = match card.status {
            LearningStatus::Loading => ("正在推荐地道口语……", false),
            LearningStatus::Failed => ("暂时没有译文", false),
            LearningStatus::Ready => {
                if let Some(prim) = &card.primary {
                    (prim.text.as_str(), true)
                } else if let Some(first) = card.alternatives.first() {
                    (first.text.as_str(), true)
                } else {
                    ("暂时没有译文", false)
                }
            }
        };
        let text_color_style = if card.status == LearningStatus::Ready {
            &active_text_style
        } else {
            &style
        };
        x += self.draw_text(canvas, main_text, text_color_style, x, y);

        if has_audio {
            let audio_style = m.style(m.theme.index_font, m.theme.colors.cloud);
            self.draw_text(canvas, " 🔊", &audio_style, x, y);
        }

        // 右侧仅保留两个按钮：[Try English] 与「更多译法 ∨ / 收起 ∧」
        let drill_text = "Try English";
        let drill_style = m.style(m.theme.index_font, Color::rgb(0, 166, 153));
        let drill_w = self.measure(drill_text, &drill_style).width + m.px(12.0);
        let btn_h = line_height;

        let btn_text = if card.expanded { "收起 ∧" } else { "更多译法 ∨" };
        let btn_style = m.style(m.theme.index_font, m.theme.colors.cloud);
        let btn_w = self.measure(btn_text, &btn_style).width + m.px(12.0);

        let btn_x = left + content_width - m.padding() - btn_w;
        let drill_x = btn_x - m.column_gap() - drill_w;

        // Try English button
        canvas.fill_round_rect(drill_x, y - m.px(1.0), drill_w, btn_h, m.px(4.0), Color::rgba(0, 166, 153, 30));
        self.draw_text(canvas, drill_text, &drill_style, drill_x + m.px(6.0), y);

        // 更多译法 button
        canvas.fill_round_rect(btn_x, y - m.px(1.0), btn_w, btn_h, m.px(4.0), Color::rgba(128, 128, 128, 20));
        self.draw_text(canvas, btn_text, &btn_style, btn_x + m.px(6.0), y);

        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));
        y += line_height + m.row_padding();

        // 2. Layer 4: 渐进展开列表 (更多译法: 1–5 条紧凑条目)
        if card.expanded && !card.alternatives.is_empty() {
            canvas.fill_rect(
                left + m.padding(),
                y - m.row_padding() / 2.0,
                content_width - m.padding() * 2.0,
                1.0,
                Color::rgba(128, 128, 128, 30),
            );

            for (idx, alt) in card.alternatives.iter().take(5).enumerate() {
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

                // Index: e.g. "1. "
                let idx_str = format!("{}. ", idx + 1);
                let idx_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                ax += self.draw_text(canvas, &idx_str, &idx_style, ax, alt_y);

                // English text
                ax += self.draw_text(canvas, &alt.text, &active_text_style, ax, alt_y);
                ax += m.column_gap();

                // Tag pill
                let tag_raw = if alt.tag.is_empty() { "Natural" } else { alt.tag.as_str() };
                let (tag_bg, tag_fg) = match tag_raw {
                    "Natural" => (Color::rgba(82, 196, 26, 30), Color::rgb(56, 158, 13)),
                    "Casual" => (Color::rgba(24, 144, 255, 30), Color::rgb(9, 109, 217)),
                    "British" => (Color::rgba(114, 46, 209, 30), Color::rgb(83, 29, 171)),
                    _ => (Color::rgba(128, 128, 128, 30), Color::rgb(100, 100, 100)),
                };
                let tag_font_style = m.style(m.theme.index_font, tag_fg);
                let tag_w = self.measure(tag_raw, &tag_font_style).width + m.px(8.0);
                let tag_h = small_style.line_height + m.px(2.0);
                canvas.fill_round_rect(ax, alt_y + m.px(1.0), tag_w, tag_h, m.px(3.0), tag_bg);
                self.draw_text(canvas, tag_raw, &tag_font_style, ax + m.px(4.0), alt_y + m.px(1.0));
                ax += tag_w + m.column_gap();

                // Meaning
                if !alt.meaning.is_empty() {
                    let meaning_str = format!("› {}", alt.meaning);
                    ax += self.draw_text(canvas, &meaning_str, &small_style, ax, alt_y);
                    ax += m.column_gap();
                }

                // Audio icon
                let audio_style = m.style(m.theme.index_font, m.theme.colors.cloud);
                self.draw_text(canvas, "🔊", &audio_style, ax, alt_y);

                y += row_h;
            }
        }
    }
}
