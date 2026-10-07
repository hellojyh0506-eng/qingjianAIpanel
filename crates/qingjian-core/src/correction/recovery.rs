//! 拼音容错与软切分恢复 (Pinyin Error Recovery & Soft Lattice)
//!
//! 针对用户快速输入中的：
//! 1. 噪声 apostrophe 与破碎单辅音/单字母 (如: `si'n'g` -> `sing`, `xi'm'g` -> `ximg`)
//! 2. 声母与韵母音似/形似/键邻混淆 (如: `s` <-> `x`, `m` <-> `n`, `sing` -> `xing`, `ximg` -> `xing`)
//! 3. 编辑距离软约束 (Levenshtein distance <= 1)
//! 恢复出高置信度的拼音音节切分假说与带代价的 Segmentation。

use crate::parser::{self, Segmentation};

/// 尝试从可能带有噪声分隔符、错字母、声母韵母混淆的原始输入中恢复合法的拼音音节切分。
/// 返回 (恢复后的切分, 恢复代价, 恢复后的拼音串)。
pub fn recover_noisy_pinyin(raw: &str) -> Option<(Segmentation, f64, String)> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.contains('\'') {
        return None;
    }

    // 1. 如果包含 apostrophe，先进行音节碎片合并
    let raw_parts: Vec<&str> = raw.split('\'').filter(|s| !s.is_empty()).collect();
    if raw_parts.is_empty() {
        return None;
    }

    // 如果所有切片都是单字母声母 (如 w'z'd'f'l, n's'b, z'j'd'l)，这是简拼/缩写序列，绝非噪声切分
    if raw_parts.iter().all(|p| p.len() == 1 && parser::INITIALS.contains(p)) {
        return None;
    }

    let mut merged_tokens: Vec<String> = Vec::new();
    let mut current_chunk = String::new();

    for part in raw_parts {
        // 只有当前 part 是孤立的尾音辅音 (n, g, m, r)，且前一个 chunk 必须以元音或鼻音结尾时，才能合并为尾音
        // 注意: h 纯属声母，绝非尾音；且辅音绝不能附加到无元音的孤立声母后 (如 x + h + n)
        let is_coda = match part {
            "n" | "m" | "r" => {
                current_chunk.chars().last().map(|c| "aeiouüv".contains(c)).unwrap_or(false)
            }
            "g" => {
                let last = current_chunk.chars().last();
                let is_vowel = last.map(|c| "aeiouüv".contains(c)).unwrap_or(false);
                let is_nasal = last.map(|c| c == 'n' || c == 'm').unwrap_or(false);
                is_vowel || is_nasal
            }
            _ => false,
        };

        if is_coda && !current_chunk.is_empty() {
            current_chunk.push_str(part);
        } else {
            if !current_chunk.is_empty() {
                merged_tokens.push(std::mem::take(&mut current_chunk));
            }
            current_chunk.push_str(part);
        }
    }
    if !current_chunk.is_empty() {
        merged_tokens.push(current_chunk);
    }

    // 2. 针对每个 token 尝试音节修复 (包含混淆矩阵与键邻距离)
    let mut recovered_syllables: Vec<String> = Vec::with_capacity(merged_tokens.len());
    let mut total_cost = 0.0;
    let mut any_recovered = false;

    for token in &merged_tokens {
        if parser::is_syllable(token) {
            recovered_syllables.push(token.clone());
        } else if let Some((best_syllable, cost)) = repair_syllable(token) {
            recovered_syllables.push(best_syllable);
            total_cost += cost;
            any_recovered = true;
        } else {
            // 无法修复成合法音节
            return None;
        }
    }

    if !any_recovered && !raw.contains('\'') {
        // 全是原样音节且无切分碎片，无需纠偏
        return None;
    }

    let joined = recovered_syllables.join("'");
    let segmentation = crate::correction::complete_segmentation(&joined)?;
    Some((segmentation, total_cost, joined))
}

/// 针对单个未知/破损音节寻找最优合法音节修复。
pub fn repair_syllable(token: &str) -> Option<(String, f64)> {
    if parser::is_syllable(token) {
        return Some((token.to_string(), 0.0));
    }

    // 1. 优先尝试高频混淆与键邻变换 (Cost = 1.0)
    // A. 尾部/内部 m <-> n (键盘相邻键)
    if token.contains('m') {
        let candidate = token.replace('m', "n");
        if parser::is_syllable(&candidate) {
            return Some((candidate, 1.0));
        }
        // 如果替换 m->n 后再试 s->x (如 ximg -> xing)
        if candidate.starts_with('s') {
            let mut cand_sx = candidate.clone();
            cand_sx.replace_range(..1, "x");
            if parser::is_syllable(&cand_sx) {
                return Some((cand_sx, 1.5));
            }
        }
    }

    // B. 声母 s <-> x (平舌与舌面音混淆，如 sing -> xing)
    if token.starts_with('s') {
        let mut cand = token.to_string();
        cand.replace_range(..1, "x");
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    } else if token.starts_with('x') {
        let mut cand = token.to_string();
        cand.replace_range(..1, "s");
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    }

    // C. 鼻音混淆 in <-> ing, en <-> eng, an <-> ang
    if token.ends_with("ing") {
        let cand = format!("{}in", &token[..token.len() - 3]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    } else if token.ends_with("in") {
        let cand = format!("{}ing", &token[..token.len() - 2]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    }

    // D. 平翘舌 z <-> zh, c <-> ch, s <-> sh
    if token.starts_with("zh") {
        let cand = format!("z{}", &token[2..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    } else if token.starts_with('z') {
        let cand = format!("zh{}", &token[1..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    }
    if token.starts_with("ch") {
        let cand = format!("c{}", &token[2..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    } else if token.starts_with('c') {
        let cand = format!("ch{}", &token[1..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    }
    if token.starts_with("sh") {
        let cand = format!("s{}", &token[2..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    } else if token.starts_with('s') {
        let cand = format!("sh{}", &token[1..]);
        if parser::is_syllable(&cand) {
            return Some((cand, 1.0));
        }
    }

    // 2. 次选：编辑距离为 1 的合法音节 (必须长度 >= 2，单字母不能随意被 Levenshtein 替换为无关音节)
    if token.len() >= 2 {
        for &valid in parser::SYLLABLES {
            if levenshtein(token, valid) == 1 {
                return Some((valid.to_string(), 2.0));
            }
        }
    }

    None
}

/// 计算两个 ASCII 字符串的莱文斯坦编辑距离。
fn levenshtein(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let len_a = a_bytes.len();
    let len_b = b_bytes.len();
    if len_a.abs_diff(len_b) > 1 {
        return 2;
    }
    let mut prev = (0..=len_b).collect::<Vec<_>>();
    let mut curr = vec![0; len_b + 1];
    for i in 1..=len_a {
        curr[0] = i;
        for j in 1..=len_b {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev.copy_from_slice(&curr);
    }
    prev[len_b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repair_syllables() {
        assert_eq!(repair_syllable("sing"), Some(("xing".to_string(), 1.0)));
        assert_eq!(repair_syllable("ximg"), Some(("xing".to_string(), 1.0)));
        assert_eq!(repair_syllable("zem"), Some(("zen".to_string(), 1.0)));
    }

    #[test]
    fn test_recover_noisy_pinyin_ni_dao_di_si_n_g_bu_xi_m_g() {
        let raw = "ni'dao'di'si'n'g'bu'xi'm'g";
        let res = recover_noisy_pinyin(raw);
        assert!(res.is_some(), "Should recover noisy pinyin lattice");
        let (seg, cost, joined) = res.unwrap();
        assert_eq!(joined, "ni'dao'di'xing'bu'xing");
        assert_eq!(seg.syllables.len(), 6);
        assert!(cost <= 3.0);
    }

    #[test]
    fn test_abbreviations_not_treated_as_noisy() {
        assert_eq!(recover_noisy_pinyin("wo'x'h'n"), None);
        assert_eq!(recover_noisy_pinyin("w'z'd'f'l"), None);
        assert_eq!(recover_noisy_pinyin("z'j'd'l"), None);
        assert_eq!(recover_noisy_pinyin("t'l'p'l"), None);
        assert_eq!(recover_noisy_pinyin("n's'b"), None);
        assert_eq!(recover_noisy_pinyin("b's'b"), None);
        assert_eq!(recover_noisy_pinyin("w'f'l"), None);
    }
}
