//! 三通道并行：用本机假服务验证「慢的深度请求不挡打字、翻译与重排互不等待」。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use qingjian_core::{Prediction, PredictionKind, PredictionRequest, Predictor};
use qingjian_predict::{CloudPredictor, PredictConfig};

/// 请求体里带 `slow-panel` 的（面板模型）延迟 700ms，其余延迟 300ms。
fn serve(mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (header_end, length) = loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (pos + 4, length);
        }
    };
    while buf.len() < header_end + length {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).into_owned();
    let delay = if body.contains("slow-panel") {
        700
    } else {
        300
    };
    thread::sleep(Duration::from_millis(delay));
    let payload = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"{\"words\":[\"好\"]}"},"finish_reason":"stop"}]}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || serve(stream));
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

fn predictor() -> CloudPredictor {
    let config = PredictConfig {
        base_url: start_server(),
        model: "typing".to_owned(),
        panel_model: "slow-panel".to_owned(),
        api_key: Some("k".to_owned()),
        timeout_ms: 5000,
        debounce_ms: 20,
        ..PredictConfig::default()
    };
    CloudPredictor::new(&config).unwrap()
}

fn request(sequence: u64, kind: PredictionKind) -> PredictionRequest {
    PredictionRequest {
        sequence,
        kind,
        before: String::new(),
        after: String::new(),
        pinyin: "nihao".to_owned(),
        letters: "nihao".to_owned(),
        syllables: 2,
        candidates: vec!["你好".to_owned()],
        candidate_readings: Vec::new(),
        guess: String::new(),
        current_expression: String::new(),
        max_items: 3,
        want_sentence: false,
        text: "你好".to_owned(),
        target_language: "en".to_owned(),
        user_attempt: String::new(),
        reference: String::new(),
    }
}

/// 在 `limit` 内收齐 `want` 条结果，返回（序号，耗时）列表。
fn collect(predictor: &mut CloudPredictor, want: usize, limit: Duration) -> Vec<(u64, Duration)> {
    let start = Instant::now();
    let mut got: Vec<Prediction> = Vec::new();
    let mut seen = Vec::new();
    while got.len() < want && start.elapsed() < limit {
        match predictor.poll() {
            Some(p) => {
                seen.push((p.sequence, start.elapsed()));
                got.push(p);
            }
            None => thread::sleep(Duration::from_millis(5)),
        }
    }
    seen
}

#[test]
fn translate_and_rerank_run_in_parallel() {
    let mut predictor = predictor();
    predictor.submit(request(1, PredictionKind::Translate));
    predictor.submit(request(2, PredictionKind::Rerank));
    let seen = collect(&mut predictor, 2, Duration::from_millis(3000));
    assert_eq!(seen.len(), 2, "两条都应返回：{seen:?}");
    let last = seen.iter().map(|(_, t)| *t).max().unwrap();
    assert!(
        last < Duration::from_millis(550),
        "串行会 ≥600ms，并行应接近 300ms：{seen:?}"
    );
}

#[test]
fn slow_deep_request_does_not_block_or_get_cancelled_by_typing() {
    let mut predictor = predictor();
    predictor.submit(request(1, PredictionKind::DeepLearn));
    thread::sleep(Duration::from_millis(50));
    predictor.submit(request(2, PredictionKind::Rerank));
    thread::sleep(Duration::from_millis(60));
    predictor.submit(request(3, PredictionKind::Rerank));
    let seen = collect(&mut predictor, 2, Duration::from_millis(3000));
    let typing = seen
        .iter()
        .find(|(s, _)| *s == 3)
        .expect("最新的打字请求要返回");
    let deep = seen
        .iter()
        .find(|(s, _)| *s == 1)
        .expect("深度请求不能被打字打断");
    assert!(typing.1 < deep.1, "打字应先于慢的深度请求返回：{seen:?}");
    assert!(
        typing.1 < Duration::from_millis(600),
        "打字不应被深度请求拖慢：{seen:?}"
    );
}

#[test]
fn newer_request_in_the_same_lane_replaces_the_older_one() {
    let mut predictor = predictor();
    predictor.submit(request(1, PredictionKind::Rerank));
    thread::sleep(Duration::from_millis(80));
    predictor.submit(request(2, PredictionKind::Intent));
    let seen = collect(&mut predictor, 2, Duration::from_millis(1200));
    assert!(seen.iter().any(|(s, _)| *s == 2), "新请求要返回：{seen:?}");
    assert!(
        !seen.iter().any(|(s, _)| *s == 1),
        "同通道旧请求应被打断：{seen:?}"
    );
}
