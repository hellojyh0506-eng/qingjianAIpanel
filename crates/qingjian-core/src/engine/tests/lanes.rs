//! 请求通道：不同通道的结果互不过期，同通道只认最新的，结果自带种类。

use super::*;

fn reply(sequence: u64, sentence: &str) -> Prediction {
    Prediction {
        sequence,
        sentence: Some(sentence.into()),
        ..Default::default()
    }
}

fn engine_with(replies: Vec<Prediction>) -> Engine {
    let predictor = EchoPredictor {
        submitted: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        replies,
        sentence: false,
    };
    engine().with_predictor(Box::new(predictor))
}

#[test]
fn kinds_map_to_their_lanes() {
    for kind in [
        PredictionKind::Compose,
        PredictionKind::Intent,
        PredictionKind::Rerank,
        PredictionKind::Question,
    ] {
        assert_eq!(kind.lane(), PredictionLane::Typing, "{kind:?}");
    }
    assert_eq!(PredictionKind::Translate.lane(), PredictionLane::Learning);
    assert_eq!(PredictionKind::Compare.lane(), PredictionLane::Deep);
    assert_eq!(PredictionKind::DeepLearn.lane(), PredictionLane::Deep);
}

#[test]
fn a_translation_does_not_expire_the_typing_result() {
    // 回复从后往前取：先到打字的（序号 1），再到翻译的（序号 2）
    let mut engine = engine_with(vec![reply(2, "译文"), reply(1, "开发")]);
    engine.set_input("kaifa");
    assert_eq!(engine.request_prediction(None, &[]), Some(1));
    assert_eq!(engine.request_translation("开发"), Some(2));

    let typing = engine.poll_prediction().expect("typing result survives");
    assert_eq!(typing.sequence, 1);
    assert_eq!(typing.kind, PredictionKind::Compose);
    let translation = engine.poll_prediction().expect("translation arrives");
    assert_eq!(translation.sequence, 2);
    assert_eq!(translation.kind, PredictionKind::Translate);
    assert_eq!(translation.lane(), PredictionLane::Learning);
}

#[test]
fn a_typing_request_does_not_expire_the_translation() {
    let mut engine = engine_with(vec![reply(1, "译文")]);
    engine.set_input("kaifa");
    assert_eq!(engine.request_translation("开发"), Some(1));
    assert_eq!(engine.request_prediction(None, &[]), Some(2));
    let translation = engine
        .poll_prediction()
        .expect("translation is on its own lane");
    assert_eq!(translation.kind, PredictionKind::Translate);
}

#[test]
fn a_newer_request_on_the_same_lane_expires_the_older_one() {
    let mut engine = engine_with(vec![reply(1, "旧")]);
    engine.set_input("kaifa");
    engine.request_prediction(None, &[]);
    engine.request_prediction(None, &[]);
    assert_eq!(engine.poll_prediction(), None);

    let mut engine = engine_with(vec![reply(1, "旧译文")]);
    engine.request_translation("开发");
    engine.request_translation("开花");
    assert_eq!(engine.poll_prediction(), None);
}

#[test]
fn deep_results_survive_typing_and_cancel() {
    let mut engine = engine_with(vec![reply(1, "深度")]);
    engine.set_input("kaifa");
    assert_eq!(engine.request_deep_learn("开发", "develop"), Some(1));
    engine.request_prediction(None, &[]);
    engine.cancel_prediction();
    let deep = engine
        .poll_prediction()
        .expect("deep result is not interrupted by typing");
    assert_eq!(deep.kind, PredictionKind::DeepLearn);
}

#[test]
fn cancel_expires_typing_and_learning_results() {
    let mut engine = engine_with(vec![reply(2, "译文"), reply(1, "打字")]);
    engine.set_input("kaifa");
    engine.request_prediction(None, &[]);
    engine.request_translation("开发");
    engine.cancel_prediction();
    assert_eq!(engine.poll_prediction(), None);
}

#[test]
fn entering_private_input_expires_every_in_flight_result() {
    let mut engine = engine_with(vec![reply(2, "译文"), reply(1, "打字")]);
    engine.set_input("kaifa");
    engine.request_prediction(None, &[]);
    engine.request_translation("开发");
    engine.set_private(true);
    assert_eq!(engine.poll_prediction(), None);
}
