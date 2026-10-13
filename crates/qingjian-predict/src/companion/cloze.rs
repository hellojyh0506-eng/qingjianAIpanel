//! 挖空练习：把例句里要学的短语换成空缺，让学习者自己补出来。

/// 空缺的占位符。
pub const BLANK: &str = "____";

/// 把 `sentence` 里第一处 `phrase`（忽略英文大小写）换成 [`BLANK`]；句子里找不到短语或短语为空时返回 `None`。
pub fn cloze(sentence: &str, phrase: &str) -> Option<String> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return None;
    }
    // 只转 ASCII 大小写，字节长度不变，找到的下标可以直接用来切原句。
    let start = sentence
        .to_ascii_lowercase()
        .find(&phrase.to_ascii_lowercase())?;
    let end = start + phrase.len();
    Some(format!(
        "{}{}{}",
        &sentence[..start],
        BLANK,
        &sentence[end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanks_out_the_phrase_ignoring_case() {
        assert_eq!(
            cloze("Let's call it a day and go home.", "call it a day").as_deref(),
            Some("Let's ____ and go home.")
        );
        assert_eq!(
            cloze("Call it a day, will you?", "call it a day").as_deref(),
            Some("____, will you?")
        );
    }

    #[test]
    fn only_the_first_match_is_blanked() {
        assert_eq!(cloze("so so good", "so").as_deref(), Some("____ so good"));
    }

    #[test]
    fn missing_or_empty_phrase_gives_none() {
        assert_eq!(cloze("See you soon.", "call it a day"), None);
        assert_eq!(cloze("See you soon.", "  "), None);
    }

    #[test]
    fn chinese_neighbours_keep_their_boundaries() {
        assert_eq!(
            cloze("他说 no big deal 就走了", "No Big Deal").as_deref(),
            Some("他说 ____ 就走了")
        );
    }
}
