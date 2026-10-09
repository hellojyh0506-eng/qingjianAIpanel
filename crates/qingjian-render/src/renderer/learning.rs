//! 轻量英语学习卡片（候选窗口第三层）的尺寸计算与绘制。

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
        let style = m.annotation_style(m.theme.colors.gloss);
        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));

        let flag_width = m.px(22.0);
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

        let is_learn = frame.learning_mode == crate::frame::LearningMode::Learn;

        let width = if is_learn {
            let btn_text = "📖 学习助手(Ctrl+Alt+E)";
            let btn_width = self.measure(btn_text, &m.index_style()).width + m.px(16.0);
            flag_width + main_width + m.column_gap() * 3.0 + btn_width
        } else {
            flag_width + main_width + m.column_gap() * 2.0
        };

        let height = line_height + m.row_padding() * 2.0;
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
        let active_text_style = m.annotation_style(m.theme.colors.text);
        let line_height = style.line_height.max(m.px(m.theme.annotation_font.line_height));

        // 顶部分隔细线
        let sep_y = y + m.row_padding() / 2.0;
        canvas.fill_rect(
            left + m.padding(),
            sep_y,
            content_width - m.padding() * 2.0,
            m.px(1.0).max(1.0),
            Color::rgba(128, 128, 128, 38),
        );
        y += m.row_padding();

        let mut x = left + m.padding();

        let is_learn = frame.learning_mode == crate::frame::LearningMode::Learn;

        // 🇺🇸 国旗前缀：矢量绘制精致美国国旗微标，彻底解决 Windows 无 Emoji 国旗导致退化成 "us" 的问题
        let flag_w = m.px(16.0);
        let flag_h = m.px(11.0);
        let flag_y = y + (line_height - flag_h) / 2.0;
        // 红白底纹
        canvas.fill_round_rect(x, flag_y, flag_w, flag_h, m.px(1.5), Color::rgb(179, 25, 66));
        canvas.fill_rect(x, flag_y + flag_h * 2.0 / 7.0, flag_w, flag_h / 7.0, Color::rgb(255, 255, 255));
        canvas.fill_rect(x, flag_y + flag_h * 4.0 / 7.0, flag_w, flag_h / 7.0, Color::rgb(255, 255, 255));
        // 蓝底
        canvas.fill_rect(x, flag_y, flag_w * 0.45, flag_h * 4.0 / 7.0, Color::rgb(10, 49, 97));
        x += flag_w + m.px(6.0);

        // 主表达
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

        if is_learn {
            let btn_text = "📖 学习助手(Ctrl+Alt+E)";
            let btn_style = m.style(m.theme.index_font, m.theme.colors.gloss);
            let btn_w = self.measure(btn_text, &btn_style).width + m.px(16.0);
            let btn_x = left + content_width - m.padding() - btn_w;
            let btn_h = line_height;

            canvas.fill_round_rect(btn_x, y - m.px(1.0), btn_w, btn_h, m.px(4.0), m.theme.colors.highlight);
            self.draw_text(canvas, btn_text, &btn_style, btn_x + m.px(8.0), y);
        }
    }
}
