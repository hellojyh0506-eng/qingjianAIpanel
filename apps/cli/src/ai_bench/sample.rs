//! 样本集：`letters  before  after  expected  category` 五列（可有第六列备注），`#` 开头为注释。

use std::path::Path;

use super::BenchError;

pub struct Sample {
    /// 敲的字母。
    pub letters: String,

    /// 光标前的文字。
    pub before: String,

    /// 光标后的文字。
    pub after: String,

    /// 用户真正想打的字。
    pub expected: String,

    /// `typo` 错拼 / `clean` 正确拼音 / `context` 上下文同音。
    pub category: String,
}

/// 读样本；`limit` 不为 0 时每个类别最多取这么多条（省接口费用）。
pub fn load(path: &Path, limit: usize) -> Result<Vec<Sample>, BenchError> {
    let text = std::fs::read_to_string(path)?;
    let mut samples = Vec::new();
    let mut per_category: Vec<(String, usize)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            return Err(BenchError::Sample {
                line: index + 1,
                reason: "need 5 tab-separated columns",
            });
        }
        let category = fields[4].trim().to_owned();
        if limit > 0 {
            match per_category.iter_mut().find(|(name, _)| *name == category) {
                Some((_, count)) if *count >= limit => continue,
                Some((_, count)) => *count += 1,
                None => per_category.push((category.clone(), 1)),
            }
        }
        samples.push(Sample {
            letters: fields[0].trim().to_owned(),
            before: fields[1].to_owned(),
            after: fields[2].to_owned(),
            expected: fields[3].trim().to_owned(),
            category,
        });
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_columns_and_applies_the_per_category_limit() {
        let dir = std::env::temp_dir().join("qj-bench-sample-test.tsv");
        std::fs::write(
            &dir,
            "# header\na\t\t\t甲\ttypo\nb\t前\t后\t乙\ttypo\nc\t\t\t丙\tclean\t备注\n",
        )
        .unwrap();
        let all = load(&dir, 0).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[1].before, "前");
        let limited = load(&dir, 1).unwrap();
        assert_eq!(limited.len(), 2);
        std::fs::write(&dir, "only\ttwo\n").unwrap();
        assert!(load(&dir, 0).is_err());
        std::fs::remove_file(&dir).ok();
    }
}
