//! GDI Markdown 文本排版与渲染组件
//! 支持：标题（#、##、###）、引用（>）、无序列表（-、*）、有序列表（1.）、行内代码（`）、粗体（**）、代码块（```）

use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{GetTextExtentPoint32W, SelectObject, SetTextColor, TextOutW, HDC, HFONT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineSpan {
    Normal(String),
    Bold(String),
    Code(String),
}

#[derive(Debug, Clone)]
pub enum MarkdownBlock {
    Header { level: u8, text: String },
    BulletItem { text: Vec<InlineSpan> },
    NumberedItem { number: usize, text: Vec<InlineSpan> },
    Quote { text: Vec<InlineSpan> },
    CodeBlock { lang: String, lines: Vec<String> },
    Paragraph { spans: Vec<InlineSpan> },
}

pub fn parse_inline_spans(text: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut cur = String::new();

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
            if !cur.is_empty() {
                spans.push(InlineSpan::Normal(cur.clone()));
                cur.clear();
            }
            let mut bold_text = String::new();
            let mut closed = false;
            while let Some(bch) = chars.next() {
                if bch == '*' && chars.peek() == Some(&'*') {
                    chars.next();
                    closed = true;
                    break;
                }
                bold_text.push(bch);
            }
            if closed {
                spans.push(InlineSpan::Bold(bold_text));
            } else {
                cur.push_str("**");
                cur.push_str(&bold_text);
            }
        } else if ch == '`' {
            if !cur.is_empty() {
                spans.push(InlineSpan::Normal(cur.clone()));
                cur.clear();
            }
            let mut code_text = String::new();
            let mut closed = false;
            while let Some(cch) = chars.next() {
                if cch == '`' {
                    closed = true;
                    break;
                }
                code_text.push(cch);
            }
            if closed {
                spans.push(InlineSpan::Code(code_text));
            } else {
                cur.push('`');
                cur.push_str(&code_text);
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        spans.push(InlineSpan::Normal(cur));
    }
    spans
}

pub fn parse_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                blocks.push(MarkdownBlock::CodeBlock {
                    lang: code_lang.clone(),
                    lines: std::mem::take(&mut code_lines),
                });
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = trimmed.trim_start_matches('`').trim().to_string();
                code_lines.clear();
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("### ") {
            blocks.push(MarkdownBlock::Header {
                level: 3,
                text: trimmed[4..].trim().to_string(),
            });
        } else if trimmed.starts_with("## ") {
            blocks.push(MarkdownBlock::Header {
                level: 2,
                text: trimmed[3..].trim().to_string(),
            });
        } else if trimmed.starts_with("# ") {
            blocks.push(MarkdownBlock::Header {
                level: 1,
                text: trimmed[2..].trim().to_string(),
            });
        } else if trimmed.starts_with("> ") {
            blocks.push(MarkdownBlock::Quote {
                text: parse_inline_spans(trimmed[2..].trim()),
            });
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            blocks.push(MarkdownBlock::BulletItem {
                text: parse_inline_spans(trimmed[2..].trim()),
            });
        } else if let Some((num_str, rest)) = trimmed.split_once(". ") {
            if let Ok(num) = num_str.parse::<usize>() {
                blocks.push(MarkdownBlock::NumberedItem {
                    number: num,
                    text: parse_inline_spans(rest.trim()),
                });
                continue;
            }
            blocks.push(MarkdownBlock::Paragraph {
                spans: parse_inline_spans(trimmed),
            });
        } else {
            blocks.push(MarkdownBlock::Paragraph {
                spans: parse_inline_spans(trimmed),
            });
        }
    }

    if in_code_block {
        blocks.push(MarkdownBlock::CodeBlock {
            lang: code_lang,
            lines: code_lines,
        });
    }

    blocks
}

pub fn measure_text_width(hdc: HDC, font: HFONT, text: &str) -> i32 {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        SelectObject(hdc, font.into());
        let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
    }
    size.cx
}

pub fn draw_text_gdi_simple(hdc: HDC, font: HFONT, color: COLORREF, x: i32, y: i32, text: &str) -> i32 {
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

pub fn wrap_text_lines(hdc: HDC, font: HFONT, max_w: i32, text: &str) -> Vec<String> {
    unsafe { SelectObject(hdc, font.into()) };
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let p_trimmed = paragraph.trim_end();
        if p_trimmed.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut tokens: Vec<String> = Vec::new();
        let mut cur_word = String::new();

        for ch in p_trimmed.chars() {
            if ch.is_ascii_whitespace() {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(" ".to_string());
            } else if ch > '\u{2E80}' {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(ch.to_string());
            } else {
                cur_word.push(ch);
            }
        }
        if !cur_word.is_empty() {
            tokens.push(cur_word);
        }

        let mut current_line = String::new();
        for token in tokens {
            let mut test_line = current_line.clone();
            test_line.push_str(&token);
            let utf16: Vec<u16> = test_line.encode_utf16().collect();
            let mut size = SIZE::default();
            unsafe {
                let _ = GetTextExtentPoint32W(hdc, &utf16, &mut size);
            }

            if size.cx > max_w && !current_line.is_empty() {
                lines.push(current_line.trim_end().to_string());
                current_line = if token == " " { String::new() } else { token };
            } else {
                current_line = test_line;
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line.trim_end().to_string());
        }
    }
    lines
}

#[derive(Debug, Clone)]
pub struct FormattedLine {
    pub spans: Vec<InlineSpan>,
}

pub fn wrap_formatted_spans(
    hdc: HDC,
    font_normal: HFONT,
    font_bold: HFONT,
    font_code: HFONT,
    max_w: i32,
    spans: &[InlineSpan],
) -> Vec<FormattedLine> {
    let mut lines = Vec::new();
    let mut cur_spans = Vec::new();
    let mut cur_line_w = 0;

    for span in spans {
        let (font, is_code, text) = match span {
            InlineSpan::Normal(t) => (font_normal, false, t.as_str()),
            InlineSpan::Bold(t) => (font_bold, false, t.as_str()),
            InlineSpan::Code(t) => (font_code, true, t.as_str()),
        };

        let mut tokens = Vec::new();
        let mut cur_word = String::new();
        for ch in text.chars() {
            if ch.is_ascii_whitespace() {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(" ".to_string());
            } else if ch > '\u{2E80}' {
                if !cur_word.is_empty() {
                    tokens.push(cur_word.clone());
                    cur_word.clear();
                }
                tokens.push(ch.to_string());
            } else {
                cur_word.push(ch);
            }
        }
        if !cur_word.is_empty() {
            tokens.push(cur_word);
        }

        let mut token_accum = String::new();
        for tok in tokens {
            let tok_w = measure_text_width(hdc, font, &tok) + if is_code { 6 } else { 0 };
            if cur_line_w + tok_w > max_w && !cur_spans.is_empty() {
                if !token_accum.is_empty() {
                    cur_spans.push(match span {
                        InlineSpan::Normal(_) => InlineSpan::Normal(token_accum.clone()),
                        InlineSpan::Bold(_) => InlineSpan::Bold(token_accum.clone()),
                        InlineSpan::Code(_) => InlineSpan::Code(token_accum.clone()),
                    });
                    token_accum.clear();
                }
                lines.push(FormattedLine { spans: std::mem::take(&mut cur_spans) });
                cur_line_w = 0;
            }
            token_accum.push_str(&tok);
            cur_line_w += tok_w;
        }

        if !token_accum.is_empty() {
            cur_spans.push(match span {
                InlineSpan::Normal(_) => InlineSpan::Normal(token_accum),
                InlineSpan::Bold(_) => InlineSpan::Bold(token_accum),
                InlineSpan::Code(_) => InlineSpan::Code(token_accum),
            });
        }
    }

    if !cur_spans.is_empty() {
        lines.push(FormattedLine { spans: cur_spans });
    }

    if lines.is_empty() {
        lines.push(FormattedLine { spans: Vec::new() });
    }
    lines
}

pub fn render_spans_line(
    hdc: HDC,
    font_normal: HFONT,
    font_bold: HFONT,
    font_code: HFONT,
    text_color: COLORREF,
    code_color: COLORREF,
    code_bg: COLORREF,
    start_x: i32,
    y: i32,
    spans: &[InlineSpan],
    px: impl Fn(i32) -> i32,
) {
    let mut x = start_x;
    for span in spans {
        match span {
            InlineSpan::Normal(t) => {
                x += draw_text_gdi_simple(hdc, font_normal, text_color, x, y, t);
            }
            InlineSpan::Bold(t) => {
                x += draw_text_gdi_simple(hdc, font_bold, text_color, x, y, t);
            }
            InlineSpan::Code(t) => {
                let code_w = measure_text_width(hdc, font_code, t);
                let badge_r = RECT {
                    left: x,
                    top: y - px(1),
                    right: x + code_w + px(6),
                    bottom: y + px(17),
                };
                crate::ui::candidates::view::fill_round_rect(hdc, badge_r, code_bg, 3);
                let _ = draw_text_gdi_simple(hdc, font_code, code_color, x + px(3), y, t);
                x += code_w + px(6);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inline_spans() {
        let text = "Hello **world** and `code` test";
        let spans = parse_inline_spans(text);
        assert_eq!(spans.len(), 5);
        assert_eq!(spans[0], InlineSpan::Normal("Hello ".to_string()));
        assert_eq!(spans[1], InlineSpan::Bold("world".to_string()));
        assert_eq!(spans[2], InlineSpan::Normal(" and ".to_string()));
        assert_eq!(spans[3], InlineSpan::Code("code".to_string()));
        assert_eq!(spans[4], InlineSpan::Normal(" test".to_string()));
    }

    #[test]
    fn test_parse_markdown_blocks() {
        let md = "### 标题\n- 列表项1\n1. 序号项\n普通段落包含**加粗**";
        let blocks = parse_markdown_blocks(md);
        assert_eq!(blocks.len(), 4);
        match &blocks[0] {
            MarkdownBlock::Header { level, text } => {
                assert_eq!(*level, 3);
                assert_eq!(text, "标题");
            }
            _ => panic!("expected Header"),
        }
        match &blocks[1] {
            MarkdownBlock::BulletItem { text } => {
                assert_eq!(text.len(), 1);
                assert_eq!(text[0], InlineSpan::Normal("列表项1".to_string()));
            }
            _ => panic!("expected BulletItem"),
        }
        match &blocks[2] {
            MarkdownBlock::NumberedItem { number, text } => {
                assert_eq!(*number, 1);
                assert_eq!(text.len(), 1);
                assert_eq!(text[0], InlineSpan::Normal("序号项".to_string()));
            }
            _ => panic!("expected NumberedItem"),
        }
        match &blocks[3] {
            MarkdownBlock::Paragraph { spans } => {
                assert_eq!(spans.len(), 2);
                assert_eq!(spans[0], InlineSpan::Normal("普通段落包含".to_string()));
                assert_eq!(spans[1], InlineSpan::Bold("加粗".to_string()));
            }
            _ => panic!("expected Paragraph"),
        }
    }
}
