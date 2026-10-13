//! Resolve（当前这一串想打什么）在网络侧的部分：用户消息的形状与模型回复的解析。
//!
//! 解析只管格式与编号范围：回复里的拼音是否合理、纠正后能不能在本地词库里找到文字，
//! 由 Core 的 `Engine::poll_prediction` 校验。

pub mod prompt;

use qingjian_core::{CloudWord, PredictionRequest};
use serde::Serialize;

use crate::prompt::{Reply, extract_json_str};

#[derive(Serialize)]
struct CandidateEntry<'a> {
    i: usize,
    text: &'a str,
    reading: &'a str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    corrected: bool,
}

#[derive(Serialize)]
struct Message<'a> {
    letters: &'a str,
    segmentation: &'a str,
    before: &'a str,
    after: &'a str,
    candidates: Vec<CandidateEntry<'a>>,
}

/// 发给模型的用户消息。
pub fn user_prompt(request: &PredictionRequest) -> String {
    let candidates = request
        .resolve_candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| CandidateEntry {
            i: index + 1,
            text: &candidate.text,
            reading: &candidate.reading,
            corrected: candidate.corrected,
        })
        .collect();
    serde_json::to_string(&Message {
        letters: &request.letters,
        segmentation: &request.pinyin,
        before: &request.before,
        after: &request.after,
        candidates,
    })
    .unwrap_or_default()
}

/// 解析模型回复。拼音放进 `sentence`，选中的候选文字放进 `words[0]`（`pick` 为 0 时没有）。
/// 拼音里的空格与 `'` 先去掉（`ni'hao`、`shenme gui` 照样认）；去掉后仍不是全小写字母就当没给拼音，
/// 但选中的候选还在时照样返回（Core 以候选自己的读音为准）。格式不对、编号越界、拼音与候选都没有一律当没回复。
pub fn parse_reply(content: &str, request: &PredictionRequest) -> Reply {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(extract_json_str(content)) else {
        return Reply::default();
    };
    let pinyin: String = value
        .get("pinyin")
        .and_then(|pinyin| pinyin.as_str())
        .unwrap_or_default()
        .chars()
        .filter(|ch| *ch != '\'' && !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let pinyin = (!pinyin.is_empty() && pinyin.bytes().all(|byte| byte.is_ascii_lowercase()))
        .then_some(pinyin);
    let pick = match value.get("pick") {
        None | Some(serde_json::Value::Null) => 0,
        Some(serde_json::Value::Number(number)) => match number.as_u64() {
            Some(pick) => pick as usize,
            None => return Reply::default(),
        },
        Some(serde_json::Value::String(text)) => match text.trim().parse::<usize>() {
            Ok(pick) => pick,
            Err(_) => return Reply::default(),
        },
        Some(_) => return Reply::default(),
    };
    let words = match pick {
        0 => Vec::new(),
        pick => match request.resolve_candidates.get(pick - 1) {
            Some(candidate) => vec![CloudWord {
                text: candidate.text.clone(),
                syllables: Vec::new(),
                reading: None,
            }],
            None => return Reply::default(),
        },
    };
    if pinyin.is_none() && words.is_empty() {
        return Reply::default();
    }
    Reply {
        words,
        sentence: pinyin,
        ..Reply::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_core::{PredictionKind, ResolveCandidate};

    fn request() -> PredictionRequest {
        PredictionRequest {
            kind: PredictionKind::Resolve,
            letters: "nihoomma".to_owned(),
            pinyin: "ni'hoo'mma".to_owned(),
            before: "你好".to_owned(),
            resolve_candidates: vec![
                ResolveCandidate {
                    text: "你好吗".to_owned(),
                    reading: "ni'hao'ma".to_owned(),
                    corrected: true,
                },
                ResolveCandidate {
                    text: "你和妈".to_owned(),
                    reading: "ni'he'ma".to_owned(),
                    corrected: false,
                },
            ],
            ..PredictionRequest::default()
        }
    }

    #[test]
    fn user_message_numbers_candidates_from_one_and_skips_false_flags() {
        let message: serde_json::Value = serde_json::from_str(&user_prompt(&request())).unwrap();
        assert_eq!(message["letters"], "nihoomma");
        assert_eq!(message["segmentation"], "ni'hoo'mma");
        assert_eq!(message["candidates"][0]["i"], 1);
        assert_eq!(message["candidates"][0]["corrected"], true);
        assert_eq!(message["candidates"][1]["i"], 2);
        assert!(message["candidates"][1].get("corrected").is_none());
    }

    #[test]
    fn valid_reply_carries_pinyin_and_the_picked_text() {
        let reply = parse_reply(r#"{"pinyin":"nihaoma","pick":1}"#, &request());
        assert_eq!(reply.sentence.as_deref(), Some("nihaoma"));
        assert_eq!(reply.words.len(), 1);
        assert_eq!(reply.words[0].text, "你好吗");
    }

    #[test]
    fn zero_pick_keeps_the_pinyin_without_a_word() {
        let reply = parse_reply(r#"{"pinyin":"nihaoma","pick":0}"#, &request());
        assert_eq!(reply.sentence.as_deref(), Some("nihaoma"));
        assert!(reply.words.is_empty());
    }

    #[test]
    fn pick_may_be_a_numeric_string_and_fenced_json_is_accepted() {
        let reply = parse_reply(
            "```json\n{\"pinyin\":\"nihaoma\",\"pick\":\"2\"}\n```",
            &request(),
        );
        assert_eq!(reply.words[0].text, "你和妈");
    }

    #[test]
    fn out_of_range_pick_is_discarded() {
        assert_eq!(
            parse_reply(r#"{"pinyin":"nihaoma","pick":3}"#, &request()),
            Reply::default()
        );
        assert_eq!(
            parse_reply(r#"{"pinyin":"nihaoma","pick":-1}"#, &request()),
            Reply::default()
        );
    }

    #[test]
    fn separators_in_the_pinyin_are_ignored() {
        let reply = parse_reply(r#"{"pinyin":"ni'hao ma","pick":1}"#, &request());
        assert_eq!(reply.sentence.as_deref(), Some("nihaoma"));
    }

    #[test]
    fn unusable_pinyin_keeps_a_valid_pick_and_nothing_valid_is_discarded() {
        let reply = parse_reply(r#"{"pinyin":"ni3hao","pick":1}"#, &request());
        assert_eq!(reply.sentence, None);
        assert_eq!(reply.words[0].text, "你好吗");
        assert_eq!(
            parse_reply(r#"{"pinyin":"","pick":0}"#, &request()),
            Reply::default()
        );
        assert_eq!(parse_reply(r#"{"pick":0}"#, &request()), Reply::default());
        assert_eq!(parse_reply("not json", &request()), Reply::default());
    }
}
