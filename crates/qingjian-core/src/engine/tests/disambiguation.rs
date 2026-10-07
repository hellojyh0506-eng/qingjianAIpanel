//! 上下文消歧专项测试集。
//!
//! 验证核心用例：
//! 1. "你不会是个啥子吧" -> 傻子 > 啥子
//! 2. "今天她吃饭但是ta" -> 她 > 他 > 它
//! 3. "我明天再去" -> 再去 > 在去
//! 4. "我人在北京" -> 北京 > 背景
//! 5. "我在北京再住一天" -> 再住一天 > 在住一天
//! 6. "他今天告诉我" -> 他 > 她
//! 7. "她今天告诉我" -> 她 > 他

use super::*;

const DISAMBIG_DICT: &str = "\
傻子\tsha zi\t9940\n\
啥子\tsha zi\t11199\n\
她\tta\t50000\n\
他\tta\t60000\n\
它\tta\t30000\n\
再去\tzai qu\t8000\n\
在去\tzai qu\t5000\n\
北京\tbei jing\t40000\n\
背景\tbei jing\t35000\n\
再住一天\tzai zhu yi tian\t2000\n\
在住一天\tzai zhu yi tian\t1000\n\
智障\tzhi zhang\t18500\n\
纸张\tzhi zhang\t15000\n\
啥\tsha\t35000\n\
傻\tsha\t40000\n\
沙\tsha\t30000\n\
杀\tsha\t30000\n\
微信\twei xin\t85000\n\
输入法\tshu ru fa\t25000\n\
微信输入法\twei xin shu ru fa\t25000\n\
为新输入法\twei xin shu ru fa\t100\n\
";

fn disambig_engine() -> Engine {
    Engine::new(Dictionary::parse(DISAMBIG_DICT).unwrap())
}

#[test]
fn test_shazi_context_disambiguation() {
    let mut engine = disambig_engine();
    engine.set_rescoring_context(Some("你不会是个".to_owned()));
    engine.set_input("shazi");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();

    // 1. "傻子" 应当在合理上下文里明显优先于 "啥子"
    let shazi_idx = texts.iter().position(|&t| t == "傻子").expect("傻子 should exist");
    let dialect_idx = texts.iter().position(|&t| t == "啥子").expect("啥子 should exist");
    assert!(shazi_idx < dialect_idx, "傻子 (index {shazi_idx}) should rank above 啥子 (index {dialect_idx})");
    assert_eq!(texts[0], "傻子");

    // 2. 绝对不能静默强制改写：用户依然拥有选择权，"啥子" 仍在候选列表中
    assert!(texts.contains(&"啥子"));
}

#[test]
fn test_pronoun_coreference_disambiguation() {
    let mut engine = disambig_engine();

    // 场景 A: 前文核心实体为女性代词 "她"
    engine.set_rescoring_context(Some("今天她吃饭但是".to_owned()));
    engine.set_input("ta");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();

    let ta_female = texts.iter().position(|&t| t == "她").expect("她 should exist");
    let ta_male = texts.iter().position(|&t| t == "他").expect("他 should exist");
    let ta_it = texts.iter().position(|&t| t == "它").expect("它 should exist");

    assert!(ta_female < ta_male, "她 (idx {ta_female}) should rank above 他 (idx {ta_male})");
    assert!(ta_male < ta_it, "他 (idx {ta_male}) should rank above 它 (idx {ta_it})");
    assert_eq!(texts[0], "她");

    // 场景 B: 前文核心实体为男性代词 "他"
    engine.set_rescoring_context(Some("他今天告诉我".to_owned()));
    engine.set_input("ta");
    let query_b = engine.query().unwrap();
    let texts_b: Vec<&str> = query_b.candidates.items.iter().map(|c| c.text.as_str()).collect();
    let male_idx = texts_b.iter().position(|&t| t == "他").expect("他 should exist");
    let female_idx = texts_b.iter().position(|&t| t == "她").expect("她 should exist");
    assert!(male_idx < female_idx, "他 should rank above 她 when preceded by 他");
    assert_eq!(texts_b[0], "他");

    // 场景 C: 前文为女性代词 "她今天告诉我"
    engine.set_rescoring_context(Some("她今天告诉我".to_owned()));
    engine.set_input("ta");
    let query_c = engine.query().unwrap();
    let texts_c: Vec<&str> = query_c.candidates.items.iter().map(|c| c.text.as_str()).collect();
    let female_idx_c = texts_c.iter().position(|&t| t == "她").expect("她 should exist");
    let male_idx_c = texts_c.iter().position(|&t| t == "他").expect("他 should exist");
    assert!(female_idx_c < male_idx_c, "她 should rank above 他 when preceded by 她");
    assert_eq!(texts_c[0], "她");
}

#[test]
fn test_zai_vs_zai_disambiguation() {
    let mut engine = disambig_engine();

    // "我明天再去"
    engine.set_rescoring_context(Some("我明天".to_owned()));
    engine.set_input("zaiqu");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts[0], "再去");

    // "我在北京再住一天"
    engine.set_rescoring_context(Some("我在北京".to_owned()));
    engine.set_input("zaizhuyitian");
    let query_stay = engine.query().unwrap();
    let texts_stay: Vec<&str> = query_stay.candidates.items.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts_stay[0], "再住一天");
}

#[test]
fn test_beijing_location_disambiguation() {
    let mut engine = disambig_engine();

    // "我人在北京"
    engine.set_rescoring_context(Some("我人在".to_owned()));
    engine.set_input("beijing");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts[0], "北京");
    let bj_idx = texts.iter().position(|&t| t == "北京").unwrap();
    let bg_idx = texts.iter().position(|&t| t == "背景").unwrap();
    assert!(bj_idx < bg_idx);
}

#[test]
fn test_case_a_zhizhang_disambiguation() {
    let mut engine = disambig_engine();
    engine.set_rescoring_context(Some("是很".to_owned()));
    engine.set_input("zhizhang");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();

    let zhizhang_idx = texts.iter().position(|&t| t == "智障").expect("智障 should exist");
    let zhizhang_paper_idx = texts.iter().position(|&t| t == "纸张").expect("纸张 should exist");

    assert!(zhizhang_idx < zhizhang_paper_idx, "智障 (idx {zhizhang_idx}) must rank above 纸张 (idx {zhizhang_paper_idx}) after 是很");
    assert_eq!(texts[0], "智障");
    assert!(texts.contains(&"纸张"), "纸张 must remain selectable");
}

#[test]
fn test_case_b_sha_disambiguation() {
    let mut engine = disambig_engine();
    engine.set_rescoring_context(Some("我不知道你在说".to_owned()));
    engine.set_input("sha");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();

    let sha_what = texts.iter().position(|&t| t == "啥").expect("啥 should exist");
    let sha_fool = texts.iter().position(|&t| t == "傻").expect("傻 should exist");

    assert!(sha_what < sha_fool, "啥 (idx {sha_what}) must rank above 傻 (idx {sha_fool}) after 我不知道你在说");
    assert_eq!(texts[0], "啥");
    assert!(texts.contains(&"傻"), "傻 must remain selectable");
}

#[test]
fn test_case_e_weixinshurufa_ranking() {
    let mut engine = disambig_engine();
    engine.set_input("weixinshurufa");
    let query = engine.query().unwrap();
    let texts: Vec<&str> = query.candidates.items.iter().map(|c| c.text.as_str()).collect();

    let weixin_ime = texts.iter().position(|&t| t == "微信输入法").expect("微信输入法 should exist");
    if let Some(wei_xin_bad) = texts.iter().position(|&t| t == "为新输入法") {
        assert!(weixin_ime < wei_xin_bad, "微信输入法 must rank above 为新输入法");
    }
    assert_eq!(texts[0], "微信输入法");
}

#[test]
fn test_case_g_incomplete_pinyin_disambiguation() {
    let mut engine = disambig_engine();

    // 1. Incomplete prefix 's' after "我不知道你在说"
    engine.set_rescoring_context(Some("我不知道你在说".to_owned()));
    engine.set_input("s");
    let query_s = engine.query().unwrap();
    let texts_s: Vec<&str> = query_s.candidates.items.iter().map(|c| c.text.as_str()).collect();
    let sha_idx = texts_s.iter().position(|&t| t == "啥").expect("啥 should match prefix s");
    let fool_idx = texts_s.iter().position(|&t| t == "傻").expect("傻 should match prefix s");
    assert!(sha_idx < fool_idx, "啥 should outrank 傻 even for incomplete single-letter prefix 's'");

    // 2. Incomplete prefix 'zh' after "是很"
    engine.set_rescoring_context(Some("是很".to_owned()));
    engine.set_input("zh");
    let query_zh = engine.query().unwrap();
    let texts_zh: Vec<&str> = query_zh.candidates.items.iter().map(|c| c.text.as_str()).collect();
    let zz_idx = texts_zh.iter().position(|&t| t == "智障").expect("智障 should match prefix zh");
    let paper_idx = texts_zh.iter().position(|&t| t == "纸张").expect("纸张 should match prefix zh");
    assert!(zz_idx < paper_idx, "智障 should outrank 纸张 even for incomplete prefix 'zh'");
}

fn find_real_dict() -> Option<Dictionary> {
    let candidates = [
        "../../data/generated/dict.qj",
        "data/generated/dict.qj",
        "E:/Qingjian/data/generated/dict.qj",
        "../../assets/lexicon/dict.tsv",
        "assets/lexicon/dict.tsv",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            if let Ok(dict) = Dictionary::from_path(std::path::Path::new(path)) {
                return Some(dict);
            }
        }
    }
    None
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct GateResult {
    pub name: &'static str,
    pub context: Option<&'static str>,
    pub input: &'static str,
    pub expected_top1: &'static str,
    pub before_top1: String,
    pub before_top3: Vec<String>,
    pub before_micros: u128,
    pub after_top1: String,
    pub after_top3: Vec<String>,
    pub after_micros: u128,
    pub top1_changed: bool,
    pub top1_passed: bool,
    pub target_in_top3: bool,
}

#[test]
fn test_real_user_experience_gate_benchmark() {
    let dict = find_real_dict().expect("Real dictionary (dict.qj or dict.tsv) must be present for experience gate");
    let mut engine = Engine::new(dict);

    let test_cases: Vec<(&'static str, Option<&'static str>, &'static str, &'static str)> = vec![
        // 核心用例 1: 是很 zhi'zhang -> 智障
        ("Case 1: 是很 zhi'zhang", Some("是很"), "zhizhang", "智障"),
        // 核心用例 2: 我不知道你在说 sha -> 啥
        ("Case 2: 我不知道你在说 sha", Some("我不知道你在说"), "sha", "啥"),
        // 核心用例 3: 今天她吃饭但是 ta -> 她
        ("Case 3: 今天她吃饭但是 ta", Some("今天她吃饭但是"), "ta", "她"),
        // 核心用例 4: 你不会是个 shazi ba -> 傻子
        ("Case 4-1: 你不会是个 shazi", Some("你不会是个"), "shazi", "傻子"),
        ("Case 4-2: 你不会是个 shaziba", Some("你不会是个"), "shaziba", "傻子吧"),
        // 核心用例 5: wei xin shu ru fa -> 微信输入法
        ("Case 5: wei xin shu ru fa", None, "weixinshurufa", "微信输入法"),
        // 核心用例 6a: 的 / 地 / 得
        ("Case 6a-1: 跑 de -> 得", Some("跑"), "de", "得"),
        ("Case 6a-2: 慢慢 de -> 地", Some("慢慢"), "de", "地"),
        ("Case 6a-3: 我 de -> 的", Some("我"), "de", "的"),
        // 核心用例 6b: 在 / 再
        ("Case 6b-1: 我明天 zai -> 再", Some("我明天"), "zai", "再"),
        ("Case 6b-2: 我明天 zaiqu -> 再去", Some("我明天"), "zaiqu", "再去"),
        ("Case 6b-3: 我人在 beijing -> 北京", Some("我人在"), "beijing", "北京"),
        ("Case 6b-4: 我在北京 zaizhuyitian -> 再住一天", Some("我在北京"), "zaizhuyitian", "再住一天"),
        // 核心用例 6c: 他 / 她 / 它
        ("Case 6c-1: 他今天告诉我 ta -> 他", Some("他今天告诉我"), "ta", "他"),
        ("Case 6c-2: 她今天告诉我 ta -> 她", Some("她今天告诉我"), "ta", "她"),
        // 核心用例 6d: 做 / 作
        ("Case 6d-1: 打算 zuo -> 做", Some("打算"), "zuo", "做"),
        ("Case 6d-2: 好 zuo -> 作", Some("好"), "zuo", "作"),
        // 核心用例 6e: 哪 / 那
        ("Case 6e-1: 你想去 na -> 哪", Some("你想去"), "na", "哪"),
        ("Case 6e-2: 就在 na -> 那", Some("就在"), "na", "那"),
        // 核心用例 6f: 全局长句语义重排
        ("Case 6f: 全局语义重排裙子", None, "tayouyijianhenbangdequnzi", "她有一件很棒的裙子"),
    ];

    let mut results = Vec::new();
    println!("\n==================== REAL USER EXPERIENCE GATE BENCHMARK ====================");

    for (name, ctx, input, expected) in test_cases {
        // --- 1. Baseline WITHOUT context (Before) ---
        engine.set_rescoring_context(None);
        engine.set_input(input);
        let t0 = std::time::Instant::now();
        let query_before = engine.query().unwrap();
        let before_micros = t0.elapsed().as_micros();
        let before_texts: Vec<String> = query_before.candidates.items.iter().map(|c| c.text.clone()).collect();
        let before_top1 = before_texts.first().cloned().unwrap_or_default();
        let before_top3: Vec<String> = before_texts.iter().take(3).cloned().collect();

        // --- 2. Hardened WITH context (After) ---
        if let Some(c) = ctx {
            engine.set_rescoring_context(Some(c.to_owned()));
        } else {
            engine.set_rescoring_context(None);
        }
        engine.set_input(input);
        let t1 = std::time::Instant::now();
        let query_after = engine.query().unwrap();
        let after_micros = t1.elapsed().as_micros();
        let after_texts: Vec<String> = query_after.candidates.items.iter().map(|c| c.text.clone()).collect();
        let after_top1 = after_texts.first().cloned().unwrap_or_default();
        let after_top3: Vec<String> = after_texts.iter().take(3).cloned().collect();

        let top1_passed = after_top1 == expected;
        let target_in_top3 = after_top3.iter().any(|t| t == expected);
        let top1_changed = before_top1 != after_top1;

        println!(
            "[{}] Expected: '{}' | Before: Top1='{}', Top3={:?} ({}µs) | After: Top1='{}', Top3={:?} ({}µs) | Top1 Match: {}",
            name, expected, before_top1, before_top3, before_micros, after_top1, after_top3, after_micros, if top1_passed { "PASS" } else { "FAIL" }
        );

        results.push(GateResult {
            name,
            context: ctx,
            input,
            expected_top1: expected,
            before_top1,
            before_top3,
            before_micros,
            after_top1,
            after_top3,
            after_micros,
            top1_changed,
            top1_passed,
            target_in_top3,
        });
    }

    println!("===============================================================================\n");

    let total = results.len();
    let passed = results.iter().filter(|r| r.top1_passed).count();
    let top3_passed = results.iter().filter(|r| r.target_in_top3).count();
    println!("Total Cases: {}, Top-1 Passed: {}/{}, In Top-3: {}/{}", total, passed, total, top3_passed, total);

    for r in &results {
        assert!(r.target_in_top3, "FAIL: Expected '{}' must at least be in Top-3 for {}", r.expected_top1, r.name);
        assert!(r.top1_passed, "FAIL: Expected '{}' must be Top-1 for {}, got '{}'", r.expected_top1, r.name, r.after_top1);
    }
}

#[test]
fn test_case_6_global_sentence_agreement_qunzi() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);
    engine.set_input("tayouyijianhenbangdequnzi");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty());
    assert_eq!(texts[0], "她有一件很棒的裙子");
}

#[test]
fn test_tachongdianlema_grammar_ranking_overrides_stray_choices() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);

    // 模拟用户历史曾误选 "骂" 为 ma 的首选词 (choice=2, user ngram 了 -> 骂)
    engine.learner_mut().record_choice("ma", "骂");
    engine.learner_mut().record_choice("ma", "骂");

    // 1. 无前文单音节 ma 输入时，历史选择可起软性偏好
    engine.set_input("ma");
    let query_single = engine.query().unwrap();
    let single_texts: Vec<String> = query_single.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(single_texts.contains(&"吗".to_string()));
    assert!(single_texts.contains(&"骂".to_string()));

    // 2. 整句输入: "ta'chong'dian'le'ma" -> 句法助词吗必须绝对压倒及物动词骂
    engine.set_input("ta'chong'dian'le'ma");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty(), "Candidates must not be empty");
    assert_eq!(texts[0], "他充电了吗", "Top-1 must be 他充电了吗, got: {:?}", &texts[..texts.len().min(5)]);

    // 3. 其它疑问虚词句式验证
    let pairs = [
        ("ni'chi'fan'le'ma", "你吃饭了吗"),
        ("ni'zai'gan'ma'ne", "你在干嘛呢"),
        ("ni'zen'me'le'ne", "你怎么了呢"),
        ("ta'qu'le'ma", "他去了吗"),
    ];
    for (input, expected) in pairs {
        engine.set_input(input);
        let q = engine.query().unwrap();
        let list: Vec<String> = q.candidates.items.iter().map(|c| c.text.clone()).collect();
        assert_eq!(list[0], expected, "Input {} must produce Top-1 {}, got: {:?}", input, expected, &list[..list.len().min(5)]);
    }
}

#[test]
fn test_daodi_vs_daode_clause_initial_ranking() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);

    // 在标点或句首后，到底是极高频篇章关联副词，而到的不能作为句首
    engine.set_rescoring_context(Some("! ! ".to_owned()));
    engine.set_input("dao'di");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty());
    assert_eq!(texts[0], "到底", "Top-1 after '! ! ' must be 到底, got {:?}", &texts[..texts.len().min(5)]);

    let daodi_pos = texts.iter().position(|t| t == "到底").expect("到底 must exist");
    if let Some(daode_pos) = texts.iter().position(|t| t == "到的") {
        assert!(daodi_pos < daode_pos, "到底 (idx {}) must rank above 到的 (idx {})", daodi_pos, daode_pos);
    }
}

#[test]
fn test_pinyin_error_recovery_nidaodisingbuximg() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);

    // 核心用例: 用户键盘快速输入产生噪声 apostrophe、单字母碎片、声母混淆 (s->x) 与键邻错位 (m->n)
    engine.set_input("ni'dao'di'si'n'g'bu'xi'm'g");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty(), "Candidates must not be empty for noisy input");
    assert_eq!(texts[0], "你到底行不行", "Top-1 must recover 你到底行不行, got {:?}", &texts[..texts.len().min(5)]);

    // 常规完整长句基线用例
    let standard_cases = [
        ("ni'zen'me'hai'bu'lai", "你怎么还不来"),
        ("wo'jue'de'zhe'ge'ting'bu'cuo", "我觉得这个挺不错"),
    ];
    for (input, expected) in standard_cases {
        engine.set_input(input);
        let q = engine.query().unwrap();
        let list: Vec<String> = q.candidates.items.iter().map(|c| c.text.clone()).collect();
        assert_eq!(list[0], expected, "Input {} must produce Top-1 {}, got: {:?}", input, expected, &list[..list.len().min(5)]);
    }
}

#[test]
fn test_abbreviation_benchmark_six_cases() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);
    let cases = [
        ("w'z'd'f'l", "我真的服了"),
        ("z'j'd'l", "真的假的"),
        ("t'l'p'l", "太离谱了"),
        ("n's'b", "你是不是"),
        ("b's'b", "不是吧"),
        ("w'f'l", "我服了"),
    ];
    for (input, expected) in cases {
        engine.set_input(input);
        let query = engine.query().unwrap();
        let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
        assert!(!texts.is_empty(), "Candidates must not be empty for {}", input);
        assert_eq!(
            texts[0], expected,
            "Input {} must produce Top-1 {}, got: {:?}",
            input,
            expected,
            &texts[..texts.len().min(5)]
        );
    }
}

#[test]
fn test_abbreviation_contextual_multi_intent() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let mut engine = Engine::new(dict);

    // 1. z'j'd'l 上下文消歧：传闻吃惊 -> 真的假的 vs 归属陈述 -> 自己的了
    engine.set_rescoring_context(Some("他说他发财了".to_owned()));
    engine.set_input("z'j'd'l");
    let q_rumor = engine.query().unwrap();
    let texts_rumor: Vec<String> = q_rumor.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert_eq!(texts_rumor[0], "真的假的", "Context '他说他发财了' must prefer 真的假的, got {:?}", &texts_rumor[..texts_rumor.len().min(5)]);

    engine.set_rescoring_context(Some("这是我".to_owned()));
    engine.set_input("z'j'd'l");
    let q_own = engine.query().unwrap();
    let texts_own: Vec<String> = q_own.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(
        texts_own[0] == "自己的了" || texts_own[0] == "自己的",
        "Context '这是我' must prefer 自己的了/自己的, got {:?}",
        &texts_own[..texts_own.len().min(5)]
    );

    // 2. n's'b 上下文消歧：推测反问 -> 你是不是 vs 工作考勤 -> 你上班 vs 空间方位 -> 你身边
    engine.set_rescoring_context(Some("我看你脸色不好".to_owned()));
    engine.set_input("n's'b");
    let q_guess = engine.query().unwrap();
    let texts_guess: Vec<String> = q_guess.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert_eq!(texts_guess[0], "你是不是", "Context '我看你脸色不好' must prefer 你是不是, got {:?}", &texts_guess[..texts_guess.len().min(5)]);

    engine.set_rescoring_context(Some("今天周一".to_owned()));
    engine.set_input("n's'b");
    let q_work = engine.query().unwrap();
    let texts_work: Vec<String> = q_work.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert_eq!(texts_work[0], "你上班", "Context '今天周一' must prefer 你上班, got {:?}", &texts_work[..texts_work.len().min(5)]);

    engine.set_rescoring_context(Some("留在他".to_owned()));
    engine.set_input("n's'b");
    let q_loc = engine.query().unwrap();
    let texts_loc: Vec<String> = q_loc.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert_eq!(texts_loc[0], "你身边", "Context '留在他' must prefer 你身边, got {:?}", &texts_loc[..texts_loc.len().min(5)]);
}

#[test]
fn test_current_input_vs_next_token_prediction_separation() {
    let dict = find_real_dict().expect("Real dictionary must be present");
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    struct TestRecorder(std::sync::Arc<std::sync::Mutex<Vec<PredictionRequest>>>);
    impl Predictor for TestRecorder {
        fn policy(&self) -> PredictionPolicy {
            PredictionPolicy::default()
        }
        fn submit(&mut self, request: PredictionRequest) {
            self.0.lock().unwrap().push(request);
        }
        fn poll(&mut self) -> Option<Prediction> {
            None
        }
    }
    let mut engine = Engine::new(dict).with_predictor(Box::new(TestRecorder(sent.clone())));

    // Case A: 当前输入 wo'x'h'n -> 本地首选必须是 "我喜欢你"
    engine.set_input("wo'x'h'n");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty(), "Candidates must not be empty for wo'x'h'n");
    assert_eq!(texts[0], "我喜欢你", "Local Top-1 candidate must be 我喜欢你, got {:?}", &texts[..texts.len().min(5)]);

    // 验证预测请求构造：current_expression 必须严格绑定当前 Top-1 候选 "我喜欢你"
    let surrounding = Some(crate::SurroundingText {
        before: "我想对你说，".to_owned(),
        after: String::new(),
    });
    let seq = engine.request_prediction(surrounding, &query.candidates.items);
    assert!(seq.is_some(), "Prediction request must be dispatched");
    let reqs = sent.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].current_expression, "我喜欢你");
}

#[test]
fn test_henan_dialect_disambiguation() {
    let dict = Dictionary::parse("\
河南话\the nan hua\t25000\n\
河南话\then an hua\t20000\n\
很那话\then an hua\t5000\n\
我嘞个豆\two lei ge dou\t15000\n\
我日他哥\two ri ta ge\t12000\n\
妮儿\tni er\t12000\n\
得劲\tde jin\t15000\n\
弄啥哩\tnong sha li\t12000\n\
").unwrap();
    let mut engine = Engine::new(dict);

    // 1. hen'an'hua -> 河南话 必须压倒 很那话
    engine.set_input("hen'an'hua");
    let query = engine.query().unwrap();
    let texts: Vec<String> = query.candidates.items.iter().map(|c| c.text.clone()).collect();
    assert!(!texts.is_empty());
    assert_eq!(texts[0], "河南话", "Top 1 for hen'an'hua must be 河南话, got {:?}", texts);
    if let Some(pos) = texts.iter().position(|t| t == "很那话") {
        let henan_pos = texts.iter().position(|t| t == "河南话").unwrap();
        assert!(henan_pos < pos, "河南话 must rank higher than 很那话");
    }

    // 2. he'nan'hua -> 河南话 Top 1
    engine.set_input("he'nan'hua");
    let query2 = engine.query().unwrap();
    assert_eq!(query2.candidates.items[0].text, "河南话");

    // 3. wo'lei'ge'dou -> 我嘞个豆
    engine.set_input("wo'lei'ge'dou");
    let query3 = engine.query().unwrap();
    assert_eq!(query3.candidates.items[0].text, "我嘞个豆");

    // 4. wo'ri'ta'ge -> 我日他哥
    engine.set_input("wo'ri'ta'ge");
    let query4 = engine.query().unwrap();
    assert_eq!(query4.candidates.items[0].text, "我日他哥");
}

#[test]
fn test_real_qj_henan_dialect() {
    let dict_path = std::path::Path::new("../../data/generated/dict.qj");
    if dict_path.exists() {
        let dict = Dictionary::from_path(dict_path).unwrap();
        let mut engine = Engine::new(dict);

        // 1. hen'an'hua
        engine.set_input("hen'an'hua");
        let q1 = engine.query().unwrap();
        assert!(!q1.candidates.items.is_empty());
        assert_eq!(q1.candidates.items[0].text, "河南话", "Expected 河南话 for hen'an'hua, got {}", q1.candidates.items[0].text);

        // 2. he'nan'hua
        engine.set_input("he'nan'hua");
        let q2 = engine.query().unwrap();
        assert_eq!(q2.candidates.items[0].text, "河南话");

        // 3. wo'lei'ge'dou
        engine.set_input("wo'lei'ge'dou");
        let q3 = engine.query().unwrap();
        assert_eq!(q3.candidates.items[0].text, "我嘞个豆");

        // 4. wo'ri'ta'ge
        engine.set_input("wo'ri'ta'ge");
        let q4 = engine.query().unwrap();
        assert_eq!(q4.candidates.items[0].text, "我日他哥");

        // 5. ni'er
        engine.set_input("ni'er");
        let q5 = engine.query().unwrap();
        assert_eq!(q5.candidates.items[0].text, "妮儿");

        // 6. nong'sha'li
        engine.set_input("nong'sha'li");
        let q6 = engine.query().unwrap();
        assert_eq!(q6.candidates.items[0].text, "弄啥哩");
    }
}

#[test]
fn test_debug_jieou_and_wsm() {
    let dict_path = std::path::Path::new("../../data/generated/dict.qj");
    if dict_path.exists() {
        let dict = Dictionary::from_path(dict_path).unwrap();
        let mut engine = Engine::new(dict);

        // 1. jie'ou 必须以 解耦 为首选，不得被 结构 / 节奏 抢占
        engine.set_input("jie'ou");
        let q = engine.query().unwrap();
        assert!(!q.candidates.items.is_empty());
        assert_eq!(q.candidates.items[0].text, "解耦", "Top-1 for jie'ou must be 解耦, got {}", q.candidates.items[0].text);

        // 2. w's'm 必须以 为什么 为首选，我是吗 严禁排在前面
        engine.set_input("w's'm");
        let q2 = engine.query().unwrap();
        assert!(!q2.candidates.items.is_empty());
        assert_eq!(q2.candidates.items[0].text, "为什么", "Top-1 for w's'm must be 为什么, got {}", q2.candidates.items[0].text);
        if let Some(pos_wsm) = q2.candidates.items.iter().position(|c| c.text == "我是吗") {
            let pos_wei = q2.candidates.items.iter().position(|c| c.text == "为什么").unwrap();
            assert!(pos_wei < pos_wsm, "为什么 (idx {}) must rank above 我是吗 (idx {})", pos_wei, pos_wsm);
        }

        // 3. wsm 连打缩写也必须首选 为什么
        engine.set_input("wsm");
        let q3 = engine.query().unwrap();
        assert!(!q3.candidates.items.is_empty());
        assert_eq!(q3.candidates.items[0].text, "为什么", "Top-1 for wsm must be 为什么, got {}", q3.candidates.items[0].text);
    }
}

#[test]
fn test_compile_dict_qj() {
    let source = std::path::Path::new("../../assets/lexicon/dict.tsv");
    if source.exists() {
        let dest = std::path::Path::new("../../data/generated");
        let res = qingjian_dictionary::import::import(source, dest).unwrap();
        println!("Compiled dict to {:?}, entries: {}", res.path, res.entries);
        let e_dest = std::path::Path::new("E:/Qingjian/data/generated");
        if e_dest.exists() {
            let _ = qingjian_dictionary::import::import(source, e_dest);
            println!("Copied to E:/Qingjian/data/generated");
        }
    }
}
