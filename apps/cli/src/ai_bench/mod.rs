//! 云端打字识别测速：把样本集逐条喂给引擎，用真实接口跑当前的意图纠错链路，按模型出 Markdown 对比表。
//!
//! 只测 `request_intent_suggestion` 这一条（长拼音纠错）；它走的就是输入法里的 `CloudPredictor`，
//! 所以延迟包含防抖、网络与本地校验。每轮重建 Predictor，避免缓存让后几轮的耗时失真。

mod error;
mod outcome;
mod report;
mod sample;

use std::path::Path;
use std::time::{Duration, Instant};

use qingjian_core::{Candidate, CandidateKind, Engine, SurroundingText};
use qingjian_platform::Config;
use qingjian_predict::CloudPredictor;

pub use error::BenchError;
use outcome::Outcome;
use sample::Sample;

/// 轮询间隔；比接口超时多留的余量，让超时请求也能自己返回。
const POLL: Duration = Duration::from_millis(10);
const WAIT_SLACK: Duration = Duration::from_millis(1500);

/// 跑完所有模型，返回完整的 Markdown 报告。
pub fn run(
    engine: &mut Engine,
    config: &Config,
    samples_path: &Path,
    models: &[String],
    rounds: usize,
    limit: usize,
) -> Result<String, BenchError> {
    if models.is_empty() {
        return Err(BenchError::NoModels);
    }
    let samples = sample::load(samples_path, limit)?;
    let mut sections = Vec::new();
    for model in models {
        let mut predict = config.predict.clone();
        predict.enabled = true;
        predict.typing_model = model.clone();
        let wait = Duration::from_millis(predict.timeout_ms) + WAIT_SLACK;
        let mut outcomes = Vec::new();
        for round in 0..rounds.max(1) {
            engine.set_predictor(Box::new(CloudPredictor::new(&predict)?));
            eprintln!(
                "{model}: 第 {} / {} 轮，{} 条",
                round + 1,
                rounds.max(1),
                samples.len()
            );
            for sample in &samples {
                outcomes.push(measure(engine, sample, wait));
            }
        }
        sections.push(report::render(model, rounds.max(1), &outcomes));
    }
    Ok(sections.join("\n"))
}

fn measure(engine: &mut Engine, sample: &Sample, wait: Duration) -> Outcome {
    crate::display::feed(engine, &sample.letters);
    let Ok(query) = engine.query() else {
        return Outcome::failed(sample);
    };
    let local = texts(&query.candidates.items);
    let local_rank = rank_of(&local, &sample.expected);

    let surrounding = SurroundingText {
        before: sample.before.clone(),
        after: sample.after.clone(),
    };
    let sent = engine
        .request_intent_suggestion(Some(surrounding), &query.candidates.items)
        .is_some();
    let mut latency = None;
    let mut suggestion = None;
    if sent {
        let start = Instant::now();
        loop {
            if let Some(prediction) = engine.poll_prediction() {
                suggestion = prediction.sentence;
                latency = Some(start.elapsed());
                break;
            }
            if start.elapsed() >= wait {
                latency = Some(start.elapsed());
                break;
            }
            std::thread::sleep(POLL);
        }
    }
    engine.cancel_prediction();
    let final_rank = match &suggestion {
        Some(text) if *text == sample.expected => Some(0),
        Some(text) => {
            rank_of(&local, &sample.expected).map(|rank| rank + usize::from(!local.contains(text)))
        }
        None => local_rank,
    };
    Outcome {
        category: sample.category.clone(),
        local_rank,
        final_rank,
        sent,
        latency,
        accepted: suggestion.is_some(),
    }
}

/// 候选里的汉字词与整句，去重后保持顺序。
fn texts(items: &[Candidate]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for candidate in items {
        if matches!(
            candidate.kind,
            CandidateKind::Chinese | CandidateKind::Sentence
        ) && !out.contains(&candidate.text)
        {
            out.push(candidate.text.clone());
        }
    }
    out
}

fn rank_of(list: &[String], expected: &str) -> Option<usize> {
    list.iter().position(|text| text == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_is_zero_based_and_none_when_missing() {
        let list = vec!["你好".to_owned(), "尼好".to_owned()];
        assert_eq!(rank_of(&list, "尼好"), Some(1));
        assert_eq!(rank_of(&list, "拟好"), None);
    }
}
