use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use qingjian_core::storage::{read_text_lossy, write_atomic_str};
use qingjian_core::{
    ComparisonResult, ComparisonStatus, ErrorMajorCategory, ErrorSubcategory, FeedbackItem,
    Language, RecallAttempt, RecallItem, RecallOutcome, RecallTracker,
};

use crate::error::LearningError;

/// 主动回忆条目的落盘：一行 `提示(中文)\t目标(外语)\t语言\t曝光次数\t成功次数\t失败次数\t查看答案次数\t最近复习\t创建日期`
#[derive(Debug, Default)]
pub struct RecallBook {
    /// (语言代码, 提示, 目标) -> 条目
    entries: BTreeMap<(String, String, String), RecallEntry>,
    dirty: bool,
    path: Option<PathBuf>,
}

use crate::corrections::PersonalCorrectionState;

/// 一条主动回忆条目的详细记录
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallEntry {
    pub seen: u32,
    pub success: u32,
    pub failure: u32,
    pub revealed: u32,
    pub last_reviewed: Date,
    pub created_at: Date,
    pub preferred_alternative: Option<String>,
    pub state: PersonalCorrectionState,
    pub alternative_counts: BTreeMap<String, u32>,
}

impl RecallEntry {
    fn new(date: Date) -> Self {
        Self {
            seen: 1,
            success: 0,
            failure: 0,
            revealed: 0,
            last_reviewed: date,
            created_at: date,
            preferred_alternative: None,
            state: PersonalCorrectionState::Observed,
            alternative_counts: BTreeMap::new(),
        }
    }
}

impl RecallBook {
    /// 从文件加载（不存在就从零开始，flush 时保存）；读不了就退回只在内存中记。
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        match read_text_lossy(&path) {
            Ok(text) => Self {
                entries: text.as_deref().map(parse).unwrap_or_default(),
                dirty: false,
                path: Some(path),
            },
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "主动回忆记录读不了，本次只在内存中记");
                Self::default()
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 记一次被动曝光（上屏带释义的中文词）
    pub fn record_exposure_on(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        date: Date,
    ) {
        if prompt.is_empty() || target.is_empty() {
            return;
        }
        self.dirty = true;
        let key = (language.code().to_owned(), prompt.to_owned(), target.to_owned());
        self.entries
            .entry(key)
            .and_modify(|e| {
                e.seen = e.seen.saturating_add(1);
                e.last_reviewed = date;
            })
            .or_insert_with(|| RecallEntry::new(date));
    }

    /// 记一次主动回忆测试结果
    pub fn record_recall_on(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
        date: Date,
    ) {
        self.record_recall_with_input_on(language, prompt, target, outcome, "", date);
    }

    /// 记一次主动回忆测试结果，附带用户实际输入的英文（成功时用于记录自然同义替代表达，状态跃迁）
    pub fn record_recall_with_input_on(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
        user_input: &str,
        date: Date,
    ) {
        if prompt.is_empty() || target.is_empty() {
            return;
        }
        self.dirty = true;
        let key = (language.code().to_owned(), prompt.to_owned(), target.to_owned());
        let entry = self
            .entries
            .entry(key)
            .or_insert_with(|| RecallEntry::new(date));
        entry.last_reviewed = date;

        match outcome {
            RecallOutcome::Success => {
                entry.success = entry.success.saturating_add(1);
                let trimmed_input = user_input.trim();
                // 仅当用户输入的英文通过了自然口语判定时，才允许记录为认可的替代表达（防学坏）
                if !trimmed_input.is_empty()
                    && Self::evaluate_with_prompt(trimmed_input, target, Some(prompt))
                {
                    let count = entry
                        .alternative_counts
                        .entry(trimmed_input.to_owned())
                        .or_default();
                    *count = count.saturating_add(1);

                    // 找出使用次数最多的替代表达
                    let top_alt = entry
                        .alternative_counts
                        .iter()
                        .max_by_key(|(_, cnt)| *cnt)
                        .map(|(alt, _)| alt.clone());

                    if let Some(top) = top_alt {
                        let top_count = entry.alternative_counts.get(&top).copied().unwrap_or(0);
                        if top_count >= 2 {
                            entry.preferred_alternative = Some(top);
                            entry.state = PersonalCorrectionState::Learned;
                        }
                    }
                }
                // 若掌握度稳固，状态进入 Learned
                if entry.success >= 3 {
                    entry.state = PersonalCorrectionState::Learned;
                } else if entry.state == PersonalCorrectionState::Observed {
                    entry.state = PersonalCorrectionState::Corrected;
                }
            }
            RecallOutcome::Failure => {
                entry.failure = entry.failure.saturating_add(1);
                // 失败输入绝不进入 alternative_counts（防学坏核心）
            }
            RecallOutcome::Revealed => {
                entry.revealed = entry.revealed.saturating_add(1);
            }
        }
    }

    /// 记一次主动回忆 Attempt 尝试
    pub fn record_attempt_on(
        &mut self,
        language: Language,
        attempt: &RecallAttempt,
        date: Date,
    ) {
        let input = attempt.user_attempt.trim();
        if input.is_empty() {
            return;
        }
        let outcome = match attempt.comparison.status {
            Some(ComparisonStatus::ExactMatch) | Some(ComparisonStatus::ValidB2Upgrade) => {
                RecallOutcome::Success
            }
            Some(ComparisonStatus::NaturalBetter) => {
                if Self::evaluate_with_prompt(input, &attempt.reference_expression, Some(&attempt.prompt)) {
                    RecallOutcome::Success
                } else {
                    RecallOutcome::Failure
                }
            }
            Some(ComparisonStatus::GrammarIssue) | Some(ComparisonStatus::Chinglish) => {
                RecallOutcome::Failure
            }
            _ => return,
        };
        self.record_recall_with_input_on(
            language,
            &attempt.prompt,
            &attempt.reference_expression,
            outcome,
            input,
            date,
        );
    }

    /// 查询该词条的首选自然替代表达（仅在达到 Learned 状态时生效）
    pub fn preferred_alternative_for(
        &self,
        language: Language,
        prompt: &str,
        target: &str,
    ) -> Option<String> {
        let key = (language.code().to_owned(), prompt.to_owned(), target.to_owned());
        let entry = self.entries.get(&key)?;
        if entry.state == PersonalCorrectionState::Learned {
            entry.preferred_alternative.clone()
        } else {
            None
        }
    }

    /// 高频曝光条目 (seen >= min_seen)
    pub fn frequently_exposed_items(&self, language: Language, min_seen: u32) -> Vec<RecallItem> {
        let code = language.code();
        self.entries
            .iter()
            .filter(|((c, _, _), entry)| c == code && entry.seen >= min_seen)
            .map(|((_, p, t), e)| RecallItem {
                prompt: p.clone(),
                target: t.clone(),
                language,
                seen: e.seen,
                success: e.success,
                failure: e.failure,
                revealed: e.revealed,
            })
            .collect()
    }

    /// 高频查看答案卡点条目 (revealed >= min_revealed)
    pub fn frequently_revealed_items(
        &self,
        language: Language,
        min_revealed: u32,
    ) -> Vec<RecallItem> {
        let code = language.code();
        self.entries
            .iter()
            .filter(|((c, _, _), entry)| c == code && entry.revealed >= min_revealed)
            .map(|((_, p, t), e)| RecallItem {
                prompt: p.clone(),
                target: t.clone(),
                language,
                seen: e.seen,
                success: e.success,
                failure: e.failure,
                revealed: e.revealed,
            })
            .collect()
    }

    /// 记忆困难/需要加强条目 (失败 > 成功 或 查看答案 >= 2)
    pub fn struggling_items(&self, language: Language) -> Vec<RecallItem> {
        let code = language.code();
        self.entries
            .iter()
            .filter(|((c, _, _), entry)| {
                c == code && (entry.failure > entry.success || entry.revealed >= 2)
            })
            .map(|((_, p, t), e)| RecallItem {
                prompt: p.clone(),
                target: t.clone(),
                language,
                seen: e.seen,
                success: e.success,
                failure: e.failure,
                revealed: e.revealed,
            })
            .collect()
    }

    /// 已完全掌握条目 (state == Learned)
    pub fn learned_items(&self, language: Language) -> Vec<RecallItem> {
        let code = language.code();
        self.entries
            .iter()
            .filter(|((c, _, _), entry)| {
                c == code && entry.state == PersonalCorrectionState::Learned
            })
            .map(|((_, p, t), e)| RecallItem {
                prompt: p.clone(),
                target: t.clone(),
                language,
                seen: e.seen,
                success: e.success,
                failure: e.failure,
                revealed: e.revealed,
            })
            .collect()
    }

    /// 清空所有主动回忆记录
    pub fn clear(&mut self) {
        self.entries.clear();
        self.dirty = true;
    }

    /// 确定性回忆判定（不依赖 LLM，支持大小写归一化、去除首尾标点空格、分号/斜杠多义项分割、缩写扩展等价、自然美式口语等价簇）
    pub fn evaluate_with_prompt(input: &str, target: &str, prompt: Option<&str>) -> bool {
        let norm_input = normalize(input);
        if norm_input.is_empty() {
            return false;
        }
        let norm_target = normalize(target);
        if norm_input == norm_target {
            return true;
        }
        for part in target.split([';', '/', ',']) {
            let norm_part = normalize(part);
            if norm_input == norm_part {
                return true;
            }
            if let Some(stripped) = norm_part.strip_prefix("to ") {
                if norm_input == stripped.trim() {
                    return true;
                }
            }
            if matches_colloquial_cluster(&norm_input, &norm_part) {
                return true;
            }
        }
        if matches_colloquial_cluster(&norm_input, &norm_target) {
            return true;
        }
        if let Some(p) = prompt {
            if matches_intent_cluster(&norm_input, p) {
                return true;
            }
        }
        false
    }

    /// 向后兼容的单目标回忆判定
    pub fn evaluate(input: &str, target: &str) -> bool {
        Self::evaluate_with_prompt(input, target, None)
    }

    /// 评估用户的主动回忆尝试并生成结构化的 Attempt 与对比结果
    pub fn evaluate_attempt(prompt: &str, user_attempt: &str, reference: &str) -> RecallAttempt {
        let trimmed = user_attempt.trim();
        let timestamp = jiff::Zoned::now().timestamp().as_second();

        if trimmed.is_empty() {
            return RecallAttempt {
                prompt: prompt.to_string(),
                user_attempt: String::new(),
                reference_expression: reference.to_string(),
                timestamp,
                comparison: ComparisonResult {
                    status: Some(ComparisonStatus::EmptyInput),
                    key_advice: "请输入英文尝试进行对比".to_string(),
                    user_formatted: String::new(),
                    natural_expression: reference.to_string(),
                    primary_category: None,
                    feedback_items: Vec::new(),
                    upgrade: None,
                    dimensions: None,
                },
                error_categories: Vec::new(),
            };
        }

        // 1. Exact match / colloquial cluster match (Zero Error, Anti-Overcorrection)
        if Self::evaluate_with_prompt(trimmed, reference, Some(prompt)) {
            let upgrade = crate::upgrade::find_upgrade(prompt, reference);
            return RecallAttempt {
                prompt: prompt.to_string(),
                user_attempt: trimmed.to_string(),
                reference_expression: reference.to_string(),
                timestamp,
                comparison: ComparisonResult {
                    status: Some(ComparisonStatus::ExactMatch),
                    key_advice: "表达非常地道准确，完全符合母语者习惯！".to_string(),
                    user_formatted: trimmed.to_string(),
                    natural_expression: reference.to_string(),
                    primary_category: None,
                    feedback_items: Vec::new(),
                    upgrade,
                    dimensions: None,
                },
                error_categories: Vec::new(),
            };
        }

        // 2. 预先执行错误分类诊断，甄别语法硬伤与严重中式英语 (Error Taxonomy)
        let feedback_items = analyze_feedback_items(trimmed, reference);
        let has_fatal_issue = feedback_items.iter().any(|f| {
            f.major_category == ErrorMajorCategory::Grammar
                || f.subcategory == ErrorSubcategory::Chinglish
        });

        // 3. B2 → C1 Expression Upgrade 认知辨识与语域识别 (仅在无语法硬伤时生效)
        // 若用户输入属于语法正确的 B2 表达或某一合法语域变体，提供升级建议，严禁判为错误
        if !has_fatal_issue {
            if let Some(upgrade_comp) = crate::upgrade::evaluate_b2_upgrade(prompt, trimmed, reference) {
                let error_categories = if upgrade_comp.status == Some(ComparisonStatus::ValidB2Upgrade) {
                    vec!["b2_upgrade".to_string(), "naturalness".to_string()]
                } else {
                    Vec::new()
                };
                return RecallAttempt {
                    prompt: prompt.to_string(),
                    user_attempt: trimmed.to_string(),
                    reference_expression: reference.to_string(),
                    timestamp,
                    comparison: upgrade_comp,
                    error_categories,
                };
            }
        }

        let primary_category = feedback_items.first().map(|f| f.major_category);

        let (status, key_advice) = if let Some(first) = feedback_items.first() {
            let advice = format!("{}: {}", first.label, first.explanation);
            match first.major_category {
                ErrorMajorCategory::Grammar => (ComparisonStatus::GrammarIssue, advice),
                ErrorMajorCategory::Naturalness if first.subcategory == ErrorSubcategory::Chinglish => {
                    (ComparisonStatus::Chinglish, advice)
                }
                ErrorMajorCategory::Collocation if first.subcategory == ErrorSubcategory::UnnaturalCollocation => {
                    (ComparisonStatus::GrammarIssue, advice)
                }
                _ => (ComparisonStatus::NaturalBetter, advice),
            }
        } else {
            (
                ComparisonStatus::NaturalBetter,
                "语义基本正确，但母语者在日常口语中更倾向于使用该地道表达。".to_string(),
            )
        };

        let mut error_categories = Vec::new();
        for item in &feedback_items {
            let cat_str = format!("{:?}", item.major_category).to_ascii_lowercase();
            if !error_categories.contains(&cat_str) {
                error_categories.push(cat_str);
            }
            let sub_str = format!("{:?}", item.subcategory).to_ascii_lowercase();
            if !error_categories.contains(&sub_str) {
                error_categories.push(sub_str);
            }
        }
        if status == ComparisonStatus::GrammarIssue && !error_categories.contains(&"grammar".to_string()) {
            error_categories.push("grammar".to_string());
        }
        if status == ComparisonStatus::Chinglish && !error_categories.contains(&"chinglish".to_string()) {
            error_categories.push("chinglish".to_string());
        }
        if error_categories.is_empty() {
            error_categories.push("naturalness".to_string());
        }

        let upgrade = crate::upgrade::find_upgrade(prompt, reference);

        RecallAttempt {
            prompt: prompt.to_string(),
            user_attempt: trimmed.to_string(),
            reference_expression: reference.to_string(),
            timestamp,
            comparison: ComparisonResult {
                status: Some(status),
                key_advice,
                user_formatted: trimmed.to_string(),
                natural_expression: reference.to_string(),
                primary_category,
                feedback_items,
                upgrade,
                dimensions: None,
            },
            error_categories,
        }
    }

    /// 获取下一条待复习条目：
    /// 优先级：
    /// 1. 成功次数为 0 且失败或曝光较多的条目；
    /// 2. 失败次数大于成功次数的条目；
    /// 3. 最近复习日期较早的条目。
    pub fn next_item_for(&self, language: Language) -> Option<RecallItem> {
        let code = language.code();
        let mut candidates: Vec<_> = self
            .entries
            .iter()
            .filter(|((c, _, _), _)| c == code)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by_key(|(_, entry)| {
            (
                entry.success > 0,
                entry.success,
                std::cmp::Reverse(entry.failure),
                entry.last_reviewed,
            )
        });
        let ((_, prompt, target), entry) = candidates.first()?;
        Some(RecallItem {
            prompt: (*prompt).clone(),
            target: (*target).clone(),
            language,
            seen: entry.seen,
            success: entry.success,
            failure: entry.failure,
            revealed: entry.revealed,
        })
    }

    pub fn pending_count_for(&self, language: Language) -> usize {
        let code = language.code();
        self.entries
            .iter()
            .filter(|((c, _, _), entry)| c == code && entry.success == 0)
            .count()
    }

    /// 写回文件（没有新记录就跳过）。
    pub fn save(&mut self) -> Result<(), LearningError> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        self.save_to(&path)?;
        self.dirty = false;
        Ok(())
    }

    fn save_to(&self, path: &Path) -> Result<(), LearningError> {
        let mut text = String::from(
            "# prompt\ttarget\tlanguage\tseen\tsuccess\tfailure\trevealed\tlast_reviewed\tcreated_at\tpreferred_alternative\tstate\n",
        );
        for ((code, prompt, target), entry) in &self.entries {
            let pref = entry.preferred_alternative.as_deref().unwrap_or("-");
            let state = entry.state.as_str();
            let _ = writeln!(
                text,
                "{prompt}\t{target}\t{code}\t{}\t{}\t{}\t{}\t{}\t{}\t{pref}\t{state}",
                entry.seen,
                entry.success,
                entry.failure,
                entry.revealed,
                entry.last_reviewed,
                entry.created_at
            );
        }
        write_atomic_str(path, &text)?;
        Ok(())
    }
}

fn canonicalize_contractions(s: &str) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut result = Vec::with_capacity(words.len() * 2);
    for w in words {
        match w {
            "don't" => { result.push("do"); result.push("not"); }
            "doesn't" => { result.push("does"); result.push("not"); }
            "didn't" => { result.push("did"); result.push("not"); }
            "can't" | "cannot" => { result.push("can"); result.push("not"); }
            "won't" => { result.push("will"); result.push("not"); }
            "isn't" => { result.push("is"); result.push("not"); }
            "aren't" => { result.push("are"); result.push("not"); }
            "wasn't" => { result.push("was"); result.push("not"); }
            "weren't" => { result.push("were"); result.push("not"); }
            "haven't" => { result.push("have"); result.push("not"); }
            "hasn't" => { result.push("has"); result.push("not"); }
            "hadn't" => { result.push("had"); result.push("not"); }
            "couldn't" => { result.push("could"); result.push("not"); }
            "wouldn't" => { result.push("would"); result.push("not"); }
            "shouldn't" => { result.push("should"); result.push("not"); }
            "i'm" => { result.push("i"); result.push("am"); }
            "you're" => { result.push("you"); result.push("are"); }
            "he's" => { result.push("he"); result.push("is"); }
            "she's" => { result.push("she"); result.push("is"); }
            "it's" => { result.push("it"); result.push("is"); }
            "we're" => { result.push("we"); result.push("are"); }
            "they're" => { result.push("they"); result.push("are"); }
            "that's" => { result.push("that"); result.push("is"); }
            "what's" => { result.push("what"); result.push("is"); }
            "where's" => { result.push("where"); result.push("is"); }
            "why's" => { result.push("why"); result.push("is"); }
            "how's" => { result.push("how"); result.push("is"); }
            "there's" => { result.push("there"); result.push("is"); }
            "let's" => { result.push("let"); result.push("us"); }
            "i've" => { result.push("i"); result.push("have"); }
            "you've" => { result.push("you"); result.push("have"); }
            "we've" => { result.push("we"); result.push("have"); }
            "they've" => { result.push("they"); result.push("have"); }
            "i'll" => { result.push("i"); result.push("will"); }
            "you'll" => { result.push("you"); result.push("will"); }
            "he'll" => { result.push("he"); result.push("will"); }
            "she'll" => { result.push("she"); result.push("will"); }
            "it'll" => { result.push("it"); result.push("will"); }
            "we'll" => { result.push("we"); result.push("will"); }
            "they'll" => { result.push("they"); result.push("will"); }
            "i'd" => { result.push("i"); result.push("would"); }
            "you'd" => { result.push("you"); result.push("would"); }
            "we'd" => { result.push("we"); result.push("would"); }
            "gonna" => { result.push("going"); result.push("to"); }
            "gotta" => { result.push("got"); result.push("to"); }
            "wanna" => { result.push("want"); result.push("to"); }
            "kinda" => { result.push("kind"); result.push("of"); }
            other => result.push(other),
        }
    }
    result.join(" ")
}

// 14 个高频日常美式口语等价簇（Approved Natural American Spoken English Clusters）
const CLUSTER_DONE: &[&str] = &[
    "got it", "it is done", "we are good", "it is handled", "all set",
    "taken care of", "handled", "done",
];
const CLUSTER_NOT_ARRIVED: &[&str] = &[
    "why are not you here yet", "where are you at", "where are you",
    "what is taking so long", "what is taking you so long",
    "are you on your way", "are you almost here",
];
const CLUSTER_WHATEVER: &[&str] = &[
    "whatever", "up to you", "either is fine", "does not matter to me",
    "does not matter", "i am fine with whatever", "whichever",
];
const CLUSTER_NO_WORRIES: &[&str] = &[
    "no worries", "it is all good", "you are fine", "do not worry about it",
    "no problem", "it is nothing", "it is okay", "all good", "anytime",
    "do not mention it",
];
const CLUSTER_I_AM_DONE: &[&str] = &[
    "i can not even", "i am done", "i give up", "unbelievable", "are you serious",
];
const CLUSTER_REALLY: &[&str] = &[
    "really", "for real", "wait really", "seriously", "are you for real", "no way",
];
const CLUSTER_ARE_YOU_SERIOUS: &[&str] = &[
    "are you serious", "are you for real", "you serious", "are you kidding me",
    "you are kidding right",
];
const CLUSTER_HARD_TO_EXPLAIN: &[&str] = &[
    "i do not know how to put it", "i am not sure how to say it",
    "i do not even know what to say", "hard to explain", "i do not know what to say",
];
const CLUSTER_HEADING_OUT: &[&str] = &[
    "i gotta run", "i got to run", "i am heading out", "i am gonna head out",
    "i am going to head out", "i gotta go", "i got to go", "i am taking off", "gotta run",
];
const CLUSTER_TALK_LATER: &[&str] = &[
    "let us talk later", "we will talk later", "talk later",
    "let us deal with it later", "later",
];
const CLUSTER_CRAZY: &[&str] = &[
    "that is insane", "that is crazy", "that is ridiculous", "that is out of hand", "that is wild",
];
const CLUSTER_CONFUSED: &[&str] = &[
    "i am a little confused", "i am lost", "wait what", "i am totally lost", "a little lost",
];
const CLUSTER_NEVER_MIND: &[&str] = &[
    "never mind", "forget it", "leave it", "drop it", "let it go",
];
const CLUSTER_FAIR_ENOUGH: &[&str] = &[
    "fair enough", "alright then", "all right then", "fine", "works for me",
    "if you say so", "alright", "all right",
];

const ALL_CLUSTERS: &[&[&str]] = &[
    CLUSTER_DONE,
    CLUSTER_NOT_ARRIVED,
    CLUSTER_WHATEVER,
    CLUSTER_NO_WORRIES,
    CLUSTER_I_AM_DONE,
    CLUSTER_REALLY,
    CLUSTER_ARE_YOU_SERIOUS,
    CLUSTER_HARD_TO_EXPLAIN,
    CLUSTER_HEADING_OUT,
    CLUSTER_TALK_LATER,
    CLUSTER_CRAZY,
    CLUSTER_CONFUSED,
    CLUSTER_NEVER_MIND,
    CLUSTER_FAIR_ENOUGH,
];

fn matches_colloquial_cluster(input: &str, target: &str) -> bool {
    ALL_CLUSTERS.iter().any(|list| list.contains(&input) && list.contains(&target))
}

fn matches_intent_cluster(input: &str, prompt: &str) -> bool {
    if prompt.contains("搞定") || prompt.contains("处理好") || prompt.contains("办好") {
        return CLUSTER_DONE.contains(&input);
    }
    if prompt.contains("你咋还不来") || prompt.contains("你怎么还没来") || prompt.contains("怎么还不来") {
        return CLUSTER_NOT_ARRIVED.contains(&input);
    }
    if prompt.contains("随便") {
        return CLUSTER_WHATEVER.contains(&input);
    }
    if prompt.contains("没事") || prompt.contains("没关系") {
        return CLUSTER_NO_WORRIES.contains(&input);
    }
    if prompt.contains("我服了") || prompt.contains("服了") {
        return CLUSTER_I_AM_DONE.contains(&input);
    }
    if prompt.contains("真的假的") {
        return CLUSTER_REALLY.contains(&input);
    }
    if prompt.contains("你认真的吗") || prompt.contains("你当真") {
        return CLUSTER_ARE_YOU_SERIOUS.contains(&input);
    }
    if prompt.contains("不知道咋说") || prompt.contains("不知怎么说") || prompt.contains("不知道怎么说") {
        return CLUSTER_HARD_TO_EXPLAIN.contains(&input);
    }
    if prompt.contains("我先走了") || prompt.contains("我先撤了") {
        return CLUSTER_HEADING_OUT.contains(&input);
    }
    if prompt.contains("晚点再说") || prompt.contains("回头再说") {
        return CLUSTER_TALK_LATER.contains(&input);
    }
    if prompt.contains("太离谱了") || prompt.contains("离谱") {
        return CLUSTER_CRAZY.contains(&input);
    }
    if prompt.contains("有点懵") || prompt.contains("懵了") {
        return CLUSTER_CONFUSED.contains(&input);
    }
    if prompt.contains("算了") {
        return CLUSTER_NEVER_MIND.contains(&input);
    }
    if prompt.contains("行吧") || prompt.contains("好吧") {
        return CLUSTER_FAIR_ENOUGH.contains(&input);
    }
    false
}

/// 依据统一 Error Taxonomy 结构化评测用户的英文表达
pub fn analyze_feedback_items(input: &str, reference: &str) -> Vec<FeedbackItem> {
    let mut items = Vec::new();
    let lower_in = input.to_ascii_lowercase();
    let lower_ref = reference.to_ascii_lowercase();

    // ==========================================
    // 1. Grammar (时态、介词、冠词、语序、主谓一致、单复数)
    // ==========================================
    // 1.1 Tense: yesterday with present tense
    if lower_in.contains("yesterday") {
        if lower_in.contains("i'm")
            || lower_in.contains("i am")
            || lower_in.contains(" is ")
            || lower_in.contains(" are ")
            || lower_in.contains(" am ")
        {
            items.push(FeedbackItem {
                major_category: ErrorMajorCategory::Grammar,
                subcategory: ErrorSubcategory::Tense,
                label: "时态冲突".to_string(),
                explanation: "时间状语 'yesterday' 表示过去，谓语动词应使用过去时（如 was / were）。".to_string(),
                suggestion: Some("was / were".to_string()),
            });
        }
    }
    // 1.2 Preposition: "depend of"
    if lower_in.contains("depend of")
        || lower_in.contains("depends of")
        || lower_in.contains("depended of")
    {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Grammar,
            subcategory: ErrorSubcategory::Preposition,
            label: "介词错误".to_string(),
            explanation: "应为 'depend on / upon' 而非 'depend of'。".to_string(),
            suggestion: Some("depend on".to_string()),
        });
    }
    // 1.3 Countability: "an advice" / "advices" / "informations"
    if lower_in.contains("an advice") || lower_in.contains("advices") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Grammar,
            subcategory: ErrorSubcategory::Countability,
            label: "不可数名词误用".to_string(),
            explanation: "'advice' 为不可数名词，不能加不定冠词 'an' 或复数 's'，可用 'a piece of advice' 或 'some advice'。".to_string(),
            suggestion: Some("some advice / a piece of advice".to_string()),
        });
    }

    // ==========================================
    // 2. Collocation (固定搭配、动宾搭配、修饰搭配)
    // ==========================================
    // 2.1 Unnatural collocation: "very like"
    if lower_in.contains("very like") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Collocation,
            subcategory: ErrorSubcategory::UnnaturalCollocation,
            label: "搭配生硬".to_string(),
            explanation: "'very like' 不符合英语搭配习惯，修饰动词应用 'really like' 或在句末用 'like it very much'。".to_string(),
            suggestion: Some("really like".to_string()),
        });
    }
    // 2.2 Fixed expression / redundant prep: "discuss about"
    if lower_in.contains("discuss about")
        || lower_in.contains("discusses about")
        || lower_in.contains("discussed about")
        || lower_in.contains("discussing about")
    {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Collocation,
            subcategory: ErrorSubcategory::FixedExpression,
            label: "动词搭配冗余".to_string(),
            explanation: "'discuss' 为及物动词，应直接接宾语，不加介词 'about'。".to_string(),
            suggestion: Some("discuss + 宾语".to_string()),
        });
    }
    // 2.3 Verb-noun pairing: "make a photo"
    if lower_in.contains("make a photo")
        || lower_in.contains("make a picture")
        || lower_in.contains("make photos")
    {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Collocation,
            subcategory: ErrorSubcategory::IncorrectVerbNounPairing,
            label: "动宾搭配不当".to_string(),
            explanation: "拍照应使用 'take a picture/photo'，而不是 'make'。".to_string(),
            suggestion: Some("take a picture / take a photo".to_string()),
        });
    }

    // ==========================================
    // 3. Naturalness (中式直译 Chinglish、表述别扭、冗余)
    // ==========================================
    // 3.1 Chinglish: "give you some color to see see"
    if lower_in.contains("color to see") || lower_in.contains("give you some color") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Naturalness,
            subcategory: ErrorSubcategory::Chinglish,
            label: "字面直译中式表达".to_string(),
            explanation: "'give you some color to see see' 属于字面直译，地道英文可使用 'teach you a lesson' 或 'show you what I'm made of'。".to_string(),
            suggestion: Some("teach you a lesson".to_string()),
        });
    }
    // 3.2 Chinglish: "head is burning"
    if lower_in.contains("head is burning") || lower_in.contains("head burning") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Naturalness,
            subcategory: ErrorSubcategory::Chinglish,
            label: "中式思维直译".to_string(),
            explanation: "形容忙得焦头烂额不要字面直译 'head is burning'，地道美式口语常用 'swamped'、'slammed' 或 'snowed under'。".to_string(),
            suggestion: Some("swamped / slammed".to_string()),
        });
    }
    // 3.3 Chinglish: "busy like dog"
    if lower_in.contains("busy like dog") || lower_in.contains("busy like a dog") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Naturalness,
            subcategory: ErrorSubcategory::Chinglish,
            label: "中式思维直译".to_string(),
            explanation: "'忙成狗'字面直译为 'busy like dog' 不符合习惯，美式口语推荐 'I've been swamped' 或 'working around the clock'。".to_string(),
            suggestion: Some("swamped".to_string()),
        });
    }
    // 3.4 Chinglish: "people mountain people sea"
    if lower_in.contains("people mountain") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Naturalness,
            subcategory: ErrorSubcategory::Chinglish,
            label: "字面直译中式表达".to_string(),
            explanation: "'people mountain people sea' 为直译，地道表达应为 'packed'、'jam-packed' 或 'extremely crowded'。".to_string(),
            suggestion: Some("jam-packed / packed".to_string()),
        });
    }
    // 3.5 Awkward phrasing vs natural upgrade (swamped vs very busy)
    if lower_ref.contains("swamped") && (lower_in.contains("busy") || lower_in.contains("very busy")) {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Naturalness,
            subcategory: ErrorSubcategory::AwkwardPhrasing,
            label: "地道表达升级".to_string(),
            explanation: "busy 没有错，但 swamped 更口语、更符合“忙得不可开交”的语义。".to_string(),
            suggestion: Some("swamped".to_string()),
        });
    }

    // ==========================================
    // 4. Vocabulary (用词精度、选词不当)
    // ==========================================
    // 4.1 Precision: "regular" vs "often customer"
    if lower_ref.contains("regular") && (lower_in.contains("often") || lower_in.contains("customer")) {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Vocabulary,
            subcategory: ErrorSubcategory::VocabularyPrecision,
            label: "用词精度提升".to_string(),
            explanation: "'regular' 是地道的'常客'名词表达，比说明性短语更简练地道。".to_string(),
            suggestion: Some("regular".to_string()),
        });
    }
    // 4.2 Wrong word: "look the tv"
    if lower_in.contains("look the tv") || lower_in.contains("look tv") || lower_in.contains("look television") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Vocabulary,
            subcategory: ErrorSubcategory::WrongWord,
            label: "动词选词不当".to_string(),
            explanation: "看电视习惯使用动词 'watch' 而不是 'look'。".to_string(),
            suggestion: Some("watch TV".to_string()),
        });
    }

    // ==========================================
    // 5. Phrasal / Idiomatic (短语动词、习语对等)
    // ==========================================
    if lower_ref.contains("call off") && lower_in.contains("cancel") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::PhrasalIdiomatic,
            subcategory: ErrorSubcategory::MissingPhrasalVerb,
            label: "短语动词升级".to_string(),
            explanation: "在日常交流与会议讨论中，使用短语动词 'call off' 往往比正式书面词 'cancel' 更自然流畅。".to_string(),
            suggestion: Some("call off".to_string()),
        });
    }

    // ==========================================
    // 6. Pragmatics (语用视角、语境基调、礼貌程度)
    // ==========================================
    // 6.1 "according to me" -> personal perspective
    if lower_in.contains("according to me") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Pragmatics,
            subcategory: ErrorSubcategory::WrongSocialTone,
            label: "语用视角建议".to_string(),
            explanation: "'according to' 用于引述他人或第三方资料，表达个人看法请用 'in my opinion' 或 'from my perspective'。".to_string(),
            suggestion: Some("in my opinion / from my perspective".to_string()),
        });
    }
    // 6.2 Too formal: "we're good" vs "it is done"
    if lower_ref.contains("we're good") && lower_in.contains("it is done") {
        items.push(FeedbackItem {
            major_category: ErrorMajorCategory::Pragmatics,
            subcategory: ErrorSubcategory::TooFormal,
            label: "口语社交语境".to_string(),
            explanation: "母语者日常确认事情办妥时，常使用轻快自然的 'We're good'。".to_string(),
            suggestion: Some("We're good".to_string()),
        });
    }

    // 严控数量：最多保留 1~3 条最核心建议，不搞信息轰炸
    items.truncate(3);
    items
}

#[allow(dead_code)]
pub fn detect_grammar_error(input: &str) -> Option<String> {
    analyze_feedback_items(input, "")
        .into_iter()
        .find(|item| item.major_category == ErrorMajorCategory::Grammar || item.major_category == ErrorMajorCategory::Collocation)
        .map(|item| format!("{}: {}", item.label, item.explanation))
}

#[allow(dead_code)]
pub fn detect_chinglish(input: &str) -> Option<String> {
    analyze_feedback_items(input, "")
        .into_iter()
        .find(|item| item.major_category == ErrorMajorCategory::Naturalness && item.subcategory == ErrorSubcategory::Chinglish)
        .map(|item| format!("{}: {}", item.label, item.explanation))
}

#[allow(dead_code)]
pub fn generate_natural_advice(input: &str, reference: &str) -> Option<String> {
    analyze_feedback_items(input, reference)
        .into_iter()
        .find(|item| item.major_category == ErrorMajorCategory::Naturalness || item.major_category == ErrorMajorCategory::Vocabulary || item.major_category == ErrorMajorCategory::PhrasalIdiomatic || item.major_category == ErrorMajorCategory::Pragmatics)
        .map(|item| format!("{}: {}", item.label, item.explanation))
}

fn normalize(s: &str) -> String {
    let replaced_quotes = s
        .replace('’', "'")
        .replace('‘', "'")
        .replace('“', "\"")
        .replace('”', "\"");
    let cleaned = replaced_quotes
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'' || *c == '-')
        .collect::<String>();
    canonicalize_contractions(&cleaned)
}

fn parse(text: &str) -> BTreeMap<(String, String, String), RecallEntry> {
    let mut entries = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line) {
            Some((key, entry)) => {
                entries.insert(key, entry);
            }
            None => tracing::warn!(line = number + 1, "主动回忆记录有坏行，跳过"),
        }
    }
    entries
}

fn parse_line(line: &str) -> Option<((String, String, String), RecallEntry)> {
    let mut fields = line.split('\t');
    let prompt = fields.next()?.to_owned();
    let target = fields.next()?.to_owned();
    let code = fields.next()?.to_owned();
    if prompt.is_empty() || target.is_empty() || code.is_empty() {
        return None;
    }
    let mut number = || fields.next()?.parse::<u32>().ok();
    let seen = number()?;
    let success = number()?;
    let failure = number()?;
    let revealed = number()?;
    let last_reviewed: Date = fields.next()?.parse().ok()?;
    let created_at: Date = fields.next()?.parse().ok()?;
    let preferred_alternative = fields.next().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "-" {
            None
        } else {
            Some(trimmed.to_owned())
        }
    });
    let state = fields
        .next()
        .map(PersonalCorrectionState::parse)
        .unwrap_or_else(|| {
            if success >= 3 || preferred_alternative.is_some() {
                PersonalCorrectionState::Learned
            } else if success >= 1 {
                PersonalCorrectionState::Corrected
            } else {
                PersonalCorrectionState::Observed
            }
        });
    Some((
        (code, prompt, target),
        RecallEntry {
            seen,
            success,
            failure,
            revealed,
            last_reviewed,
            created_at,
            preferred_alternative,
            state,
            alternative_counts: BTreeMap::new(),
        },
    ))
}

fn today() -> Date {
    jiff::Zoned::now().date()
}

impl RecallTracker for RecallBook {
    fn record_attempt(&mut self, language: Language, attempt: &RecallAttempt) {
        self.record_attempt_on(language, attempt, today());
    }

    fn record_exposure(&mut self, language: Language, prompt: &str, target: &str) {
        self.record_exposure_on(language, prompt, target, today());
    }

    fn record_recall(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
    ) {
        self.record_recall_on(language, prompt, target, outcome, today());
    }

    fn record_recall_with_input(
        &mut self,
        language: Language,
        prompt: &str,
        target: &str,
        outcome: RecallOutcome,
        input: &str,
    ) {
        self.record_recall_with_input_on(language, prompt, target, outcome, input, today());
    }

    fn preferred_alternative(
        &self,
        language: Language,
        prompt: &str,
        target: &str,
    ) -> Option<String> {
        self.preferred_alternative_for(language, prompt, target)
    }

    fn next_item(&self, language: Language) -> Option<RecallItem> {
        self.next_item_for(language)
    }

    fn pending_count(&self, language: Language) -> usize {
        self.pending_count_for(language)
    }

    fn flush(&mut self) {
        if let Err(error) = self.save() {
            tracing::warn!(%error, "主动回忆记录落盘失败");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn test_record_exposure_and_recall_updates() {
        let mut book = RecallBook::default();
        let en = Language::English;

        // 记被动曝光
        book.record_exposure_on(en, "开发", "develop", date("2026-10-01"));
        book.record_exposure_on(en, "开发", "develop", date("2026-10-02"));
        assert_eq!(book.len(), 1);
        assert_eq!(book.pending_count_for(en), 1);

        let item = book.next_item_for(en).expect("有待复习条目");
        assert_eq!(item.prompt, "开发");
        assert_eq!(item.target, "develop");
        assert_eq!(item.seen, 2);
        assert_eq!(item.success, 0);

        // 记回忆成功
        book.record_recall_on(en, "开发", "develop", RecallOutcome::Success, date("2026-10-03"));
        assert_eq!(book.pending_count_for(en), 0);

        let item = book.next_item_for(en).expect("仍能查到已掌握条目");
        assert_eq!(item.success, 1);
    }

    #[test]
    fn test_evaluate_deterministic_matching() {
        // 精确匹配
        assert!(RecallBook::evaluate("develop", "develop"));
        // 大小写与首尾空格
        assert!(RecallBook::evaluate("  Develop  ", "develop"));
        assert!(RecallBook::evaluate("develop", "DEVELOP"));
        // 分号多义项
        assert!(RecallBook::evaluate("develop", "develop; evolve"));
        assert!(RecallBook::evaluate("evolve", "develop; evolve"));
        // 斜杠多义项
        assert!(RecallBook::evaluate("develop", "develop/grow"));
        assert!(RecallBook::evaluate("grow", "develop/grow"));
        // to 不定式剥离
        assert!(RecallBook::evaluate("develop", "to develop"));
        assert!(RecallBook::evaluate("develop", "to develop; to evolve"));
        // 缩写与全写等价 (Contraction equivalence)
        assert!(RecallBook::evaluate("don't know", "do not know"));
        assert!(RecallBook::evaluate("do not know", "don't know"));
        assert!(RecallBook::evaluate("It's done", "It is done"));
        assert!(RecallBook::evaluate("I'm here", "I am here"));
        assert!(RecallBook::evaluate("can't do it", "cannot do it"));
        // 标点归一化 (Punctuation normalization)
        assert!(RecallBook::evaluate("Got it", "Got it."));
        assert!(RecallBook::evaluate("Why aren't you here yet?", "Why are not you here yet"));
        // 自然口语等价集 (Approved colloquial alternative clusters)
        assert!(RecallBook::evaluate("Got it", "It's done"));
        assert!(RecallBook::evaluate("We're good", "It is handled"));
        assert!(RecallBook::evaluate("No worries", "No problem"));
        // 错误输入
        assert!(!RecallBook::evaluate("deve", "develop"));
        assert!(!RecallBook::evaluate("", "develop"));
    }

    #[test]
    fn test_tsv_round_trip_and_bad_line_skip() {
        let dir = std::env::temp_dir().join(format!("qingjian-recall-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user-recall.tsv");

        std::fs::write(
            &path,
            "# 头\n开发\tdevelop\ten\t2\t1\t0\t0\t2026-10-01\t2026-10-01\n坏行\n测试\t\ten\t1\t0\t0\t0\t2026-10-01\t2026-10-01\n",
        )
        .unwrap();

        let mut book = RecallBook::open(&path);
        assert_eq!(book.len(), 1);

        book.record_exposure_on(Language::English, "学习", "learn", date("2026-10-02"));
        book.save().unwrap();

        let reloaded = RecallBook::open(&path);
        assert_eq!(reloaded.len(), 2);

        let item = reloaded.next_item_for(Language::English).expect("有条目");
        // "学习" 成功次数为 0，优先于已成功的 "开发"
        assert_eq!(item.prompt, "学习");
        assert_eq!(item.target, "learn");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_14_natural_american_spoken_english_benchmarks() {
        // 1. 你咋还不来
        assert!(RecallBook::evaluate_with_prompt("Why aren't you here yet?", "Why aren't you here yet?", Some("你咋还不来")));
        assert!(RecallBook::evaluate_with_prompt("where are you at", "Why aren't you here yet?", Some("你咋还不来")));
        assert!(RecallBook::evaluate_with_prompt("Where are you?", "Why aren't you here yet?", Some("你咋还不来")));
        assert!(RecallBook::evaluate_with_prompt("What's taking you so long?", "Why aren't you here yet?", Some("你咋还不来")));

        // 2. 搞定了
        assert!(RecallBook::evaluate_with_prompt("It's done.", "It's done.", Some("搞定了")));
        assert!(RecallBook::evaluate_with_prompt("Got it.", "It's done.", Some("搞定了")));
        assert!(RecallBook::evaluate_with_prompt("We're good", "It's done.", Some("搞定了")));
        assert!(RecallBook::evaluate_with_prompt("It's handled", "It's done.", Some("搞定了")));
        assert!(RecallBook::evaluate_with_prompt("All set", "It's done.", Some("搞定了")));

        // 3. 随便吧
        assert!(RecallBook::evaluate_with_prompt("Whatever.", "Whatever.", Some("随便吧")));
        assert!(RecallBook::evaluate_with_prompt("Up to you.", "Whatever.", Some("随便吧")));
        assert!(RecallBook::evaluate_with_prompt("Either is fine.", "Whatever.", Some("随便吧")));
        assert!(RecallBook::evaluate_with_prompt("Doesn't matter to me.", "Whatever.", Some("随便吧")));

        // 4. 没事
        assert!(RecallBook::evaluate_with_prompt("No worries.", "No worries.", Some("没事")));
        assert!(RecallBook::evaluate_with_prompt("It's all good.", "No worries.", Some("没事")));
        assert!(RecallBook::evaluate_with_prompt("Don't worry about it.", "No worries.", Some("没事")));
        assert!(RecallBook::evaluate_with_prompt("You're fine.", "No worries.", Some("没事")));

        // 5. 我服了
        assert!(RecallBook::evaluate_with_prompt("I can't even.", "I can't even.", Some("我服了")));
        assert!(RecallBook::evaluate_with_prompt("I'm done.", "I can't even.", Some("我服了")));
        assert!(RecallBook::evaluate_with_prompt("I give up.", "I can't even.", Some("我服了")));

        // 6. 真的假的
        assert!(RecallBook::evaluate_with_prompt("Really?", "Really?", Some("真的假的")));
        assert!(RecallBook::evaluate_with_prompt("For real?", "Really?", Some("真的假的")));
        assert!(RecallBook::evaluate_with_prompt("Wait, really?", "Really?", Some("真的假的")));
        assert!(RecallBook::evaluate_with_prompt("Seriously?", "Really?", Some("真的假的")));

        // 7. 你认真的吗
        assert!(RecallBook::evaluate_with_prompt("Are you serious?", "Are you serious?", Some("你认真的吗")));
        assert!(RecallBook::evaluate_with_prompt("Are you for real?", "Are you serious?", Some("你认真的吗")));
        assert!(RecallBook::evaluate_with_prompt("You serious?", "Are you serious?", Some("你认真的吗")));

        // 8. 我不知道咋说
        assert!(RecallBook::evaluate_with_prompt("I don't know how to put it.", "I don't know how to put it.", Some("我不知道咋说")));
        assert!(RecallBook::evaluate_with_prompt("I'm not sure how to say it.", "I don't know how to put it.", Some("我不知道咋说")));
        assert!(RecallBook::evaluate_with_prompt("Hard to explain.", "I don't know how to put it.", Some("我不知道咋说")));

        // 9. 我先走了
        assert!(RecallBook::evaluate_with_prompt("I gotta run.", "I gotta run.", Some("我先走了")));
        assert!(RecallBook::evaluate_with_prompt("I'm heading out.", "I gotta run.", Some("我先走了")));
        assert!(RecallBook::evaluate_with_prompt("I'm gonna head out.", "I gotta run.", Some("我先走了")));
        assert!(RecallBook::evaluate_with_prompt("I gotta go.", "I gotta run.", Some("我先走了")));

        // 10. 晚点再说
        assert!(RecallBook::evaluate_with_prompt("Let's talk later.", "Let's talk later.", Some("晚点再说")));
        assert!(RecallBook::evaluate_with_prompt("We'll talk later.", "Let's talk later.", Some("晚点再说")));
        assert!(RecallBook::evaluate_with_prompt("Later.", "Let's talk later.", Some("晚点再说")));

        // 11. 太离谱了
        assert!(RecallBook::evaluate_with_prompt("That's insane.", "That's insane.", Some("太离谱了")));
        assert!(RecallBook::evaluate_with_prompt("That's crazy.", "That's insane.", Some("太离谱了")));
        assert!(RecallBook::evaluate_with_prompt("That's ridiculous.", "That's insane.", Some("太离谱了")));

        // 12. 我有点懵
        assert!(RecallBook::evaluate_with_prompt("I'm a little confused.", "I'm a little confused.", Some("我有点懵")));
        assert!(RecallBook::evaluate_with_prompt("I'm lost.", "I'm a little confused.", Some("我有点懵")));
        assert!(RecallBook::evaluate_with_prompt("Wait, what?", "I'm a little confused.", Some("我有点懵")));

        // 13. 算了
        assert!(RecallBook::evaluate_with_prompt("Never mind.", "Never mind.", Some("算了")));
        assert!(RecallBook::evaluate_with_prompt("Forget it.", "Never mind.", Some("算了")));
        assert!(RecallBook::evaluate_with_prompt("Drop it.", "Never mind.", Some("算了")));

        // 14. 行吧
        assert!(RecallBook::evaluate_with_prompt("Fair enough.", "Fair enough.", Some("行吧")));
        assert!(RecallBook::evaluate_with_prompt("Alright then.", "Fair enough.", Some("行吧")));
        assert!(RecallBook::evaluate_with_prompt("Fine.", "Fair enough.", Some("行吧")));
        assert!(RecallBook::evaluate_with_prompt("Works for me.", "Fair enough.", Some("行吧")));

        // 错误输入绝不能判定为正确
        assert!(!RecallBook::evaluate_with_prompt("resolve", "It's done.", Some("搞定了")));
        assert!(!RecallBook::evaluate_with_prompt("apple", "It's done.", Some("搞定了")));
        assert!(!RecallBook::evaluate_with_prompt("hello", "Why aren't you here yet?", Some("你咋还不来")));
    }

    #[test]
    fn test_preferred_alternative_and_three_state_progression() {
        let mut book = RecallBook::default();
        let en = Language::English;

        // 1. 被动曝光
        book.record_exposure_on(en, "搞定了", "It's done.", date("2026-10-01"));
        assert_eq!(book.len(), 1);

        // 初始状态为 Observed，无首选替代表达
        assert_eq!(
            book.preferred_alternative_for(en, "搞定了", "It's done."),
            None
        );

        // 2. 第一次主动回忆，用户输入自然口语表达 "We're good" 并判定成功
        book.record_recall_with_input_on(
            en,
            "搞定了",
            "It's done.",
            RecallOutcome::Success,
            "We're good",
            date("2026-10-02"),
        );
        // 第一次使用进入 Corrected 状态，但因使用次数仅 1 次，尚未固化为首选
        assert_eq!(
            book.preferred_alternative_for(en, "搞定了", "It's done."),
            None
        );

        // 3. 第二次主动回忆，再次成功输入 "We're good"
        book.record_recall_with_input_on(
            en,
            "搞定了",
            "It's done.",
            RecallOutcome::Success,
            "We're good",
            date("2026-10-03"),
        );
        // 使用次数达到 2 次，固化为 Learned 状态，首选替代表达生效！
        assert_eq!(
            book.preferred_alternative_for(en, "搞定了", "It's done."),
            Some("We're good".to_string())
        );

        // 4. 防学坏测试：用户输错（例如打入不合意图的 "resolve" 或 "apple"）
        book.record_recall_with_input_on(
            en,
            "搞定了",
            "It's done.",
            RecallOutcome::Failure,
            "resolve",
            date("2026-10-04"),
        );
        // 错误输入绝不能进入替代表达，首选依然稳固保持 "We're good"
        assert_eq!(
            book.preferred_alternative_for(en, "搞定了", "It's done."),
            Some("We're good".to_string())
        );

        // 5. 分析查询验证
        let learned = book.learned_items(en);
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].prompt, "搞定了");
    }

    #[test]
    fn test_analytics_and_11_column_round_trip() {
        let dir = std::env::temp_dir().join(format!("qingjian-recall-p4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user-recall.tsv");

        let mut book = RecallBook::open(&path);
        let en = Language::English;

        // 曝光 6 次的词条
        for i in 1..=6 {
            book.record_exposure_on(en, "常客", "regular", date(&format!("2026-10-0{i}")));
        }
        // 卡点条目：查看答案 2 次
        book.record_recall_on(en, "常客", "regular", RecallOutcome::Revealed, date("2026-10-06"));
        book.record_recall_on(en, "常客", "regular", RecallOutcome::Revealed, date("2026-10-07"));

        // 验证高频曝光与卡点查询
        assert_eq!(book.frequently_exposed_items(en, 5).len(), 1);
        assert_eq!(book.frequently_revealed_items(en, 2).len(), 1);
        assert_eq!(book.struggling_items(en).len(), 1);

        // 保存并重载（11 列）
        book.save().unwrap();

        let reloaded = RecallBook::open(&path);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.frequently_revealed_items(en, 2).len(), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_evaluate_attempt_matrix() {
        // 1. 空输入被拒绝，绝不伪造对比
        let empty = RecallBook::evaluate_attempt("我最近忙得焦头烂额", "", "I've been swamped lately.");
        assert_eq!(empty.comparison.status, Some(ComparisonStatus::EmptyInput));
        assert!(empty.user_attempt.is_empty());

        // 2. 精确匹配
        let exact = RecallBook::evaluate_attempt("我最近忙得焦头烂额", "I've been swamped lately.", "I've been swamped lately.");
        assert_eq!(exact.comparison.status, Some(ComparisonStatus::ExactMatch));

        // 3. 语法错误 (B2 典型语法错)
        let grammar1 = RecallBook::evaluate_attempt("我很喜欢", "I very like it.", "I really like it.");
        assert_eq!(grammar1.comparison.status, Some(ComparisonStatus::GrammarIssue));
        assert!(grammar1.error_categories.contains(&"grammar".to_string()));

        let grammar2 = RecallBook::evaluate_attempt("我昨天很忙", "I'm very busy yesterday.", "I was very busy yesterday.");
        assert_eq!(grammar2.comparison.status, Some(ComparisonStatus::GrammarIssue));

        // 4. 中式英语直译 (Chinglish)
        let chinglish1 = RecallBook::evaluate_attempt("给你点颜色看看", "give you some color to see see", "teach you a lesson");
        assert_eq!(chinglish1.comparison.status, Some(ComparisonStatus::Chinglish));
        assert!(chinglish1.error_categories.contains(&"chinglish".to_string()));

        let chinglish2 = RecallBook::evaluate_attempt("忙得焦头烂额", "my head is burning", "I've been swamped lately.");
        assert_eq!(chinglish2.comparison.status, Some(ComparisonStatus::Chinglish));

        // 5. 语义正确但表达不够地道 (Meaning Correct But Less Natural)
        let natural = RecallBook::evaluate_attempt("我最近忙得焦头烂额", "I've been very busy recently.", "I've been swamped lately.");
        assert_eq!(natural.comparison.status, Some(ComparisonStatus::NaturalBetter));
        assert!(natural.comparison.key_advice.contains("swamped"));

        // 6. 持久化防护：空尝试绝不写入 RecallBook
        let mut book = RecallBook::default();
        book.record_attempt_on(Language::English, &empty, date("2026-10-08"));
        assert_eq!(book.len(), 0);

        // 有效尝试正常记录
        book.record_attempt_on(Language::English, &exact, date("2026-10-08"));
        assert_eq!(book.len(), 1);
    }
}

