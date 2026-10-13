//! 输入框的文本编辑：光标是 `text` 里的字节位置，所有操作都落在字符边界上。

/// 输入框最多容纳的字符数，防止误粘贴超大文本拖慢排版。
pub(super) const MAX_CHARS: usize = 8000;

/// 在光标处插入 `piece`（换行统一成 `\n`，控制字符丢弃），超出上限的部分截掉。
pub(super) fn insert(text: &mut String, cursor: &mut usize, piece: &str) {
    let room = MAX_CHARS.saturating_sub(text.chars().count());
    let cleaned: String = piece
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|ch| *ch == '\n' || !ch.is_control())
        .take(room)
        .collect();
    if cleaned.is_empty() {
        return;
    }
    text.insert_str(*cursor, &cleaned);
    *cursor += cleaned.len();
}

/// 删除光标前一个字符；返回是否真的删了。
pub(super) fn backspace(text: &mut String, cursor: &mut usize) -> bool {
    let Some(start) = prev_boundary(text, *cursor) else {
        return false;
    };
    text.replace_range(start..*cursor, "");
    *cursor = start;
    true
}

/// 删除光标后一个字符；返回是否真的删了。
pub(super) fn delete(text: &mut String, cursor: usize) -> bool {
    let Some(end) = next_boundary(text, cursor) else {
        return false;
    };
    text.replace_range(cursor..end, "");
    true
}

pub(super) fn left(text: &str, cursor: usize) -> usize {
    prev_boundary(text, cursor).unwrap_or(cursor)
}

pub(super) fn right(text: &str, cursor: usize) -> usize {
    next_boundary(text, cursor).unwrap_or(cursor)
}

/// 当前这一行（以 `\n` 分行）的开头。
pub(super) fn home(text: &str, cursor: usize) -> usize {
    text[..cursor].rfind('\n').map_or(0, |at| at + 1)
}

/// 当前这一行的末尾（换行符之前）。
pub(super) fn end(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .find('\n')
        .map_or(text.len(), |at| cursor + at)
}

fn prev_boundary(text: &str, cursor: usize) -> Option<usize> {
    text[..cursor].char_indices().next_back().map(|(at, _)| at)
}

fn next_boundary(text: &str, cursor: usize) -> Option<usize> {
    text[cursor..]
        .chars()
        .next()
        .map(|ch| cursor + ch.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_moves_cursor_past_multibyte_text() {
        let mut text = "ab".to_string();
        let mut cursor = 1;
        insert(&mut text, &mut cursor, "你好");
        assert_eq!(text, "a你好b");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn insert_normalizes_newlines_and_drops_control_chars() {
        let mut text = String::new();
        let mut cursor = 0;
        insert(&mut text, &mut cursor, "a\r\nb\rc\x07\td");
        assert_eq!(text, "a\nb\ncd");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn insert_stops_at_the_character_limit() {
        let mut text = "x".repeat(MAX_CHARS - 2);
        let mut cursor = text.len();
        insert(&mut text, &mut cursor, "你好吗");
        assert_eq!(text.chars().count(), MAX_CHARS);
        assert!(text.ends_with("你好"));
        insert(&mut text, &mut cursor, "多");
        assert_eq!(text.chars().count(), MAX_CHARS);
    }

    #[test]
    fn backspace_and_delete_remove_whole_characters() {
        let mut text = "a你b".to_string();
        let mut cursor = 4;
        assert!(backspace(&mut text, &mut cursor));
        assert_eq!((text.as_str(), cursor), ("ab", 1));
        assert!(delete(&mut text, cursor));
        assert_eq!(text, "a");
        assert!(!delete(&mut text, 1));
        let mut zero = 0;
        assert!(!backspace(&mut text, &mut zero));
    }

    #[test]
    fn arrows_step_over_multibyte_characters_and_clamp() {
        let text = "你a";
        assert_eq!(right(text, 0), 3);
        assert_eq!(right(text, 3), 4);
        assert_eq!(right(text, 4), 4);
        assert_eq!(left(text, 4), 3);
        assert_eq!(left(text, 3), 0);
        assert_eq!(left(text, 0), 0);
    }

    #[test]
    fn home_and_end_work_per_line() {
        let text = "ab\n你好\ncd";
        assert_eq!(home(text, 1), 0);
        assert_eq!(end(text, 1), 2);
        assert_eq!(home(text, 6), 3);
        assert_eq!(end(text, 6), 9);
        assert_eq!(home(text, text.len()), 10);
        assert_eq!(end(text, text.len()), text.len());
    }
}
