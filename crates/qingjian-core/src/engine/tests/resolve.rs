//! Resolve：请求怎么组、回复怎么校验、通过校验的结果怎么上屏。

use super::*;
use crate::{CandidateLayout, ResolveApplied};

const DICTIONARY: &str = "开发\tkai fa\t9000\n开发者\tkai fa zhe\t3000\n开饭\tkai fan\t800\n\
你好\tni hao\t9000\n你\tni\t9000\n好\thao\t9000\n吗\tma\t9000\n你好吗\tni hao ma\t3000\n\
睡觉\tshui jiao\t5000\n水饺\tshui jiao\t3000\n现在\txian zai\t9000\n";

type Submitted = std::rc::Rc<std::cell::RefCell<Vec<PredictionRequest>>>;

fn resolve_engine(replies: Vec<Prediction>) -> (Engine, Submitted) {
    let submitted: Submitted = Default::default();
    let predictor = EchoPredictor {
        submitted: submitted.clone(),
        replies,
        sentence: false,
    };
    let engine =
        Engine::new(Dictionary::parse(DICTIONARY).unwrap()).with_predictor(Box::new(predictor));
    (engine, submitted)
}

fn reply(sequence: u64, pinyin: &str, pick: Option<&str>) -> Prediction {
    Prediction {
        sequence,
        sentence: Some(pinyin.into()),
        words: pick.into_iter().map(|text| cloud(text, &[])).collect(),
        ..Default::default()
    }
}

fn local(engine: &mut Engine) -> Vec<Candidate> {
    engine.query().unwrap().candidates.items
}

fn context(before: &str) -> Option<SurroundingText> {
    Some(SurroundingText {
        before: before.into(),
        after: String::new(),
    })
}

#[test]
fn request_carries_numbered_candidates_with_readings_and_trimmed_context() {
    let (mut engine, submitted) = resolve_engine(Vec::new());
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    let surrounding = SurroundingText {
        before: "我们今天一起来".into(),
        after: "吧，好不好".into(),
    };
    assert_eq!(engine.request_resolve(Some(surrounding), &items), Some(1));
    let request = submitted.borrow()[0].clone();
    assert_eq!(request.kind, PredictionKind::Resolve);
    assert_eq!(request.letters, "kaifazhes");
    assert_eq!(request.before, "天一起来");
    assert_eq!(request.after, "吧，");
    assert!(!request.resolve_candidates.is_empty());
    assert!(request.resolve_candidates.len() <= 12);
    let developer = request
        .resolve_candidates
        .iter()
        .find(|candidate| candidate.text == "开发者")
        .expect("the typo'd string still lists 开发者");
    assert_eq!(developer.reading, "kai'fa'zhe");
    assert_eq!(request.candidates.len(), request.resolve_candidates.len());
}

#[test]
fn request_is_skipped_for_modes_and_inputs_that_cannot_benefit() {
    // 单音节、没有上下文
    let (mut engine, submitted) = resolve_engine(Vec::new());
    engine.set_input("shui");
    let items = local(&mut engine);
    assert_eq!(engine.request_resolve(None, &items), None);
    // 短到不足 4 个字母、也不足 2 个音节：没有上下文不发，有上下文才发
    engine.set_input("hao");
    let items = local(&mut engine);
    assert_eq!(engine.request_resolve(None, &items), None);
    assert!(engine.request_resolve(context("你"), &items).is_some());
    // 本地首选正好读得出整串、没有上下文、没有纠正候选：本地把握很大
    engine.set_input("kaifazhe");
    let items = local(&mut engine);
    assert_eq!(engine.request_resolve(None, &items), None);
    // 同一串有上下文就发
    assert!(engine.request_resolve(context("我是"), &items).is_some());
    // 私密输入、英文模式、大写字母不发
    engine.set_private(true);
    assert_eq!(engine.request_resolve(context("我是"), &items), None);
    engine.set_private(false);
    engine.set_input("KaifaZhe");
    assert_eq!(engine.request_resolve(context("我是"), &items), None);
    assert_eq!(submitted.borrow().len(), 2);
}

#[test]
fn same_letters_only_pick_from_the_listed_candidates() {
    let (mut engine, _) = resolve_engine(vec![
        reply(2, "shuijiao", Some("水饺")),
        reply(1, "shuijiao", Some("不在列表里")),
    ]);
    engine.set_input("shuijiao");
    let items = local(&mut engine);
    assert_eq!(
        engine.request_resolve(context("太困了，我想去"), &items),
        Some(1)
    );
    let rejected = engine.poll_prediction().expect("reply 1");
    assert!(rejected.resolved.is_none());

    assert_eq!(
        engine.request_resolve(context("太困了，我想去"), &items),
        Some(2)
    );
    let accepted = engine.poll_prediction().expect("reply 2");
    let resolved = accepted.resolved.expect("listed candidate passes");
    assert_eq!(resolved.candidate.text, "水饺");
    assert!(!resolved.pinyin_changed);
    assert_eq!(resolved.placement, ResolvePlacement::First);
    assert!(resolved.candidate.corrected);
}

#[test]
fn context_free_homophone_swap_only_moves_within_the_first_three() {
    let (mut engine, _) = resolve_engine(vec![reply(1, "shuijiao", Some("水饺"))]);
    engine.set_input("shuijiao");
    let mut items = local(&mut engine);
    // 没有上下文又没有纠正候选时本地把握很大，不会发请求；给第 1 位标个纠正才会走到这里
    items[0].corrected = true;
    assert_eq!(engine.request_resolve(None, &items), Some(1));
    let resolved = engine.poll_prediction().unwrap().resolved.unwrap();
    assert_eq!(resolved.placement, ResolvePlacement::WithinThree);
    assert!(!resolved.candidate.corrected);
}

#[test]
fn corrected_pinyin_is_requeried_locally_and_commit_consumes_the_whole_typed_string() {
    let (mut engine, _) = resolve_engine(vec![reply(1, "kaifazhe", Some("开发者"))]);
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    assert_eq!(engine.request_resolve(None, &items), Some(1));
    let resolved = engine
        .poll_prediction()
        .unwrap()
        .resolved
        .expect("valid correction");
    assert_eq!(resolved.candidate.text, "开发者");
    assert!(resolved.pinyin_changed);
    assert_eq!(resolved.placement, ResolvePlacement::First);
    assert!(resolved.candidate.corrected);
    assert_eq!(resolved.candidate.syllables.concat(), "kaifazhe");

    assert_eq!(engine.commit(&resolved.candidate), "开发者");
    assert!(engine.composition().is_empty(), "整串都应上屏，不留尾巴");
}

#[test]
fn picked_text_outside_the_requeried_results_falls_back_to_the_top_candidate() {
    let (mut engine, _) = resolve_engine(vec![reply(1, "kaifazhe", Some("不存在的词"))]);
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    engine.request_resolve(None, &items);
    let resolved = engine.poll_prediction().unwrap().resolved.unwrap();
    assert_eq!(resolved.candidate.text, "开发者");

    // pick 为 0（没有合适的）同样取新结果第 1 个
    let (mut engine, _) = resolve_engine(vec![reply(1, "kaifazhe", None)]);
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    engine.request_resolve(None, &items);
    let resolved = engine.poll_prediction().unwrap().resolved.unwrap();
    assert_eq!(resolved.candidate.text, "开发者");
}

#[test]
fn invalid_corrections_without_a_usable_pick_are_dropped() {
    let cases = [
        // 编辑距离超限
        ("kaifazhes", "woxiangchi"),
        // 不能完整切分
        ("kaifazhes", "kaifazh"),
        // 含非字母
        ("kaifazhes", "kai'fa'zhe"),
        // 切得开但词库里没有
        ("kaifazhes", "kaifazha"),
    ];
    for (typed, pinyin) in cases {
        let (mut engine, _) = resolve_engine(vec![reply(1, pinyin, None)]);
        engine.set_input(typed);
        let items = local(&mut engine);
        engine.request_resolve(None, &items);
        let prediction = engine.poll_prediction().expect("reply is delivered");
        assert!(prediction.resolved.is_none(), "{typed} -> {pinyin}");
    }
}

#[test]
fn reply_after_the_input_changed_is_dropped() {
    // 请求之后又发了新请求：旧结果序号过期
    let (mut engine, _) = resolve_engine(vec![reply(1, "kaifazhe", Some("开发者"))]);
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    engine.request_resolve(None, &items);
    engine.request_resolve(context("我是"), &items);
    assert!(engine.poll_prediction().is_none(), "序号已过期");

    // 序号没变但作用域变了
    for typed in ["kaifazhex", "kaifazhe"] {
        let (mut engine, _) = resolve_engine(vec![reply(1, "kaifazhe", Some("开发者"))]);
        engine.set_input("kaifazhes");
        let items = local(&mut engine);
        engine.request_resolve(None, &items);
        engine.set_input(typed);
        let prediction = engine.poll_prediction().expect("same lane sequence");
        assert!(prediction.resolved.is_none(), "{typed}");
    }
}

#[test]
fn query_reading_leaves_the_composition_untouched() {
    let (mut engine, _) = resolve_engine(Vec::new());
    engine.set_input("kaifazhes");
    let items = engine.query_reading("kaifazhe");
    assert!(items.iter().any(|candidate| candidate.text == "开发者"));
    assert_eq!(engine.composition().text(), "kaifazhes");
    assert!(engine.query_reading("").is_empty());
    assert!(engine.query_reading("Kai").is_empty());
}

#[test]
fn repeatedly_rejected_scope_only_leaves_a_hint() {
    let submitted: Submitted = Default::default();
    let predictor = EchoPredictor {
        submitted,
        replies: vec![reply(1, "kaifazhe", Some("开发者"))],
        sentence: false,
    };
    let mut engine = Engine::new(Dictionary::parse(DICTIONARY).unwrap())
        .with_predictor(Box::new(predictor))
        .with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.learner_mut().record_rejected_correction("kaifazhes");
    engine.learner_mut().record_rejected_correction("kaifazhes");
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    engine.request_resolve(None, &items);
    let resolved = engine.poll_prediction().unwrap().resolved.unwrap();
    assert_eq!(resolved.placement, ResolvePlacement::Hint);
    assert!(!resolved.candidate.corrected);
}

#[test]
fn layout_applies_resolved_results_by_placement() {
    let (mut engine, _) = resolve_engine(vec![reply(1, "shuijiao", Some("水饺"))]);
    engine.set_input("shuijiao");
    let items = local(&mut engine);
    let first = items[0].text.clone();
    assert_eq!(first, "睡觉");
    engine.request_resolve(context("太困了，我想去"), &items);
    let resolved = engine.poll_prediction().unwrap().resolved.unwrap();

    let mut layout = CandidateLayout::new(items.clone(), 9, 0);
    assert_eq!(layout.apply_resolved(&resolved), ResolveApplied::Placed);
    let top = layout.candidate(0).unwrap();
    assert_eq!(top.text, "水饺");
    assert!(top.corrected);
    assert_eq!(
        layout.candidate(1).unwrap().text,
        first,
        "原第 1 位降到第 2 位"
    );
    assert_eq!(layout.apply_resolved(&resolved), ResolveApplied::Unchanged);

    let hint = Resolved {
        placement: ResolvePlacement::Hint,
        ..resolved.clone()
    };
    let mut layout = CandidateLayout::new(items, 9, 0);
    assert_eq!(layout.apply_resolved(&hint), ResolveApplied::Hint);
    assert_eq!(layout.candidate(0).unwrap().text, first);
}

#[test]
fn the_picked_candidates_own_reading_wins_over_a_miscopied_pinyin() {
    // 模型选对了 开发者，却把拼音抄成了 kaifazhee（合法切分、差一个字母，但不是它的读音）
    for pinyin in [Some("kaifazhee"), Some("kai fa zhe"), None] {
        let (mut engine, _) = resolve_engine(vec![Prediction {
            sequence: 1,
            sentence: pinyin.map(Into::into),
            words: vec![cloud("开发者", &[])],
            ..Default::default()
        }]);
        engine.set_input("kaifazhes");
        let items = local(&mut engine);
        engine.request_resolve(None, &items);
        let resolved = engine
            .poll_prediction()
            .unwrap()
            .resolved
            .expect("pick decides");
        assert_eq!(resolved.candidate.text, "开发者", "{pinyin:?}");
        assert!(resolved.pinyin_changed);
        assert_eq!(resolved.candidate.syllables.concat(), "kaifazhe");
    }
}

#[test]
fn model_pinyin_is_the_fallback_when_the_picked_reading_cannot_be_used() {
    // 模型把本地一个读音差得太远的候选（开饭）选成了 pick，但它给的拼音是对的：不能因为 pick 站不住就整条丢掉
    let (mut engine, submitted) = resolve_engine(vec![reply(1, "kaifazhe", Some("开饭"))]);
    engine.set_input("kaifazhes");
    let items = local(&mut engine);
    engine.request_resolve(None, &items);
    assert!(
        submitted.borrow()[0]
            .resolve_candidates
            .iter()
            .any(|candidate| candidate.text == "开饭"),
        "the far-off candidate is part of the request"
    );
    let resolved = engine
        .poll_prediction()
        .unwrap()
        .resolved
        .expect("the model's own pinyin is still valid");
    assert_eq!(resolved.candidate.text, "开发者");
    assert!(resolved.pinyin_changed);
}
