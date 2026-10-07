//! 个人英语学习隐私账本 (Learning Vault) 与学习画像聚合器
//!
//! 核心原则：
//! 1. 严格隐私隔离：日常中文打字与普通输入绝对不写入学习账本；
//! 2. 最小必要记录：仅在用户显式进行 Save / Recall / Compare 时产生学习与训练账本；
//! 3. 真实数据累积：画像指标与薄弱环节基于真实记录动态推导；
//! 4. 具备完整数据重置入口 (Clear All)。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use qingjian_core::storage::{read_text_lossy, write_atomic_str};
use qingjian_core::{
    ComparisonStatus, DimensionStat, ErrorMajorCategory, ErrorRecord, ErrorSubcategory,
    MasteryLevel, MasteryRecord, ProfileDimension, RecallAttempt, ReviewItem,
    ReviewRating, SavedExpression, UserAttempt, UserEnglishProfile,
};

use crate::error::LearningError;
use crate::fsrs::FsrsScheduler;

/// 学习账本与画像聚合管理器
#[derive(Debug, Default)]
pub struct LearningVault {
    base_dir: Option<PathBuf>,
    saved_expressions: Vec<SavedExpression>,
    attempts: Vec<UserAttempt>,
    error_records: BTreeMap<String, ErrorRecord>,
    mastery_records: BTreeMap<(String, String), MasteryRecord>,
    review_queue: BTreeMap<(String, String), ReviewItem>,
    dirty: bool,
}

impl LearningVault {
    /// 打开或初始化指定目录下的学习账本
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let mut vault = Self {
            base_dir: Some(dir.clone()),
            saved_expressions: Vec::new(),
            attempts: Vec::new(),
            error_records: BTreeMap::new(),
            mastery_records: BTreeMap::new(),
            review_queue: BTreeMap::new(),
            dirty: false,
        };

        vault.load_from_disk(&dir);
        vault
    }

    /// 从磁盘加载已有账本数据（容错设计：单行损坏跳过）
    fn load_from_disk(&mut self, dir: &Path) {
        // 1. 加载主动收藏的表达 (user-saved-expressions.tsv)
        let saved_path = dir.join("user-saved-expressions.tsv");
        if let Ok(Some(content)) = read_text_lossy(&saved_path) {
            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    let prompt = parts[0].to_string();
                    let expression = parts[1].to_string();
                    let tag = parts.get(2).and_then(|s| if s.is_empty() || *s == "-" { None } else { Some(s.to_string()) });
                    let notes = parts.get(3).and_then(|s| if s.is_empty() || *s == "-" { None } else { Some(s.to_string()) });
                    let saved_at = parts.get(4).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                    let mastery = parts.get(5).map(|s| match *s {
                        "learning" => MasteryLevel::Learning,
                        "familiar" => MasteryLevel::Familiar,
                        "mastered" => MasteryLevel::Mastered,
                        _ => MasteryLevel::Unseen,
                    }).unwrap_or(MasteryLevel::Learning);

                    self.saved_expressions.push(SavedExpression {
                        prompt,
                        expression,
                        tag,
                        notes,
                        saved_at,
                        mastery,
                    });
                }
            }
        }

        // 2. 加载训练尝试记录 (user-learning-attempts.jsonl)
        let attempts_path = dir.join("user-learning-attempts.jsonl");
        if let Ok(Some(content)) = read_text_lossy(&attempts_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(attempt) = serde_json::from_str::<UserAttempt>(trimmed) {
                    self.attempts.push(attempt);
                }
            }
        }

        // 3. 加载错误归档 (user-errors.tsv)
        let errors_path = dir.join("user-errors.tsv");
        if let Ok(Some(content)) = read_text_lossy(&errors_path) {
            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 6 {
                    let major_str = parts[0];
                    let sub_str = parts[1];
                    let user_fragment = parts[2].to_string();
                    let corrected_fragment = parts[3].to_string();
                    let explanation = parts[4].to_string();
                    let occurrence_count = parts.get(5).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                    let last_occurred_at = parts.get(6).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);

                    let major = match major_str {
                        "grammar" => ErrorMajorCategory::Grammar,
                        "vocabulary" => ErrorMajorCategory::Vocabulary,
                        "collocation" => ErrorMajorCategory::Collocation,
                        "naturalness" => ErrorMajorCategory::Naturalness,
                        "phrasal_idiomatic" => ErrorMajorCategory::PhrasalIdiomatic,
                        "pragmatics" => ErrorMajorCategory::Pragmatics,
                        _ => ErrorMajorCategory::Naturalness,
                    };
                    let sub = match sub_str {
                        "article" => Some(ErrorSubcategory::Article),
                        "tense" => Some(ErrorSubcategory::Tense),
                        "preposition" => Some(ErrorSubcategory::Preposition),
                        "word_order" => Some(ErrorSubcategory::WordOrder),
                        "agreement" => Some(ErrorSubcategory::Agreement),
                        "countability" => Some(ErrorSubcategory::Countability),
                        "wrong_word" => Some(ErrorSubcategory::WrongWord),
                        "literal_translation" => Some(ErrorSubcategory::LiteralTranslation),
                        "vocabulary_precision" => Some(ErrorSubcategory::VocabularyPrecision),
                        "unnatural_collocation" => Some(ErrorSubcategory::UnnaturalCollocation),
                        "incorrect_verb_noun_pairing" => Some(ErrorSubcategory::IncorrectVerbNounPairing),
                        "adjective_noun_pairing" => Some(ErrorSubcategory::AdjectiveNounPairing),
                        "fixed_expression" => Some(ErrorSubcategory::FixedExpression),
                        "awkward_phrasing" => Some(ErrorSubcategory::AwkwardPhrasing),
                        "redundancy" => Some(ErrorSubcategory::Redundancy),
                        "unnatural_sentence_rhythm" => Some(ErrorSubcategory::UnnaturalSentenceRhythm),
                        "chinglish" => Some(ErrorSubcategory::Chinglish),
                        "missing_phrasal_verb" => Some(ErrorSubcategory::MissingPhrasalVerb),
                        "wrong_idiom" => Some(ErrorSubcategory::WrongIdiom),
                        "unnatural_literal_equivalent" => Some(ErrorSubcategory::UnnaturalLiteralEquivalent),
                        "too_direct" => Some(ErrorSubcategory::TooDirect),
                        "too_formal" => Some(ErrorSubcategory::TooFormal),
                        "too_casual" => Some(ErrorSubcategory::TooCasual),
                        "wrong_social_tone" => Some(ErrorSubcategory::WrongSocialTone),
                        _ => None,
                    };

                    let key = format!("{}:{}:{}", major_str, sub_str, user_fragment);
                    self.error_records.insert(key, ErrorRecord {
                        major_category: major,
                        subcategory: sub,
                        user_fragment,
                        corrected_fragment,
                        explanation,
                        occurrence_count,
                        last_occurred_at,
                    });
                }
            }
        }

        // 4. 加载掌握状态表 (user-mastery.tsv)
        let mastery_path = dir.join("user-mastery.tsv");
        if let Ok(Some(content)) = read_text_lossy(&mastery_path) {
            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 5 {
                    let prompt = parts[0].to_string();
                    let target = parts[1].to_string();
                    let level = match parts[2] {
                        "learning" => MasteryLevel::Learning,
                        "familiar" => MasteryLevel::Familiar,
                        "mastered" => MasteryLevel::Mastered,
                        _ => MasteryLevel::Unseen,
                    };
                    let consecutive = parts.get(3).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                    let total = parts.get(4).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                    let updated_at = parts.get(5).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);

                    self.mastery_records.insert((prompt.clone(), target.clone()), MasteryRecord {
                        prompt,
                        target,
                        level,
                        consecutive_successes: consecutive,
                        total_reviews: total,
                        updated_at,
                    });
                }
            }
        }

        // 5. 加载 FSRS 复习队列 (user-review-queue.json)
        let queue_path = dir.join("user-review-queue.json");
        if let Ok(Some(content)) = read_text_lossy(&queue_path) {
            if let Ok(items) = serde_json::from_str::<Vec<ReviewItem>>(&content) {
                for item in items {
                    let key = (item.prompt.clone(), item.expression.clone());
                    self.review_queue.insert(key, item);
                }
            }
        }

        // 容错补充：确保所有已收藏表达均在复习队列中
        for saved in &self.saved_expressions {
            let key = (saved.prompt.clone(), saved.expression.clone());
            if !self.review_queue.contains_key(&key) {
                let mut item = FsrsScheduler::init_item(&saved.prompt, &saved.expression, saved.saved_at);
                item.mastery = saved.mastery;
                if let Some(mastery_rec) = self.mastery_records.get(&key) {
                    item.attempts_count = mastery_rec.total_reviews;
                    item.mastery = mastery_rec.level;
                }
                self.review_queue.insert(key, item);
            }
        }
    }

    /// 用户显式点击 ⭐ 保存或快捷键 Ctrl+S 时记录地道表达
    pub fn save_expression(
        &mut self,
        prompt: &str,
        expression: &str,
        tag: Option<&str>,
        notes: Option<&str>,
    ) {
        if prompt.trim().is_empty() || expression.trim().is_empty() {
            return;
        }
        let now = jiff::Zoned::now().timestamp().as_second();
        let prompt_trimmed = prompt.trim().to_string();
        let expr_trimmed = expression.trim().to_string();

        // 查重：若已有记录则更新时间与信息
        if let Some(existing) = self.saved_expressions.iter_mut().find(|e| e.prompt == prompt_trimmed && e.expression == expr_trimmed) {
            existing.saved_at = now;
            if tag.is_some() {
                existing.tag = tag.map(|s| s.to_string());
            }
            if notes.is_some() {
                existing.notes = notes.map(|s| s.to_string());
            }
        } else {
            self.saved_expressions.push(SavedExpression {
                prompt: prompt_trimmed.clone(),
                expression: expr_trimmed.clone(),
                tag: tag.map(|s| s.to_string()),
                notes: notes.map(|s| s.to_string()),
                saved_at: now,
                mastery: MasteryLevel::Learning,
            });
        }

        // 同步初始化/更新掌握状态
        self.mastery_records
            .entry((prompt_trimmed.clone(), expr_trimmed.clone()))
            .or_insert_with(|| MasteryRecord {
                prompt: prompt_trimmed.clone(),
                target: expr_trimmed.clone(),
                level: MasteryLevel::Learning,
                consecutive_successes: 0,
                total_reviews: 0,
                updated_at: now,
            });

        // 同步初始化/更新 FSRS 复习条目
        let queue_key = (prompt_trimmed.clone(), expr_trimmed.clone());
        self.review_queue
            .entry(queue_key)
            .or_insert_with(|| FsrsScheduler::init_item(&prompt_trimmed, &expr_trimmed, now));

        self.dirty = true;
    }

    /// 记录用户在 Drill 或 Compare 模式下的主动练习尝试
    pub fn record_attempt(&mut self, attempt: &RecallAttempt) {
        let input = attempt.user_attempt.trim();
        if input.is_empty() {
            return;
        }
        let now = attempt.timestamp;
        let is_success = matches!(
            attempt.comparison.status,
            Some(ComparisonStatus::ExactMatch) | Some(ComparisonStatus::ValidB2Upgrade)
        );

        // 1. 追加训练尝试记录
        self.attempts.push(UserAttempt {
            prompt: attempt.prompt.clone(),
            user_input: input.to_string(),
            reference: attempt.reference_expression.clone(),
            is_success,
            error_categories: attempt.error_categories.clone(),
            timestamp: now,
        });

        // 2. 提取并聚合错误记录
        for item in &attempt.comparison.feedback_items {
            let major_s = major_category_str(item.major_category);
            let sub_s = subcategory_str(&item.subcategory);
            let key = format!("{}:{}:{}", major_s, sub_s, input);
            self.error_records
                .entry(key)
                .and_modify(|rec| {
                    rec.occurrence_count = rec.occurrence_count.saturating_add(1);
                    rec.last_occurred_at = now;
                })
                .or_insert_with(|| ErrorRecord {
                    major_category: item.major_category,
                    subcategory: Some(item.subcategory.clone()),
                    user_fragment: input.to_string(),
                    corrected_fragment: item.suggestion.clone().unwrap_or_else(|| attempt.reference_expression.clone()),
                    explanation: item.explanation.clone(),
                    occurrence_count: 1,
                    last_occurred_at: now,
                });
        }

        // 3. 更新该项目的掌握度记录
        let key = (attempt.prompt.clone(), attempt.reference_expression.clone());
        let record = self.mastery_records.entry(key.clone()).or_insert_with(|| MasteryRecord {
            prompt: attempt.prompt.clone(),
            target: attempt.reference_expression.clone(),
            level: MasteryLevel::Learning,
            consecutive_successes: 0,
            total_reviews: 0,
            updated_at: now,
        });

        record.total_reviews = record.total_reviews.saturating_add(1);
        record.updated_at = now;

        if is_success {
            record.consecutive_successes = record.consecutive_successes.saturating_add(1);
            if record.consecutive_successes >= 4 {
                record.level = MasteryLevel::Mastered;
            } else if record.consecutive_successes >= 2 {
                record.level = MasteryLevel::Familiar;
            } else {
                record.level = MasteryLevel::Learning;
            }
        } else {
            record.consecutive_successes = 0;
            record.level = MasteryLevel::Learning;
        }

        let new_level = record.level;
        if let Some(saved) = self
            .saved_expressions
            .iter_mut()
            .find(|e| e.prompt == attempt.prompt && e.expression == attempt.reference_expression)
        {
            saved.mastery = new_level;
        }

        // 4. 同步推进 FSRS 复习状态
        let rating = FsrsScheduler::infer_rating(attempt.comparison.status, attempt.comparison.feedback_items.len());
        let fsrs_item = self.review_queue.entry(key).or_insert_with(|| {
            FsrsScheduler::init_item(&attempt.prompt, &attempt.reference_expression, now)
        });
        FsrsScheduler::step(fsrs_item, rating, now);

        self.dirty = true;
    }

    /// 基于真实累积记录动态计算个人英语画像
    pub fn calculate_profile(&self) -> UserEnglishProfile {
        let mut profile = UserEnglishProfile::default();
        profile.total_saved = self.saved_expressions.len() as u32;
        profile.total_attempts = self.attempts.len() as u32;
        profile.total_reviews = self.mastery_records.values().map(|r| r.total_reviews).sum();

        // 维度统计收集器 (tested, errors, successes)
        let mut dim_data: BTreeMap<ProfileDimension, (u32, u32, u32)> = BTreeMap::new();
        for dim in [
            ProfileDimension::Vocabulary,
            ProfileDimension::Grammar,
            ProfileDimension::Collocation,
            ProfileDimension::Naturalness,
            ProfileDimension::PhrasalVerbs,
            ProfileDimension::Pragmatics,
            ProfileDimension::Register,
        ] {
            dim_data.insert(dim, (0, 0, 0));
        }

        // 遍历所有 attempts 统计测试量与成功量
        for att in &self.attempts {
            // 对每个维度累计 tested
            for (_, (tested, _, successes)) in dim_data.iter_mut() {
                *tested += 1;
                if att.is_success {
                    *successes += 1;
                }
            }
        }

        // 遍历错误记录累计对应维度的 errors
        for err in self.error_records.values() {
            let dim = match err.major_category {
                ErrorMajorCategory::Grammar => ProfileDimension::Grammar,
                ErrorMajorCategory::Vocabulary => ProfileDimension::Vocabulary,
                ErrorMajorCategory::Collocation => ProfileDimension::Collocation,
                ErrorMajorCategory::Naturalness => ProfileDimension::Naturalness,
                ErrorMajorCategory::PhrasalIdiomatic => ProfileDimension::PhrasalVerbs,
                ErrorMajorCategory::Pragmatics => {
                    if let Some(ErrorSubcategory::TooFormal) | Some(ErrorSubcategory::TooCasual) = &err.subcategory {
                        ProfileDimension::Register
                    } else {
                        ProfileDimension::Pragmatics
                    }
                }
            };
            if let Some((_, errors, _)) = dim_data.get_mut(&dim) {
                *errors += err.occurrence_count;
            }
        }

        // 计算各维度得分：基准分 70 (B2)，成功微调加分，错误惩罚减分
        let calc_dim = |dim: ProfileDimension| -> DimensionStat {
            let &(tested, errors, successes) = dim_data.get(&dim).unwrap_or(&(0, 0, 0));
            let score = if tested == 0 && errors == 0 {
                70 // 默认 B2 基准
            } else {
                let base = 70i32;
                let boost = (successes as i32 * 3).min(30);
                let penalty = (errors as i32 * 8).min(60);
                (base + boost - penalty).clamp(10, 100) as u8
            };
            DimensionStat {
                total_tested: tested,
                errors,
                successes,
                score,
            }
        };

        profile.vocabulary = calc_dim(ProfileDimension::Vocabulary);
        profile.grammar = calc_dim(ProfileDimension::Grammar);
        profile.collocation = calc_dim(ProfileDimension::Collocation);
        profile.naturalness = calc_dim(ProfileDimension::Naturalness);
        profile.phrasal_verbs = calc_dim(ProfileDimension::PhrasalVerbs);
        profile.pragmatics = calc_dim(ProfileDimension::Pragmatics);
        profile.register = calc_dim(ProfileDimension::Register);

        // 找出薄弱环节（得分最低且有错误产生的 1~2 个维度）
        let mut candidates = vec![
            ("Vocabulary", profile.vocabulary.score, profile.vocabulary.errors),
            ("Grammar", profile.grammar.score, profile.grammar.errors),
            ("Collocations", profile.collocation.score, profile.collocation.errors),
            ("Naturalness", profile.naturalness.score, profile.naturalness.errors),
            ("Phrasal Verbs", profile.phrasal_verbs.score, profile.phrasal_verbs.errors),
            ("Pragmatics", profile.pragmatics.score, profile.pragmatics.errors),
            ("Register", profile.register.score, profile.register.errors),
        ];

        // 优先筛选有错误的维度，并按分数升序、错误降序排列
        candidates.retain(|(_, _, errs)| *errs > 0);
        candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));

        profile.top_weaknesses = candidates
            .into_iter()
            .take(2)
            .map(|(name, _, _)| name.to_string())
            .collect();

        profile.c1_upgrades_count = self
            .attempts
            .iter()
            .filter(|a| a.error_categories.contains(&"b2_upgrade".to_string()))
            .count() as u32;

        profile
    }

    /// 获取待复习清单 (所有已记录复习条目)
    pub fn review_items(&self) -> Vec<ReviewItem> {
        self.review_queue.values().cloned().collect()
    }

    /// 按提示词和目标英文获取指定复习项
    pub fn get_review_item(&self, prompt: &str, expression: &str) -> Option<&ReviewItem> {
        self.review_queue.get(&(prompt.trim().to_string(), expression.trim().to_string()))
    }

    /// 用户在主动复习/Drill界面直接指定评级推进 FSRS 调度
    pub fn process_review(
        &mut self,
        prompt: &str,
        expression: &str,
        rating: ReviewRating,
        now: i64,
    ) {
        let key = (prompt.trim().to_string(), expression.trim().to_string());
        let item = self.review_queue.entry(key.clone()).or_insert_with(|| {
            FsrsScheduler::init_item(prompt, expression, now)
        });

        FsrsScheduler::step(item, rating, now);
        let new_mastery = item.mastery;

        // 同步更新 saved_expressions 中的 mastery
        if let Some(saved) = self.saved_expressions.iter_mut().find(|e| e.prompt == key.0 && e.expression == key.1) {
            saved.mastery = new_mastery;
        }

        // 同步更新 mastery_records
        let record = self.mastery_records.entry(key).or_insert_with(|| MasteryRecord {
            prompt: prompt.trim().to_string(),
            target: expression.trim().to_string(),
            level: new_mastery,
            consecutive_successes: 0,
            total_reviews: 0,
            updated_at: now,
        });
        record.total_reviews = record.total_reviews.saturating_add(1);
        record.updated_at = now;
        record.level = new_mastery;
        if rating == ReviewRating::Again {
            record.consecutive_successes = 0;
        } else {
            record.consecutive_successes = record.consecutive_successes.saturating_add(1);
        }

        self.dirty = true;
    }

    /// 获取今日待复习清单（到期或今日内即将到期），按紧急程度与遗忘风险排序
    pub fn today_due_reviews(&self, now: i64) -> Vec<&ReviewItem> {
        let mut list: Vec<&ReviewItem> = self
            .review_queue
            .values()
            .filter(|item| FsrsScheduler::is_due_today(item, now))
            .collect();

        // 优先级排序：逾期最久 (due_time 小) 优先；若同一时间，可提取率较低 (记忆更脆弱) 优先
        list.sort_by(|a, b| {
            a.due_time.cmp(&b.due_time).then_with(|| {
                let r_a = FsrsScheduler::calculate_retrievability(a, now);
                let r_b = FsrsScheduler::calculate_retrievability(b, now);
                r_a.partial_cmp(&r_b).unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        list
    }

    /// 今日待复习条目数量
    pub fn today_due_count(&self, now: i64) -> usize {
        self.review_queue
            .values()
            .filter(|item| FsrsScheduler::is_due_today(item, now))
            .count()
    }

    /// 获取完整复习队列引用
    pub fn review_queue(&self) -> &BTreeMap<(String, String), ReviewItem> {
        &self.review_queue
    }

    /// 获取收藏表达切片
    pub fn saved_expressions(&self) -> &[SavedExpression] {
        &self.saved_expressions
    }

    /// 获取历史训练尝试记录
    pub fn attempts(&self) -> &[UserAttempt] {
        &self.attempts
    }

    /// 账本是否为空
    pub fn is_empty(&self) -> bool {
        self.saved_expressions.is_empty() && self.attempts.is_empty()
    }

    /// 获取聚合错误记录切片
    pub fn error_records(&self) -> Vec<&ErrorRecord> {
        self.error_records.values().collect()
    }

    /// 获取掌握度记录切片
    pub fn mastery_records(&self) -> Vec<&MasteryRecord> {
        self.mastery_records.values().collect()
    }

    /// 清空所有学习数据（隐私安全撤销入口）
    pub fn clear_all(&mut self) -> Result<(), LearningError> {
        self.saved_expressions.clear();
        self.attempts.clear();
        self.error_records.clear();
        self.mastery_records.clear();
        self.review_queue.clear();
        self.dirty = false;

        if let Some(dir) = &self.base_dir {
            let files = [
                dir.join("user-saved-expressions.tsv"),
                dir.join("user-learning-attempts.jsonl"),
                dir.join("user-errors.tsv"),
                dir.join("user-mastery.tsv"),
                dir.join("user-learning-profile.json"),
                dir.join("user-review-queue.json"),
            ];
            for f in files {
                if f.exists() {
                    let _ = std::fs::remove_file(f);
                }
            }
        }
        Ok(())
    }

    /// 落盘保存当前内存数据
    pub fn flush(&mut self) -> Result<(), LearningError> {
        let Some(dir) = self.base_dir.clone() else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }

        // 1. 保存 user-saved-expressions.tsv
        let mut saved_tsv = String::from("# prompt\texpression\ttag\tnotes\tsaved_at\tmastery\n");
        for item in &self.saved_expressions {
            let tag = item.tag.as_deref().unwrap_or("-");
            let notes = item.notes.as_deref().unwrap_or("-");
            let _ = writeln!(
                saved_tsv,
                "{}\t{}\t{}\t{}\t{}\t{}",
                item.prompt, item.expression, tag, notes, item.saved_at, item.mastery.as_str()
            );
        }
        write_atomic_str(&dir.join("user-saved-expressions.tsv"), &saved_tsv)?;

        // 2. 保存 user-learning-attempts.jsonl
        let attempts_path = dir.join("user-learning-attempts.jsonl");
        let mut attempts_file = File::create(&attempts_path)?;
        for att in &self.attempts {
            if let Ok(json) = serde_json::to_string(att) {
                writeln!(attempts_file, "{}", json)?;
            }
        }

        // 3. 保存 user-errors.tsv
        let mut errors_tsv = String::from("# major\tsub\tuser_frag\tcorrect_frag\texplanation\tcount\tlast_at\n");
        for err in self.error_records.values() {
            let major = match err.major_category {
                ErrorMajorCategory::Grammar => "grammar",
                ErrorMajorCategory::Vocabulary => "vocabulary",
                ErrorMajorCategory::Collocation => "collocation",
                ErrorMajorCategory::Naturalness => "naturalness",
                ErrorMajorCategory::PhrasalIdiomatic => "phrasal_idiomatic",
                ErrorMajorCategory::Pragmatics => "pragmatics",
            };
            let sub = err.subcategory.as_ref().map(|s| match s {
                ErrorSubcategory::Article => "article",
                ErrorSubcategory::Tense => "tense",
                ErrorSubcategory::Preposition => "preposition",
                ErrorSubcategory::WordOrder => "word_order",
                ErrorSubcategory::Agreement => "agreement",
                ErrorSubcategory::Countability => "countability",
                ErrorSubcategory::WrongWord => "wrong_word",
                ErrorSubcategory::LiteralTranslation => "literal_translation",
                ErrorSubcategory::VocabularyPrecision => "vocabulary_precision",
                ErrorSubcategory::UnnaturalCollocation => "unnatural_collocation",
                ErrorSubcategory::IncorrectVerbNounPairing => "incorrect_verb_noun_pairing",
                ErrorSubcategory::AdjectiveNounPairing => "adjective_noun_pairing",
                ErrorSubcategory::FixedExpression => "fixed_expression",
                ErrorSubcategory::AwkwardPhrasing => "awkward_phrasing",
                ErrorSubcategory::Redundancy => "redundancy",
                ErrorSubcategory::UnnaturalSentenceRhythm => "unnatural_sentence_rhythm",
                ErrorSubcategory::Chinglish => "chinglish",
                ErrorSubcategory::MissingPhrasalVerb => "missing_phrasal_verb",
                ErrorSubcategory::WrongIdiom => "wrong_idiom",
                ErrorSubcategory::UnnaturalLiteralEquivalent => "unnatural_literal_equivalent",
                ErrorSubcategory::TooDirect => "too_direct",
                ErrorSubcategory::TooFormal => "too_formal",
                ErrorSubcategory::TooCasual => "too_casual",
                ErrorSubcategory::WrongSocialTone => "wrong_social_tone",
                _ => "other",
            }).unwrap_or("-");

            let _ = writeln!(
                errors_tsv,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                major, sub, err.user_fragment, err.corrected_fragment, err.explanation, err.occurrence_count, err.last_occurred_at
            );
        }
        write_atomic_str(&dir.join("user-errors.tsv"), &errors_tsv)?;

        // 4. 保存 user-mastery.tsv
        let mut mastery_tsv = String::from("# prompt\ttarget\tlevel\tconsecutive\ttotal\tupdated_at\n");
        for rec in self.mastery_records.values() {
            let _ = writeln!(
                mastery_tsv,
                "{}\t{}\t{}\t{}\t{}\t{}",
                rec.prompt, rec.target, rec.level.as_str(), rec.consecutive_successes, rec.total_reviews, rec.updated_at
            );
        }
        write_atomic_str(&dir.join("user-mastery.tsv"), &mastery_tsv)?;

        // 5. 保存 user-learning-profile.json
        let profile = self.calculate_profile();
        if let Ok(json) = serde_json::to_string_pretty(&profile) {
            write_atomic_str(&dir.join("user-learning-profile.json"), &json)?;
        }

        // 6. 保存 user-review-queue.json
        let queue_items: Vec<&ReviewItem> = self.review_queue.values().collect();
        if let Ok(json) = serde_json::to_string_pretty(&queue_items) {
            write_atomic_str(&dir.join("user-review-queue.json"), &json)?;
        }

        self.dirty = false;
        Ok(())
    }
}

pub(crate) fn major_category_str(cat: ErrorMajorCategory) -> &'static str {
    match cat {
        ErrorMajorCategory::Grammar => "grammar",
        ErrorMajorCategory::Vocabulary => "vocabulary",
        ErrorMajorCategory::Collocation => "collocation",
        ErrorMajorCategory::Naturalness => "naturalness",
        ErrorMajorCategory::PhrasalIdiomatic => "phrasal_idiomatic",
        ErrorMajorCategory::Pragmatics => "pragmatics",
    }
}

pub(crate) fn subcategory_str(sub: &ErrorSubcategory) -> &str {
    match sub {
        ErrorSubcategory::Article => "article",
        ErrorSubcategory::Tense => "tense",
        ErrorSubcategory::Preposition => "preposition",
        ErrorSubcategory::WordOrder => "word_order",
        ErrorSubcategory::Agreement => "agreement",
        ErrorSubcategory::Countability => "countability",
        ErrorSubcategory::WrongWord => "wrong_word",
        ErrorSubcategory::LiteralTranslation => "literal_translation",
        ErrorSubcategory::VocabularyPrecision => "vocabulary_precision",
        ErrorSubcategory::UnnaturalCollocation => "unnatural_collocation",
        ErrorSubcategory::IncorrectVerbNounPairing => "incorrect_verb_noun_pairing",
        ErrorSubcategory::AdjectiveNounPairing => "adjective_noun_pairing",
        ErrorSubcategory::FixedExpression => "fixed_expression",
        ErrorSubcategory::AwkwardPhrasing => "awkward_phrasing",
        ErrorSubcategory::Redundancy => "redundancy",
        ErrorSubcategory::UnnaturalSentenceRhythm => "unnatural_sentence_rhythm",
        ErrorSubcategory::Chinglish => "chinglish",
        ErrorSubcategory::MissingPhrasalVerb => "missing_phrasal_verb",
        ErrorSubcategory::WrongIdiom => "wrong_idiom",
        ErrorSubcategory::UnnaturalLiteralEquivalent => "unnatural_literal_equivalent",
        ErrorSubcategory::TooDirect => "too_direct",
        ErrorSubcategory::TooFormal => "too_formal",
        ErrorSubcategory::TooCasual => "too_casual",
        ErrorSubcategory::WrongSocialTone => "wrong_social_tone",
        ErrorSubcategory::Other(s) => s.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_core::{ComparisonResult, ComparisonStatus, FeedbackItem};

    fn test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("qingjian-vault-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn test_vault_save_expression_and_round_trip() {
        let path = test_dir("save-roundtrip");
        let mut vault = LearningVault::open(&path);

        vault.save_expression(
            "我最近忙得焦头烂额",
            "I've been swamped lately.",
            Some("Casual"),
            Some("地道美式口语"),
        );
        assert_eq!(vault.saved_expressions().len(), 1);
        vault.flush().unwrap();

        // 重新打开验证
        let reloaded = LearningVault::open(&path);
        assert_eq!(reloaded.saved_expressions().len(), 1);
        assert_eq!(reloaded.saved_expressions()[0].expression, "I've been swamped lately.");
        assert_eq!(reloaded.saved_expressions()[0].mastery, MasteryLevel::Learning);
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_vault_record_attempt_and_profile_accumulation() {
        let path = test_dir("profile-accum");
        let mut vault = LearningVault::open(&path);

        // 1. 记录一次包含搭配错误的尝试
        let attempt = RecallAttempt {
            prompt: "我非常喜欢这个方案".to_string(),
            user_attempt: "I very like this proposal".to_string(),
            reference_expression: "I really like this proposal".to_string(),
            timestamp: 1728000000,
            comparison: ComparisonResult {
                status: Some(ComparisonStatus::GrammarIssue),
                key_advice: "修饰动词应用 really like".to_string(),
                user_formatted: "I very like this proposal".to_string(),
                natural_expression: "I really like this proposal".to_string(),
                primary_category: Some(ErrorMajorCategory::Collocation),
                feedback_items: vec![FeedbackItem {
                    major_category: ErrorMajorCategory::Collocation,
                    subcategory: ErrorSubcategory::UnnaturalCollocation,
                    label: "搭配生硬".to_string(),
                    explanation: "修饰动词应用 really like".to_string(),
                    suggestion: Some("really like".to_string()),
                }],
                upgrade: None,
                dimensions: None,
            },
            error_categories: vec!["collocation".to_string()],
        };

        vault.record_attempt(&attempt);
        vault.flush().unwrap();

        let profile = vault.calculate_profile();
        assert_eq!(profile.total_attempts, 1);
        assert!(profile.collocation.errors >= 1);
        assert!(profile.collocation.score < 70); // 扣分
        assert!(profile.top_weaknesses.contains(&"Collocations".to_string()));

        // 验证 ASCII 柱状图生成
        let ascii = profile.format_ascii_bars();
        assert!(ascii.contains("Your English Profile:"));
        assert!(ascii.contains("Collocations"));
        assert!(ascii.contains("你的主要提升机会：Collocations"));
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_vault_mastery_level_progression() {
        let path = test_dir("mastery-prog");
        let mut vault = LearningVault::open(&path);

        let prompt = "搞定了";
        let target = "We're good.";

        // 第 1 次成功 -> Learning
        let mut att = RecallAttempt {
            prompt: prompt.to_string(),
            user_attempt: "We're good.".to_string(),
            reference_expression: target.to_string(),
            timestamp: 100,
            comparison: ComparisonResult {
                status: Some(ComparisonStatus::ExactMatch),
                ..Default::default()
            },
            error_categories: vec![],
        };
        vault.record_attempt(&att);
        let items = vault.review_items();
        assert_eq!(items[0].mastery, MasteryLevel::Learning);

        // 第 2 次成功 -> Familiar
        att.timestamp = 200;
        vault.record_attempt(&att);
        let items = vault.review_items();
        assert_eq!(items[0].mastery, MasteryLevel::Familiar);

        // 第 3 次失败 -> 降级为 Learning
        att.timestamp = 300;
        att.comparison.status = Some(ComparisonStatus::Chinglish);
        vault.record_attempt(&att);
        let items = vault.review_items();
        assert_eq!(items[0].mastery, MasteryLevel::Learning);

        // 连续 4 次成功 -> Mastered
        att.comparison.status = Some(ComparisonStatus::ExactMatch);
        for i in 0..4 {
            att.timestamp = 400 + i;
            vault.record_attempt(&att);
        }
        let items = vault.review_items();
        assert_eq!(items[0].mastery, MasteryLevel::Mastered);
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_vault_clear_all_privacy_purge() {
        let path = test_dir("clear-purge");
        let mut vault = LearningVault::open(&path);

        vault.save_expression("测试", "Test", None, None);
        vault.flush().unwrap();
        assert_eq!(vault.saved_expressions().len(), 1);

        vault.clear_all().unwrap();
        assert_eq!(vault.saved_expressions().len(), 0);
        // 磁盘文件已被物理清理
        assert!(!path.join("user-saved-expressions.tsv").exists());
        assert!(!path.join("user-review-queue.json").exists());
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_vault_fsrs_queue_persistence_and_process_review() {
        let path = test_dir("fsrs-queue");
        let mut vault = LearningVault::open(&path);

        let now = jiff::Zoned::now().timestamp().as_second();
        vault.save_expression("敷衍我", "brush me off", Some("Spoken"), None);
        assert_eq!(vault.today_due_count(now), 1);
        let due_list = vault.today_due_reviews(now);
        assert_eq!(due_list.len(), 1);
        assert_eq!(due_list[0].expression, "brush me off");

        // 第一次复习评级 Good
        vault.process_review("敷衍我", "brush me off", ReviewRating::Good, now);
        let item = &vault.review_items()[0];
        assert_eq!(item.success_count, 1);
        assert!(item.stability > 1.0);
        assert!(item.due_time > now);

        vault.flush().unwrap();
        assert!(path.join("user-review-queue.json").exists());

        // 重新加载验证
        let mut reloaded = LearningVault::open(&path);
        assert_eq!(reloaded.review_items().len(), 1);
        let loaded_item = &reloaded.review_items()[0];
        assert_eq!(loaded_item.expression, "brush me off");
        assert_eq!(loaded_item.success_count, 1);

        // 触发遗忘 Again
        reloaded.process_review("敷衍我", "brush me off", ReviewRating::Again, now + 86400 * 5);
        let relapsed_item = &reloaded.review_items()[0];
        assert_eq!(relapsed_item.failure_count, 1);
        assert!(relapsed_item.stability <= 0.4);

        let _ = std::fs::remove_dir_all(&path);
    }
}
