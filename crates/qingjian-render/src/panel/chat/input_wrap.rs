//! 输入框的换行：先按 `\n` 分段，再按宽度逐字贪心折行，英文尽量在空格处断。

/// 一个视觉行：在文本里的字节范围（不含换行符）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Line {
    pub start: usize,
    pub end: usize,
}

/// 把 `text` 折成视觉行；`width_of` 给出单个字符的宽度（点）。至少返回一行。
pub(super) fn wrap(text: &str, max_width: f32, mut width_of: impl FnMut(char) -> f32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for segment in text.split('\n') {
        wrap_segment(segment, offset, max_width, &mut width_of, &mut lines);
        offset += segment.len() + 1;
    }
    lines
}

fn wrap_segment(
    segment: &str,
    base: usize,
    max_width: f32,
    width_of: &mut impl FnMut(char) -> f32,
    lines: &mut Vec<Line>,
) {
    let mut start = 0;
    let mut used = 0.0;
    let mut last_space: Option<usize> = None;
    for (at, ch) in segment.char_indices() {
        let width = width_of(ch);
        if used + width > max_width && at > start {
            let cut = last_space.filter(|&cut| cut > start).unwrap_or(at);
            lines.push(Line {
                start: base + start,
                end: base + cut,
            });
            start = cut;
            used = segment[start..at].chars().map(&mut *width_of).sum();
            last_space = None;
        }
        used += width;
        if ch == ' ' {
            last_space = Some(at + 1);
        }
    }
    lines.push(Line {
        start: base + start,
        end: base + segment.len(),
    });
}

/// 光标落在哪一行：软折行处的边界归下一行，硬换行前归本行。
pub(super) fn caret_line(lines: &[Line], cursor: usize) -> usize {
    lines
        .iter()
        .rposition(|line| line.start <= cursor)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(ch: char) -> f32 {
        if ch.is_ascii() { 5.0 } else { 10.0 }
    }

    fn texts<'a>(text: &'a str, lines: &[Line]) -> Vec<&'a str> {
        lines.iter().map(|l| &text[l.start..l.end]).collect()
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        let lines = wrap("", 100.0, width);
        assert_eq!(lines, vec![Line { start: 0, end: 0 }]);
    }

    #[test]
    fn hard_newlines_split_lines() {
        let text = "ab\ncd\n";
        let lines = wrap(text, 100.0, width);
        assert_eq!(texts(text, &lines), vec!["ab", "cd", ""]);
        // 光标在第一个换行符之前归第一行，之后归第二行
        assert_eq!(caret_line(&lines, 2), 0);
        assert_eq!(caret_line(&lines, 3), 1);
        assert_eq!(caret_line(&lines, text.len()), 2);
    }

    #[test]
    fn chinese_breaks_anywhere_and_never_inside_a_char() {
        let text = "一二三四五六七";
        let lines = wrap(text, 30.0, width);
        assert_eq!(texts(text, &lines), vec!["一二三", "四五六", "七"]);
    }

    #[test]
    fn english_breaks_after_a_space_when_possible() {
        let text = "hello wonderful world";
        let lines = wrap(text, 70.0, width);
        assert_eq!(texts(text, &lines), vec!["hello ", "wonderful ", "world"]);
        assert!(lines.windows(2).all(|pair| pair[0].end == pair[1].start));
    }

    #[test]
    fn overlong_word_is_broken_by_character() {
        let text = "abcdefghij";
        let lines = wrap(text, 25.0, width);
        assert_eq!(texts(text, &lines), vec!["abcde", "fghij"]);
        // 软折行的边界归下一行
        assert_eq!(caret_line(&lines, 5), 1);
    }
}
