//! 会话主题给模型的上下文：用户从学习助手带着表达进来，后面的追问（「美国年轻人会怎么说？」）
//! 往往不再重复它，模型不知道在问什么，就得靠这条系统消息把话题串起来。

/// 告诉模型当前在学哪条表达。`english` 是学习助手里对应的英文表达（有就带上）。
pub(super) fn context_note(topic: &str, english: Option<&str>) -> String {
    let topic = topic.trim();
    let english = english
        .map(str::trim)
        .filter(|en| !en.is_empty() && *en != topic);
    let subject = match english {
        Some(en) => format!("「{topic}」（对应英文表达：{en}）"),
        None => format!("「{topic}」"),
    };
    format!(
        "用户正在学习的表达是{subject}。用户的每个问题默认都围绕它：即使问题里没再提到它\
         （例如「美国年轻人会怎么说？」「请给我 IELTS 口语题」），也要把它当作讨论对象来回答，\
         不要反问用户指的是什么；只有用户明确换了话题，才不再围绕它。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_the_topic_and_its_english() {
        let note = context_note(" 城堡 ", Some("castle"));
        assert!(note.contains("「城堡」（对应英文表达：castle）"));
        assert!(note.contains("不要反问"));
    }

    #[test]
    fn note_skips_empty_or_identical_english() {
        for english in [None, Some("  "), Some("Do your best")] {
            let note = context_note("Do your best", english);
            assert!(note.contains("「Do your best」。"), "{note}");
            assert!(!note.contains("对应英文表达"));
        }
    }
}
