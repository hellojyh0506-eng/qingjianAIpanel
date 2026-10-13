//! 行内 Markdown：粗体与行内代码。

/// 一小段行内文字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Plain(String),

    /// `**粗体**`。
    Bold(String),

    /// `` `行内代码` ``。
    Code(String),
}

impl Inline {
    /// 段内文字（不含标记符号）。
    pub fn text(&self) -> &str {
        match self {
            Inline::Plain(text) | Inline::Bold(text) | Inline::Code(text) => text,
        }
    }
}

/// 解析一行里的 `**粗体**` 与 `` `代码` ``；没闭合的标记按原样保留成普通文字（流式输出到一半时常见）。
pub fn parse_inline(text: &str) -> Vec<Inline> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            chars.next();
            let mut inner = String::new();
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'*') {
                    chars.next();
                    closed = true;
                    break;
                }
                inner.push(next);
            }
            if closed && !inner.is_empty() {
                flush(&mut spans, &mut plain);
                spans.push(Inline::Bold(inner));
            } else {
                plain.push_str("**");
                plain.push_str(&inner);
                if closed {
                    plain.push_str("**");
                }
            }
        } else if ch == '`' {
            let mut inner = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == '`' {
                    closed = true;
                    break;
                }
                inner.push(next);
            }
            if closed && !inner.is_empty() {
                flush(&mut spans, &mut plain);
                spans.push(Inline::Code(inner));
            } else {
                plain.push('`');
                plain.push_str(&inner);
                if closed {
                    plain.push('`');
                }
            }
        } else {
            plain.push(ch);
        }
    }
    flush(&mut spans, &mut plain);
    spans
}

fn flush(spans: &mut Vec<Inline>, plain: &mut String) {
    if !plain.is_empty() {
        spans.push(Inline::Plain(std::mem::take(plain)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Inline {
        Inline::Plain(text.into())
    }

    #[test]
    fn bold_and_code_are_split_out() {
        assert_eq!(
            parse_inline("use **do your best** or `try hard` here"),
            vec![
                plain("use "),
                Inline::Bold("do your best".into()),
                plain(" or "),
                Inline::Code("try hard".into()),
                plain(" here"),
            ]
        );
    }

    #[test]
    fn unclosed_markers_stay_as_text() {
        assert_eq!(parse_inline("a **bold"), vec![plain("a **bold")]);
        assert_eq!(parse_inline("a `code"), vec![plain("a `code")]);
        assert_eq!(parse_inline("2 * 3 = 6"), vec![plain("2 * 3 = 6")]);
    }

    #[test]
    fn empty_markers_are_kept_literally() {
        assert_eq!(parse_inline("a **** b"), vec![plain("a **** b")]);
        assert_eq!(parse_inline("a `` b"), vec![plain("a `` b")]);
    }

    #[test]
    fn chinese_text_is_not_split() {
        assert_eq!(
            parse_inline("**语气**：轻松"),
            vec![Inline::Bold("语气".into()), plain("：轻松")]
        );
    }
}
