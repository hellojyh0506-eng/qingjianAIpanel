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

/// 只测本地：不碰网络，按类别出 Top1 / Top3、逐样本查询耗时的 P50 / P95，`misses` 不为 0 时列出没排第 1 的样本。
pub fn run_local(
    engine: &mut Engine,
    samples_path: &Path,
    limit: usize,
    misses: usize,
) -> Result<String, BenchError> {
    let samples = sample::load(samples_path, limit)?;
    let mut rows: Vec<report::LocalRow> = Vec::new();
    let mut missed: Vec<String> = Vec::new();
    for sample in &samples {
        let start = Instant::now();
        crate::display::feed(engine, &sample.letters);
        let query = engine.query().ok();
        let elapsed = start.elapsed();
        let marked_first = query
            .as_ref()
            .and_then(|query| query.candidates.items.first())
            .is_some_and(|first| first.corrected);
        let local = query
            .map(|query| texts(&query.candidates.items))
            .unwrap_or_default();
        let rank = rank_of(&local, &sample.expected);
        // clean 样本本来就敲对了：首选被标「纠」就是误纠，一并列出
        let misfired = sample.category == "clean" && marked_first;
        if (misfired || (rank != Some(0) && sample.category != "clean")) && missed.len() < misses {
            missed.push(format!(
                "{}{}\t{}\t期望 {}\t第 {}\t首选 {}",
                if misfired { "误纠 " } else { "" },
                sample.category,
                sample.letters,
                sample.expected,
                rank.map_or("-".to_owned(), |r| (r + 1).to_string()),
                local.first().map_or("-", String::as_str)
            ));
        }
        rows.push(report::LocalRow {
            category: sample.category.clone(),
            rank,
            elapsed,
            marked_first,
            similarity: local
                .first()
                .map_or(0.0, |first| similarity(first, &sample.expected)),
        });
    }
    Ok(report::render_local(&rows, &missed))
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

/// 首选与期望的字符相似度：最长公共子序列占期望长度的比例，看「差几个字」而不只是对不对。
fn similarity(candidate: &str, expected: &str) -> f64 {
    let a: Vec<char> = candidate.chars().collect();
    let b: Vec<char> = expected.chars().collect();
    if b.is_empty() {
        return 0.0;
    }
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..a.len() {
        for j in 0..b.len() {
            table[i + 1][j + 1] = if a[i] == b[j] {
                table[i][j] + 1
            } else {
                table[i][j + 1].max(table[i + 1][j])
            };
        }
    }
    table[a.len()][b.len()] as f64 / b.len() as f64
}

fn rank_of(list: &[String], expected: &str) -> Option<usize> {
    list.iter().position(|text| text == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_is_the_share_of_expected_characters_kept_in_order() {
        assert_eq!(similarity("你好", "你好"), 1.0);
        assert_eq!(similarity("你号", "你好"), 0.5);
        assert_eq!(similarity("", "你好"), 0.0);
        assert_eq!(similarity("我今天想去差施加东西", "我今天想去超市买东西"), 0.7);
    }

    #[test]
    fn rank_is_zero_based_and_none_when_missing() {
        let list = vec!["你好".to_owned(), "尼好".to_owned()];
        assert_eq!(rank_of(&list, "尼好"), Some(1));
        assert_eq!(rank_of(&list, "拟好"), None);
    }
}
