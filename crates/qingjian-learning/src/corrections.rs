//! 个人错字与高频混淆词学习器 (Personal Correction & Confusion Hardening)。
//!
//! 支持三个不同生命周期状态：
//! - `Observed`: 仅被观察到，尚未确认，不进行过度偏置，绝不强化错误；
//! - `Corrected`: 用户曾发生选错并主动退格更正（例如把 "的" 更正为 "得"）；
//! - `Learned`: 用户在同一上下文中多次稳定使用或多次纠正确认，已固化为个人习惯。
//!
//! 关键防学坏防错原则：
//! 1. 错误输入（未退格或用户偶然选错）绝不会直接被提升为高权重标准答案；
//! 2. 只有发生主动退格撤销重选（`apply_retraction`）时，才记录更正意图；
//! 3. 针对高频易混淆集合（如 他/她/它、在/再、的/地/得、做/作、哪/那），结合上下文进行个性化提权/降权；
//! 4. 数据完全本地保存（`user-corrections.tsv`），支持用户查看、清空与重置，私密输入下绝对隔离。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use qingjian_core::storage::{read_text_lossy, write_atomic_str};

use crate::error::LearningError;

/// 已知高频混淆词集合
pub const CONFUSION_SETS: &[&[&str]] = &[
    &["他", "她", "它"],
    &["在", "再"],
    &["的", "地", "得"],
    &["做", "作"],
    &["哪", "那"],
    &["像", "象"],
    &["纸张", "智障"],
    &["傻子", "啥子", "沙子"],
    &["微信输入法", "为新输入法"],
];

/// 判断两个词是否属于已知高频混淆集合
pub fn is_known_confusion_pair(a: &str, b: &str) -> bool {
    CONFUSION_SETS
        .iter()
        .any(|set| set.contains(&a) && set.contains(&b))
}

/// 个人纠错与输入习惯的三个生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PersonalCorrectionState {
    /// 仅观察到输入，未确认，不予偏置加权
    Observed,
    /// 用户曾发生选错并主动退格更正
    Corrected,
    /// 用户多次确认正确使用，已固化为个人可靠偏好
    Learned,
}

impl PersonalCorrectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observed => "Observed",
            Self::Corrected => "Corrected",
            Self::Learned => "Learned",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "Learned" => Self::Learned,
            "Corrected" => Self::Corrected,
            _ => Self::Observed,
        }
    }
}

/// 一条个性化纠错规则
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionRule {
    /// 触发上下文（光标前文或前词，例如 "跑"、"现居"、"她"）
    pub context: String,
    /// 误选或被更正掉的候选词（例如 "的"、"再"、"他"）
    pub mistyped: String,
    /// 用户真正期望并更正后的候选词（例如 "得"、"在"、"她"）
    pub corrected: String,
    /// 当前状态：Observed -> Corrected -> Learned
    pub state: PersonalCorrectionState,
    /// 用户主动撤销更正的次数
    pub corrections_count: u32,
    /// 用户在更正后，后续直接正确选择并确认的次数
    pub confirmed_count: u32,
    /// 最近更新时间
    pub last_updated: String,
}

/// 内存中的个人更正与防混淆追踪器
#[derive(Debug, Default, Clone)]
pub struct PersonalCorrectionTracker {
    /// (上下文, 误选词, 目标词) -> 规则
    rules: BTreeMap<(String, String, String), CorrectionRule>,
    dirty: bool,
}

impl PersonalCorrectionTracker {
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn rules(&self) -> Vec<&CorrectionRule> {
        self.rules.values().collect()
    }

    /// 记录一次错字/同音混淆更正（由 `apply_retraction` 触发）
    pub fn record_correction(&mut self, context: &str, mistyped: &str, corrected: &str) {
        let ctx = context.trim();
        let m = mistyped.trim();
        let c = corrected.trim();
        if m.is_empty() || c.is_empty() || m == c {
            return;
        }

        self.dirty = true;
        let today = jiff::civil::date(2026, 10, 7).to_string();
        let key = (ctx.to_owned(), m.to_owned(), c.to_owned());

        let rule = self.rules.entry(key).or_insert_with(|| CorrectionRule {
            context: ctx.to_owned(),
            mistyped: m.to_owned(),
            corrected: c.to_owned(),
            state: PersonalCorrectionState::Observed,
            corrections_count: 0,
            confirmed_count: 0,
            last_updated: today.clone(),
        });

        rule.corrections_count = rule.corrections_count.saturating_add(1);
        rule.last_updated = today;

        // 状态跃迁：
        // 第一次主动更正 -> Corrected
        // 更正达到 2 次，或者更正 1 次 + 后续确认 >= 2 次 -> Learned
        if rule.corrections_count >= 2
            || (rule.corrections_count >= 1 && rule.confirmed_count >= 2)
        {
            rule.state = PersonalCorrectionState::Learned;
        } else {
            rule.state = PersonalCorrectionState::Corrected;
        }
    }

    /// 用户在某上下文下正常提交了某个词且未撤销；若属于已知更正规则的目标词，确认其正确使用
    pub fn confirm_choice(&mut self, context: &str, choice: &str) {
        let ctx = context.trim();
        let c = choice.trim();
        if c.is_empty() {
            return;
        }

        for rule in self.rules.values_mut() {
            if rule.corrected == c && (rule.context.is_empty() || ctx.ends_with(&rule.context)) {
                rule.confirmed_count = rule.confirmed_count.saturating_add(1);
                self.dirty = true;
                if rule.corrections_count >= 1 && rule.confirmed_count >= 2 {
                    rule.state = PersonalCorrectionState::Learned;
                }
            }
        }
    }

    /// 根据前文与候选词，计算个性化得分调整：
    /// - 目标正确词：Corrected 给 +3.0，Learned 给 +5.0；
    /// - 误选词：Corrected 给 -3.0，Learned 给 -5.0；
    /// - Observed 状态：给 0.0（不提前偏置，防止强化错误）。
    pub fn adjustment(&self, before: Option<&str>, candidate: &str) -> f64 {
        let Some(before_text) = before else {
            return 0.0;
        };
        if before_text.is_empty() || candidate.is_empty() {
            return 0.0;
        }

        let mut total_adj = 0.0;
        for rule in self.rules.values() {
            if !rule.context.is_empty() && !before_text.ends_with(&rule.context) {
                continue;
            }

            if candidate == rule.corrected {
                match rule.state {
                    PersonalCorrectionState::Learned => total_adj += 5.0,
                    PersonalCorrectionState::Corrected => total_adj += 3.0,
                    PersonalCorrectionState::Observed => {}
                }
            } else if candidate == rule.mistyped {
                match rule.state {
                    PersonalCorrectionState::Learned => total_adj -= 5.0,
                    PersonalCorrectionState::Corrected => total_adj -= 3.0,
                    PersonalCorrectionState::Observed => {}
                }
            }
        }
        total_adj
    }

    /// 遗忘一条更正规则
    pub fn forget(&mut self, context: &str, mistyped: &str, corrected: &str) -> bool {
        let key = (context.to_owned(), mistyped.to_owned(), corrected.to_owned());
        if self.rules.remove(&key).is_some() {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// 清空所有个人更正数据
    pub fn clear(&mut self) {
        self.rules.clear();
        self.dirty = true;
    }

    /// 从 TSV 字符串加载
    pub fn load_tsv(&mut self, source: &str) -> usize {
        let mut loaded = 0;
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let Some(context) = fields.next() else { continue };
            let Some(mistyped) = fields.next() else { continue };
            let Some(corrected) = fields.next() else { continue };
            let state_str = fields.next().unwrap_or("Observed");
            let corrections = fields
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let confirmed = fields
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let last_updated = fields.next().unwrap_or("2026-10-07").to_owned();

            let state = PersonalCorrectionState::parse(state_str);
            let key = (context.to_owned(), mistyped.to_owned(), corrected.to_owned());
            self.rules.insert(
                key,
                CorrectionRule {
                    context: context.to_owned(),
                    mistyped: mistyped.to_owned(),
                    corrected: corrected.to_owned(),
                    state,
                    corrections_count: corrections,
                    confirmed_count: confirmed,
                    last_updated,
                },
            );
            loaded += 1;
        }
        self.dirty = false;
        loaded
    }

    /// 导出为 TSV
    pub fn to_tsv(&self) -> String {
        let mut text = String::from(
            "# 青简个人纠错与偏好规则：上下文\t误选词\t更正词\t状态\t更正次数\t确认次数\t最近更新\n",
        );
        for rule in self.rules.values() {
            let _ = writeln!(
                text,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                rule.context,
                rule.mistyped,
                rule.corrected,
                rule.state.as_str(),
                rule.corrections_count,
                rule.confirmed_count,
                rule.last_updated
            );
        }
        text
    }

    /// 保存到文件
    pub fn save_to(&mut self, path: &Path) -> Result<(), LearningError> {
        let text = self.to_tsv();
        write_atomic_str(path, &text)?;
        self.dirty = false;
        Ok(())
    }

    /// 从文件打开
    pub fn from_path(path: &Path) -> Self {
        let mut tracker = Self::default();
        if let Ok(Some(text)) = read_text_lossy(path) {
            tracker.load_tsv(&text);
        }
        tracker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_state_progression_and_anti_corruption() {
        let mut tracker = PersonalCorrectionTracker::default();

        // 初始无规则
        assert_eq!(tracker.adjustment(Some("跑"), "得"), 0.0);
        assert_eq!(tracker.adjustment(Some("跑"), "的"), 0.0);

        // 1. 用户第一次在 "跑" 后误选了 "的"，随后退格更正为 "得"
        tracker.record_correction("跑", "的", "得");
        assert_eq!(tracker.len(), 1);

        let rules = tracker.rules();
        assert_eq!(rules[0].state, PersonalCorrectionState::Corrected);
        assert_eq!(rules[0].corrections_count, 1);
        assert_eq!(rules[0].confirmed_count, 0);

        // 进入 Corrected 状态：得 +3.0，的 -3.0
        assert_eq!(tracker.adjustment(Some("跑"), "得"), 3.0);
        assert_eq!(tracker.adjustment(Some("跑"), "的"), -3.0);
        // 无关上下文不受影响
        assert_eq!(tracker.adjustment(Some("看"), "得"), 0.0);

        // 2. 用户后续在 "跑" 后直接确认选择了 "得"
        tracker.confirm_choice("跑", "得");
        assert_eq!(tracker.rules()[0].confirmed_count, 1);
        // 仍为 Corrected（需要 confirmed >= 2 或 corrections >= 2）
        assert_eq!(tracker.rules()[0].state, PersonalCorrectionState::Corrected);

        // 再次确认选择 "得"
        tracker.confirm_choice("跑", "得");
        assert_eq!(tracker.rules()[0].confirmed_count, 2);
        // 跃迁至 Learned 状态！
        assert_eq!(tracker.rules()[0].state, PersonalCorrectionState::Learned);
        assert_eq!(tracker.adjustment(Some("跑"), "得"), 5.0);
        assert_eq!(tracker.adjustment(Some("跑"), "的"), -5.0);

        // 3. 防学坏验证：如果用户直接输入不合语法的 "跑" + "的"，且未发生退格更正
        // confirm_choice 绝不会为 "的" 创建规则或提升 "的"
        tracker.confirm_choice("跑", "的");
        // 规则依然只有 1 条，且状态是 "得" 获益
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.rules()[0].corrected, "得");
        assert_eq!(tracker.rules()[0].mistyped, "的");
    }

    #[test]
    fn test_tsv_round_trip() {
        let mut tracker = PersonalCorrectionTracker::default();
        tracker.record_correction("现居", "再", "在");
        tracker.confirm_choice("现居", "在");
        tracker.confirm_choice("现居", "在");

        let tsv = tracker.to_tsv();
        assert!(tsv.contains("现居\t再\t在\tLearned\t1\t2"));

        let mut loaded = PersonalCorrectionTracker::default();
        loaded.load_tsv(&tsv);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.adjustment(Some("现居"), "在"), 5.0);
        assert_eq!(loaded.adjustment(Some("现居"), "再"), -5.0);
    }
}
