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
