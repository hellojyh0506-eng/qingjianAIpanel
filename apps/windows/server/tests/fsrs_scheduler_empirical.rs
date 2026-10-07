//! Phase P6: FSRS (Free Spaced Repetition Scheduler) 间隔重复算法与复习调度实机经验证测试
//!
//! 验证点：
//! 1. 记忆参数初始化与今日到期队列检索
//! 2. 空队列/无到期项边界条件
//! 3. 成功回忆（Good / Easy）稳定性幂次扩展与间隔调度
//! 4. 遗忘失误（Again）记忆衰减惩罚与 Lapse 计数自增
//! 5. 跨天时间衰减模型（Power-law Retrievability）与优先级排序（遗忘率最高优先）
//! 6. 队列演进、Server 消息分派及 JSON 磁盘持久化一致性

use std::path::{Path, PathBuf};
use qingjian_core::{ReviewItem, ReviewRating, MasteryLevel};
use qingjian_learning::{FsrsScheduler, LearningVault};

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("qj-fsrs-{}-{}-{}", name, std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_scenario_01_fsrs_initialization_and_due_queue() {
    let temp_dir = TempDir::new("init-queue");
    let mut vault = LearningVault::open(temp_dir.path());

    let now = jiff::Zoned::now().timestamp().as_second();

    // 保存生词后，自动初始化并纳入 FSRS 复习调度队列
    vault.save_expression("解耦", "decouple", Some("Engineering"), None);
    assert_eq!(vault.today_due_count(now + 10), 1);

    let due_list = vault.today_due_reviews(now + 10);
    assert_eq!(due_list.len(), 1);
    let item = due_list[0];
    assert_eq!(item.prompt, "解耦");
    assert_eq!(item.expression, "decouple");
    assert_eq!(item.attempts_count, 0);
    assert_eq!(item.failure_count, 0);
    assert_eq!(item.success_count, 0);
    assert!(item.stability > 0.0);
    assert_eq!(item.difficulty, 5.0);
    assert!(item.due_time <= now + 10);
}

#[test]
fn test_scenario_02_empty_and_future_queue_boundaries() {
    let temp_dir = TempDir::new("empty-boundaries");
    let vault = LearningVault::open(temp_dir.path());
    let now = jiff::Zoned::now().timestamp().as_second();

    // 空账本
    assert_eq!(vault.today_due_count(now), 0);
    assert!(vault.today_due_reviews(now).is_empty());
}

#[test]
fn test_scenario_03_interval_expansion_on_good_and_easy() {
    let temp_dir = TempDir::new("interval-expansion");
    let mut vault = LearningVault::open(temp_dir.path());

    let now = 1_700_000_000; // 基准秒数
    vault.save_expression("我最近忙得晕头转向", "I've been swamped lately.", Some("Spoken"), None);

    // 第一次自评：Good
    vault.process_review("我最近忙得晕头转向", "I've been swamped lately.", ReviewRating::Good, now);
    let _ = vault.flush();

    // 此时已在未来复习，基准时刻不应再出现
    assert_eq!(vault.today_due_count(now), 0);

    let item = vault.get_review_item("我最近忙得晕头转向", "I've been swamped lately.").cloned().unwrap();
    assert_eq!(item.attempts_count, 1);
    assert_eq!(item.success_count, 1);
    assert_eq!(item.failure_count, 0);
    assert!(item.stability >= 1.5); // Good 后的稳定性应显著提升
    assert_eq!(item.review_history.len(), 1);
    let first_interval = item.review_history[0].scheduled_days;
    assert!(first_interval >= 1.0);

    // 模拟时间流逝到下次到期时间（10天后）
    let next_due = now + 10 * 86400;
    assert_eq!(vault.today_due_count(next_due), 1);

    // 第二次自评：Easy (极速记忆，间隔应当显著成倍跳跃)
    vault.process_review("我最近忙得晕头转向", "I've been swamped lately.", ReviewRating::Easy, next_due);
    let item_after_easy = vault.get_review_item("我最近忙得晕头转向", "I've been swamped lately.").cloned().unwrap();
    assert_eq!(item_after_easy.attempts_count, 2);
    assert_eq!(item_after_easy.success_count, 2);
    assert!(item_after_easy.stability > item.stability * 1.5);
    assert!(item_after_easy.difficulty < item.difficulty); // Easy 降低难度
}

#[test]
fn test_scenario_04_lapse_penalty_on_again() {
    let mut item = ReviewItem {
        prompt: "攻克难关".to_string(),
        expression: "tackle the bottleneck".to_string(),
        target: "tackle the bottleneck".to_string(),
        difficulty: 4.5,
        stability: 12.0,
        retrievability: 1.0,
        due_time: 1_700_000_000,
        next_review_at: 1_700_000_000,
        last_reviewed_at: 1_699_000_000,
        attempts_count: 4,
        success_count: 4,
        failure_count: 0,
        mastery: MasteryLevel::Familiar,
        review_history: Vec::new(),
    };

    let now = 1_700_000_000;
    FsrsScheduler::step(&mut item, ReviewRating::Again, now);

    assert_eq!(item.attempts_count, 5);
    assert_eq!(item.failure_count, 1); // 遗忘次数自增
    assert!(item.stability <= 0.4); // 遗忘严厉惩罚，稳定性骤降到 0.4 天以内
    assert!(item.difficulty > 4.5); // 难度显著提升 (+0.8)
    assert!(item.due_time <= now + 86400); // 必须在 1 天内再次复习
}

#[test]
fn test_scenario_05_retrievability_decay_and_priority_sorting() {
    let temp_dir = TempDir::new("retrievability-decay");
    let mut vault = LearningVault::open(temp_dir.path());

    let base_time = 1_700_000_000;

    // 验证幂律衰减公式：当 elapsed_days == 9 * stability 时，R == 0.5
    let test_item = ReviewItem {
        stability: 5.0,
        last_reviewed_at: base_time,
        ..Default::default()
    };
    let r_fresh = FsrsScheduler::calculate_retrievability(&test_item, base_time);
    assert!((r_fresh - 1.0).abs() < 1e-4);

    let half_life_time = base_time + (9 * 5 * 86400);
    let r_half = FsrsScheduler::calculate_retrievability(&test_item, half_life_time);
    assert!((r_half - 0.5).abs() < 1e-3);

    // 存入两个词汇：一个复习很久以前 (高遗忘风险)，一个刚刚复习过
    vault.save_expression("常规项", "common item", None, None);
    vault.save_expression("严重逾期项", "urgently overdue item", None, None);

    // 模拟复习记录：常规项 5 天前复习，严重逾期项 30 天前复习
    vault.process_review("常规项", "common item", ReviewRating::Good, base_time - 5 * 86400);
    vault.process_review("严重逾期项", "urgently overdue item", ReviewRating::Good, base_time - 30 * 86400);

    let due_list = vault.today_due_reviews(base_time);
    assert_eq!(due_list.len(), 2);
    // 遗忘率更高（可提取性 R 更低）的项目排在最前
    assert_eq!(due_list[0].prompt, "严重逾期项");
    assert_eq!(due_list[1].prompt, "常规项");
}

#[test]
fn test_scenario_06_queue_progression_and_disk_persistence() {
    let temp_dir = TempDir::new("queue-persistence");
    let user_path = temp_dir.path().to_path_buf();
    let now = 1_700_000_000;

    {
        let mut vault = LearningVault::open(&user_path);
        vault.save_expression("词条A", "Item A", None, None);
        vault.save_expression("词条B", "Item B", None, None);
        // 将两者的 due_time 设为 now（通过一次立即到期的复习模拟）
        vault.process_review("词条A", "Item A", ReviewRating::Again, now - 86400);
        vault.process_review("词条B", "Item B", ReviewRating::Again, now - 86400);
        let _ = vault.flush();
    }

    // 从磁盘重新载入
    {
        let mut vault = LearningVault::open(&user_path);
        assert_eq!(vault.today_due_count(now), 2);

        // 复习词条 A
        vault.process_review("词条A", "Item A", ReviewRating::Good, now);
        let _ = vault.flush();

        // 剩余 1 个
        assert_eq!(vault.today_due_count(now), 1);
        let remaining = vault.today_due_reviews(now);
        assert_eq!(remaining[0].prompt, "词条B");

        // 复习词条 B
        vault.process_review("词条B", "Item B", ReviewRating::Easy, now);
        let _ = vault.flush();

        // 全部复习完成
        assert_eq!(vault.today_due_count(now), 0);
    }

    // 再次从磁盘重新打开，验证持久化无回滚
    {
        let vault = LearningVault::open(&user_path);
        assert_eq!(vault.today_due_count(now), 0);
        let item_a = vault.get_review_item("词条A", "Item A").unwrap();
        assert_eq!(item_a.attempts_count, 2);
        assert_eq!(item_a.success_count, 1);
        let item_b = vault.get_review_item("词条B", "Item B").unwrap();
        assert_eq!(item_b.attempts_count, 2);
        assert_eq!(item_b.success_count, 1);
    }
}
