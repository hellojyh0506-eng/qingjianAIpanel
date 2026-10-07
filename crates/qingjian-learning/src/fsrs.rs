//! FSRS (Free Spaced Repetition Scheduler) 纯 Rust 轻量学习调度算法。
//!
//! 基于现代认知心理学 D-S-R (Difficulty, Stability, Retrievability) 记忆模型：
//! - $R(t, S) = (1 + t / (9 \cdot S))^{-1}$：记忆可提取概率随时间幂律衰减；
//! - $S$：记忆稳定性（以天为单位），反映记忆在遗忘至 90% 前可维持的时长；
//! - $D$：材料内在难度（1.0 ~ 10.0），调控复习成功后的间隔扩张系数；
//! - 评级分流（Again / Hard / Good / Easy）：错误严重缩短间隔并记录 Lapse，成功呈指数级延长复习周期。

use qingjian_core::{
    ComparisonStatus, MasteryLevel, ReviewItem, ReviewLogEntry, ReviewRating,
};

/// FSRS 调度器核心
pub struct FsrsScheduler;

impl FsrsScheduler {
    /// 初始化新保存的表达条目
    pub fn init_item(prompt: &str, expression: &str, now: i64) -> ReviewItem {
        ReviewItem {
            prompt: prompt.trim().to_string(),
            expression: expression.trim().to_string(),
            target: expression.trim().to_string(),
            difficulty: 5.0,
            stability: 1.0,
            retrievability: 1.0,
            due_time: now,
            next_review_at: now,
            last_reviewed_at: 0,
            attempts_count: 0,
            success_count: 0,
            failure_count: 0,
            mastery: MasteryLevel::Learning,
            review_history: Vec::new(),
        }
    }

    /// 计算指定时间戳下的记忆可提取率 R (0.0 ~ 1.0)
    pub fn calculate_retrievability(item: &ReviewItem, now: i64) -> f32 {
        if item.last_reviewed_at == 0 || now <= item.last_reviewed_at {
            return 1.0;
        }
        let elapsed_seconds = (now - item.last_reviewed_at).max(0) as f32;
        let elapsed_days = elapsed_seconds / 86400.0;
        let stability = item.stability.max(0.1);

        // FSRS 幂律遗忘模型：当 elapsed_days == stability 时，R == 0.90
        let r = (1.0 + elapsed_days / (9.0 * stability)).powf(-1.0);
        r.clamp(0.0, 1.0)
    }

    /// 根据用户复习评级执行一次状态推进 (Step)
    pub fn step(item: &mut ReviewItem, rating: ReviewRating, now: i64) {
        let elapsed_days = if item.last_reviewed_at > 0 && now >= item.last_reviewed_at {
            (now - item.last_reviewed_at) as f32 / 86400.0
        } else {
            0.0
        };

        let r_before = Self::calculate_retrievability(item, now);
        let s_before = item.stability.max(0.1);
        let d_before = item.difficulty.clamp(1.0, 10.0);

        let (s_after, d_after, mastery_after) = match rating {
            ReviewRating::Again => {
                // 1. 遗忘惩罚 (Lapse)：稳定性锐减为原先的 20%，上限 0.4 天 (~9.6 小时)
                let new_s = (s_before * 0.20).clamp(0.1, 0.4);
                // 难度增加 +0.8
                let new_d = (0.85 * d_before + 0.15 * 5.0 + 0.8).clamp(1.0, 10.0);
                (new_s, new_d, MasteryLevel::Learning)
            }
            ReviewRating::Hard => {
                // 2. 吃力回忆：适度温和递增 1.2 倍
                let new_s = (s_before * 1.20).clamp(0.5, 365.0);
                let new_d = (0.90 * d_before + 0.10 * 5.0 + 0.4).clamp(1.0, 10.0);
                let m = if item.success_count >= 2 {
                    MasteryLevel::Familiar
                } else {
                    MasteryLevel::Learning
                };
                (new_s, new_d, m)
            }
            ReviewRating::Good => {
                // 3. 良好掌握：根据 (10 - D) 和当前 R 进行标准指数扩张
                let factor = 1.0 + (10.0 - d_before) * 0.35 * (1.0 / r_before.max(0.1)).sqrt();
                let new_s = (s_before * factor.clamp(1.3, 4.5)).clamp(0.8, 365.0);
                let new_d = (0.95 * d_before + 0.05 * 5.0).clamp(1.0, 10.0);
                let m = if item.success_count + 1 >= 4 {
                    MasteryLevel::Mastered
                } else if item.success_count + 1 >= 2 {
                    MasteryLevel::Familiar
                } else {
                    MasteryLevel::Learning
                };
                (new_s, new_d, m)
            }
            ReviewRating::Easy => {
                // 4. 轻松秒答：大幅加速扩张
                let factor = 1.0 + (10.0 - d_before) * 0.55 * (1.0 / r_before.max(0.1)).sqrt() * 1.3;
                let new_s = (s_before * factor.clamp(1.8, 7.0)).clamp(1.5, 365.0);
                let new_d = (0.90 * d_before + 0.10 * 5.0 - 0.8).clamp(1.0, 10.0);
                let m = if item.success_count + 1 >= 2 {
                    MasteryLevel::Mastered
                } else {
                    MasteryLevel::Familiar
                };
                (new_s, new_d, m)
            }
        };

        let scheduled_seconds = (s_after * 86400.0) as i64;
        let due_time = now + scheduled_seconds;

        // 记录复习日志
        item.review_history.push(ReviewLogEntry {
            rating,
            reviewed_at: now,
            scheduled_days: s_after,
            stability_before: s_before,
            stability_after: s_after,
            difficulty: d_after,
            elapsed_days,
        });

        item.attempts_count = item.attempts_count.saturating_add(1);
        if rating == ReviewRating::Again {
            item.failure_count = item.failure_count.saturating_add(1);
        } else {
            item.success_count = item.success_count.saturating_add(1);
        }

        item.stability = s_after;
        item.difficulty = d_after;
        item.retrievability = 1.0;
        item.last_reviewed_at = now;
        item.due_time = due_time;
        item.next_review_at = due_time;
        item.mastery = mastery_after;
    }

    /// 判断项目当前是否已到期需要复习
    pub fn is_due(item: &ReviewItem, now: i64) -> bool {
        item.due_time <= now
    }

    /// 判断项目在今日窗口内是否待复习（包括已逾期和今日内即将到期的）
    pub fn is_due_today(item: &ReviewItem, now: i64) -> bool {
        // 允许提前 12 小时纳入今日复习队列，确保用户打开输入法时随时可以温习
        item.due_time <= now + 12 * 3600
    }

    /// 从 Active Recall 对比结果自动推断推荐评级
    pub fn infer_rating(status: Option<ComparisonStatus>, error_count: usize) -> ReviewRating {
        match status {
            Some(ComparisonStatus::ExactMatch) | Some(ComparisonStatus::ValidB2Upgrade) => {
                ReviewRating::Good
            }
            Some(ComparisonStatus::NaturalBetter) if error_count == 0 => ReviewRating::Good,
            Some(ComparisonStatus::NaturalBetter) => ReviewRating::Hard,
            Some(ComparisonStatus::GrammarIssue) | Some(ComparisonStatus::Chinglish) => {
                ReviewRating::Again
            }
            Some(ComparisonStatus::EmptyInput) | None => ReviewRating::Again,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsrs_initial_state() {
        let item = FsrsScheduler::init_item("敷衍我", "brush me off", 1000);
        assert_eq!(item.prompt, "敷衍我");
        assert_eq!(item.expression, "brush me off");
        assert_eq!(item.difficulty, 5.0);
        assert_eq!(item.stability, 1.0);
        assert_eq!(item.retrievability, 1.0);
        assert_eq!(item.due_time, 1000);
        assert!(FsrsScheduler::is_due(&item, 1000));
    }

    #[test]
    fn test_fsrs_retrievability_decay() {
        let mut item = FsrsScheduler::init_item("忙得不可开交", "swamped", 1000);
        item.last_reviewed_at = 1000;
        item.stability = 10.0; // 稳定性 10 天

        // 刚刚复习：R = 1.0
        assert_eq!(FsrsScheduler::calculate_retrievability(&item, 1000), 1.0);

        // 经过 10 天 (864000 秒)：R 应为 0.90
        let r_10d = FsrsScheduler::calculate_retrievability(&item, 1000 + 10 * 86400);
        assert!((r_10d - 0.90).abs() < 0.01);

        // 经过 90 天：R 明显衰减
        let r_90d = FsrsScheduler::calculate_retrievability(&item, 1000 + 90 * 86400);
        assert!(r_90d < 0.55);
    }

    #[test]
    fn test_fsrs_success_interval_extension() {
        let mut item = FsrsScheduler::init_item("不太现实", "far-fetched", 1000);
        let now = 1000;

        // 连续 3 次 Good 评级
        FsrsScheduler::step(&mut item, ReviewRating::Good, now);
        let s1 = item.stability;
        assert!(s1 > 1.0, "s1 ({}) should expand beyond 1.0", s1);

        let next_time = item.due_time;
        FsrsScheduler::step(&mut item, ReviewRating::Good, next_time);
        let s2 = item.stability;
        assert!(s2 > s1, "s2 ({}) should expand beyond s1 ({})", s2, s1);

        let next_time_2 = item.due_time;
        FsrsScheduler::step(&mut item, ReviewRating::Good, next_time_2);
        let s3 = item.stability;
        assert!(s3 > s2, "s3 ({}) should expand beyond s2 ({})", s3, s2);
        assert_eq!(item.success_count, 3);
        assert_eq!(item.failure_count, 0);
    }

    #[test]
    fn test_fsrs_failure_lapse_penalty() {
        let mut item = FsrsScheduler::init_item("敷衍我", "brush me off", 1000);
        item.stability = 25.0; // 已经培养到 25 天稳定性
        item.last_reviewed_at = 1000;

        let now = 1000 + 25 * 86400;
        // 遭遇遗忘
        FsrsScheduler::step(&mut item, ReviewRating::Again, now);

        // 稳定性必须受到严厉扣减 (<= 0.4 天)
        assert!(
            item.stability <= 0.4,
            "stability should collapse to <= 0.4 days on lapse"
        );
        assert_eq!(item.mastery, MasteryLevel::Learning);
        assert_eq!(item.failure_count, 1);
        assert!(item.due_time < now + 86400, "next review due within 1 day");
    }

    #[test]
    fn test_fsrs_easy_bonus_and_mastery() {
        let mut item = FsrsScheduler::init_item("小菜一碟", "piece of cake", 1000);
        FsrsScheduler::step(&mut item, ReviewRating::Easy, 1000);
        let s1 = item.stability;
        assert!(s1 >= 2.0);

        FsrsScheduler::step(&mut item, ReviewRating::Easy, 1000 + 86400 * 3);
        assert_eq!(item.mastery, MasteryLevel::Mastered);
    }
}
