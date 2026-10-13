//! 把测量结果汇成 Markdown 表。

use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Duration;

use super::outcome::Outcome;

/// 一个模型的报告：按类别分行，最后附误纠率。
pub fn render(model: &str, rounds: usize, outcomes: &[Outcome]) -> String {
    let mut by_category: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
    for outcome in outcomes {
        by_category
            .entry(&outcome.category)
            .or_default()
            .push(outcome);
    }
    let mut out = format!("## {model}（{rounds} 轮）\n\n");
    out.push_str("| 类别 | 条数 | 发请求 | 本地 Top1 | 本地 Top3 | +AI Top1 | +AI Top3 | P50 | P95 | 无有效建议 |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for (category, rows) in &by_category {
        let sent: Vec<&&Outcome> = rows.iter().filter(|row| row.sent).collect();
        let mut latencies: Vec<Duration> = sent.iter().filter_map(|row| row.latency).collect();
        latencies.sort();
        let empty = sent.iter().filter(|row| !row.accepted).count();
        let _ = writeln!(
            out,
            "| {category} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            rows.len(),
            sent.len(),
            percent(hits(rows, |row| row.local_rank, 1), rows.len()),
            percent(hits(rows, |row| row.local_rank, 3), rows.len()),
            percent(hits(rows, |row| row.final_rank, 1), rows.len()),
            percent(hits(rows, |row| row.final_rank, 3), rows.len()),
            millis(percentile(&latencies, 50)),
            millis(percentile(&latencies, 95)),
            percent(empty, sent.len()),
        );
    }
    let eligible: Vec<&Outcome> = outcomes
        .iter()
        .filter(|row| row.category == "clean" && row.local_rank == Some(0))
        .collect();
    let misfired = eligible
        .iter()
        .filter(|row| row.final_rank != Some(0))
        .count();
    let _ = writeln!(
        out,
        "\n误纠率（本地首选本来就对的 clean 样本，被改走的比例）：{}（{misfired} / {}）\n",
        percent(misfired, eligible.len()),
        eligible.len()
    );
    out.push_str("说明：延迟含防抖、网络与本地校验；走的是非流式接口，所以没有首字时间。\"无有效建议\" = 发了请求但没拿到通过本地校验的结果（空回复、坏回复、超时、被校验拒绝）。\n");
    out
}

/// 本地模式下一条样本的结果。
pub struct LocalRow {
    pub category: String,

    /// 期望答案在本地候选里的位置（0 起）；不在为 `None`。
    pub rank: Option<usize>,

    /// 敲键到出候选的耗时。
    pub elapsed: Duration,

    /// 首选候选带「纠」标记（本地把它当成纠正放在了第 1 位）。
    pub marked_first: bool,

    /// 首选与期望的字符相似度（0~1）。
    pub similarity: f64,
}

/// 本地模式的报告：每个类别一行（条数、Top1、Top3、首选字符相似度、查询耗时 P50 / P95），后面附没排第 1 的样本。
pub fn render_local(rows: &[LocalRow], missed: &[String]) -> String {
    let mut by_category: BTreeMap<&str, Vec<&LocalRow>> = BTreeMap::new();
    for row in rows {
        by_category.entry(&row.category).or_default().push(row);
    }
    let mut out = String::from(
        "| 类别 | 条数 | 本地 Top1 | 本地 Top3 | 首选字符相似度 | 首选标「纠」 | 查询 P50 | 查询 P95 |\n|---|---|---|---|---|---|---|---|\n",
    );
    for (category, group) in &by_category {
        let top = |n: usize| {
            group
                .iter()
                .filter(|row| row.rank.is_some_and(|r| r < n))
                .count()
        };
        let mut times: Vec<Duration> = group.iter().map(|row| row.elapsed).collect();
        times.sort();
        let similarity = group.iter().map(|row| row.similarity).sum::<f64>() / group.len() as f64;
        let _ = writeln!(
            out,
            "| {category} | {} | {} | {} | {:.1}% | {} | {} | {} |",
            group.len(),
            percent(top(1), group.len()),
            percent(top(3), group.len()),
            similarity * 100.0,
            percent(
                group.iter().filter(|row| row.marked_first).count(),
                group.len()
            ),
            millis(percentile(&times, 50)),
            millis(percentile(&times, 95)),
        );
    }
    if !missed.is_empty() {
        out.push_str("\n没排第 1 的样本：\n");
        for line in missed {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn hits(rows: &[&Outcome], rank: impl Fn(&Outcome) -> Option<usize>, top: usize) -> usize {
    rows.iter()
        .filter(|row| rank(row).is_some_and(|value| value < top))
        .count()
}

fn percent(part: usize, total: usize) -> String {
    if total == 0 {
        "-".to_owned()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / total as f64)
    }
}

/// 最近邻分位数；空集返回 `None`。
fn percentile(sorted: &[Duration], percent: usize) -> Option<Duration> {
    if sorted.is_empty() {
        return None;
    }
    let index = (sorted.len() * percent).div_ceil(100).max(1) - 1;
    Some(sorted[index.min(sorted.len() - 1)])
}

fn millis(value: Option<Duration>) -> String {
    value.map_or("-".to_owned(), |d| format!("{} ms", d.as_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(
        category: &str,
        local: Option<usize>,
        final_rank: Option<usize>,
        ms: u64,
    ) -> Outcome {
        Outcome {
            category: category.to_owned(),
            local_rank: local,
            final_rank,
            sent: true,
            latency: Some(Duration::from_millis(ms)),
            accepted: final_rank != local,
        }
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let sorted: Vec<Duration> = (1..=10).map(|n| Duration::from_millis(n * 100)).collect();
        assert_eq!(percentile(&sorted, 50), Some(Duration::from_millis(500)));
        assert_eq!(percentile(&sorted, 95), Some(Duration::from_millis(1000)));
        assert_eq!(percentile(&[], 50), None);
    }

    #[test]
    fn report_counts_hits_and_misfires() {
        let rows = vec![
            outcome("typo", None, Some(0), 400),
            outcome("typo", Some(2), Some(2), 600),
            outcome("clean", Some(0), Some(0), 300),
            outcome("clean", Some(0), Some(1), 500),
        ];
        let text = render("m", 1, &rows);
        assert!(
            text.contains("| typo | 2 | 2 | 0.0% | 50.0% | 50.0% | 100.0% |"),
            "{text}"
        );
        assert!(
            text.contains("误纠率（本地首选本来就对的 clean 样本，被改走的比例）：50.0%（1 / 2）"),
            "{text}"
        );
    }
}
