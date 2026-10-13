//! 三个标签的提示词。模型逐行输出 JSON（见 `block.rs`），每个标签带一条「尽量 / Do your best」少样本示例。

use serde_json::json;

use super::request::CompanionRequest;
use super::tab::Tab;
use crate::stream_client::ChatMessage;

const SYSTEM: &str = r#"You are the English-learning engine inside a Chinese pinyin input method. The learner typed a Chinese expression; teach them how to say it naturally in American English.

Output rules (strict):
- Output JSON Lines only: one compact JSON object per line, in the exact order described. No markdown, no code fences, no commentary, no blank lines.
- Every object has a "k" field. Use only the fields shown. Keep every string on a single line.
- Chinese fields: Simplified Chinese, concise. English fields: natural American spoken English with no Chinese characters.
- "hl" is the key phrase exactly as it appears inside the English sentence of the same object (same letters, same case).
- Do not invent facts about the learner. If the focus expression does not fit the Chinese meaning, teach the most natural expression instead."#;

const NUANCE: &str = r#"Task: nuance of the focus expression.
Lines, in this order:
{"k":"header","tags":[up to 3 Chinese tags, each at most 4 characters],"level":"CEFR level such as B1-B2"}
{"k":"meaning","text":"意思，一句话"}
{"k":"tone","text":"语气，一句话"}
{"k":"usage","text":"使用场景，一句话"}
{"k":"example","en":"one natural sentence, at most 15 words","zh":"中文翻译","hl":"focus phrase"}
then exactly 3 lines {"k":"detail","title":"搭配 / 语法 / 易错点, at most 4 characters","body":"一两句说明，可夹英文"}

Example for zh=尽量, en=Do your best:
{"k":"header","tags":["自然口语","鼓励支持","常用表达"],"level":"B1-B2"}
{"k":"meaning","text":"尽你所能；尽力就好。"}
{"k":"tone","text":"鼓励、支持，强调努力，不要求结果一定完美。"}
{"k":"usage","text":"用于安慰或鼓励别人，例如考试、工作、比赛等场景。"}
{"k":"example","en":"Just do your best. That's all you can do.","zh":"尽力就好，你能做的就是把自己能做的做到最好。","hl":"Just do your best"}
{"k":"detail","title":"搭配","body":"do your best to + 动词：尽力去做某事。"}
{"k":"detail","title":"语法","body":"best 是 good 的最高级，这里作名词，前面不加 the。"}
{"k":"detail","title":"易错点","body":"不要说 do your best effort，直接用 do your best。"}"#;

const ALTERNATIVES: &str = r#"Task: three more natural ways to say it.
Exactly 3 lines, i = 1, 2, 3. Each must be a genuinely different expression (not a small edit of the focus expression or of each other):
{"k":"alt","i":1,"expr":"English expression, at most 8 words","zh":"中文含义","tag":"one of 日常口语 / 温和安慰 / 更有力度 / 鼓励支持 / 轻松随意 / 书面正式","tone":"语气，几个词","diff":"与原表达的区别，一句话","ex_en":"new natural example, at most 15 words","ex_zh":"例句翻译"}

Example for zh=尽量, en=Do your best:
{"k":"alt","i":1,"expr":"Give it your best shot","zh":"尽力一试；试试看。","tag":"日常口语","tone":"鼓励、轻松","diff":"强调尝试，不确定能否成功时更自然。","ex_en":"You should give it your best shot. Nothing to lose!","ex_zh":"你应该尽力一试，反正没有损失！"}"#;

const DIALOGUE: &str = r#"Task: a short realistic dialogue that uses the focus expression naturally.
Lines, in this order:
{"k":"scene","title":"场景标题，如 朋友聊天（鼓励安慰）"}
exactly 3 lines {"k":"turn","sp":"A" or "B","en":"at most 15 words","zh":"中文翻译","hl":"key phrase or empty"} with speakers A, B, A; the focus expression (or a close variant) must appear in B's line and be given as hl
exactly 2 lines {"k":"point","text":"表达亮点，一句话"}

Example for zh=尽量, en=Do your best, scene "朋友聊天（鼓励安慰）":
{"k":"scene","title":"朋友聊天（鼓励安慰）"}
{"k":"turn","sp":"A","en":"I have a big presentation tomorrow. I'm really nervous.","zh":"我明天有个重要的演讲，有点紧张。","hl":""}
{"k":"turn","sp":"B","en":"You'll be fine. Just do your best.","zh":"你会没问题的，尽力就好。","hl":"Just do your best"}
{"k":"turn","sp":"A","en":"Thanks! That makes me feel better.","zh":"谢谢！这样我感觉好多了。","hl":""}
{"k":"point","text":"在安慰和鼓励场景中非常自然。"}
{"k":"point","text":"语气积极友好，适合日常对话。"}"#;

const IELTS: &str = r#"Task: an IELTS Speaking practice item that uses the focus expression naturally.
Lines, in this order:
{"k":"scene","title":"雅思口语题目的主题，如 Part 1：学习与压力"}
{"k":"question","en":"the examiner's question, at most 20 words","zh":"题目翻译"}
exactly 3 lines {"k":"turn","sp":"B","en":"at most 15 words","zh":"中文翻译","hl":"key phrase or empty"}: three sentences that together form one coherent high-scoring answer (clear answer, a reason or example, a natural finish); the focus expression (or a close variant) appears in one of them and is given as hl
exactly 2 lines {"k":"point","text":"高分点，一句话，如 衔接自然、有具体例子"}"#;

/// 一次请求要发给模型的消息。
pub fn messages(request: &CompanionRequest) -> Vec<ChatMessage> {
    let task = match request.tab {
        Tab::Nuance => NUANCE.to_owned(),
        Tab::Alternatives => ALTERNATIVES.to_owned(),
        Tab::Dialogue if request.scene.is_ielts() => IELTS.to_owned(),
        Tab::Dialogue => format!("{DIALOGUE}\n\nScene style: {}.", request.scene.guidance()),
    };
    let focus = if request.en.is_empty() {
        "(none; choose the most natural American English expression yourself)"
    } else {
        request.en.as_str()
    };
    let user = json!({ "zh": request.zh, "en": focus });
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: format!("{SYSTEM}\n\n{task}"),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: user.to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion::jsonl::BlockParser;
    use crate::companion::scene::Scene;

    /// 提示词里「Example for」之后的示例行必须能被我们自己的解析器接受，否则示例会把模型教坏。
    fn example_lines(task: &str) -> String {
        let (_, example) = task.split_once("Example for").unwrap();
        example
            .lines()
            .filter(|line| line.starts_with("{\"k\":"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn worked_examples_parse_cleanly() {
        for (tab, task, expected) in [
            (Tab::Nuance, NUANCE, 8),
            (Tab::Alternatives, ALTERNATIVES, 1),
            (Tab::Dialogue, DIALOGUE, 6),
        ] {
            let mut parser = BlockParser::new(tab);
            let blocks = parser.push(&format!("{}\n", example_lines(task)));
            assert_eq!(blocks.len(), expected, "{tab:?}: {:?}", parser.stats());
            assert_eq!(parser.stats().bad, 0, "{tab:?}");
        }
    }

    #[test]
    fn ielts_uses_its_own_task_and_user_message_is_json() {
        let ielts = messages(&CompanionRequest::new(
            Tab::Dialogue,
            Scene::Ielts,
            "尽量",
            "Do your best",
        ));
        assert!(ielts[0].content.contains("IELTS Speaking"));
        assert!(!ielts[0].content.contains("speakers A, B, A"));
        let daily = messages(&CompanionRequest::new(
            Tab::Dialogue,
            Scene::Work,
            "尽量",
            "",
        ));
        assert!(daily[0].content.contains("workplace conversation"));
        let user: serde_json::Value = serde_json::from_str(&daily[1].content).unwrap();
        assert_eq!(user["zh"], "尽量");
        assert!(
            user["en"]
                .as_str()
                .unwrap()
                .contains("choose the most natural")
        );
    }
}
