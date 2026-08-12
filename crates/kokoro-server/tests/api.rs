use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kokoro_core::{CancelToken, Chunker, Error, Pcm, Result, SAMPLE_RATE, VoiceId, VoiceSpec};
use kokoro_server::{Server, ServerConfig, SynthEngine, router};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

struct Stub {
    fail: bool,
    seen: Mutex<Vec<(VoiceSpec, f32)>>,
}

impl Stub {
    fn new(fail: bool) -> Arc<Self> {
        Arc::new(Self {
            fail,
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl SynthEngine for Stub {
    fn voices(&self) -> Vec<VoiceId> {
        ["af_bella", "af_heart", "af_sky", "am_adam"]
            .map(|v| VoiceId(v.into()))
            .to_vec()
    }

    fn synthesize_streaming(
        &self,
        phonemes: &str,
        voice: &VoiceSpec,
        speed: f32,
        _chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> Result<()>,
    ) -> Result<()> {
        if self.fail {
            return Err(Error::Model("boom".into()));
        }
        self.seen.lock().expect("lock").push((voice.clone(), speed));
        for word in phonemes.split_whitespace() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            on_chunk(Pcm {
                samples: vec![0.5; word.len()],
                sample_rate: SAMPLE_RATE,
            })?;
        }
        Ok(())
    }
}

async fn post_speech(stub: Arc<Stub>, body: Value) -> (StatusCode, axum::http::HeaderMap, Bytes) {
    let response = router(stub)
        .oneshot(
            Request::post("/v1/audio/speech")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body").to_bytes();
    (parts.status, parts.headers, bytes)
}

async fn get_json(path: &str) -> (StatusCode, Value) {
    let response = router(Stub::new(false))
        .oneshot(Request::get(path).body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body").to_bytes();
    (parts.status, serde_json::from_slice(&bytes).expect("json"))
}

fn s16(n: usize) -> Vec<u8> {
    std::iter::repeat_n(16383i16.to_le_bytes(), n)
        .flatten()
        .collect()
}

#[tokio::test]
async fn pcm_stream_frames_chunks() {
    let (status, headers, body) = post_speech(
        Stub::new(false),
        json!({"input": "ab cde", "voice": "af_bella", "response_format": "pcm"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "audio/pcm");
    assert_eq!(headers["x-accel-buffering"], "no");
    assert_eq!(
        headers["content-disposition"],
        "attachment; filename=speech.pcm"
    );
    assert_eq!(body.as_ref(), s16(5));
}

#[tokio::test]
async fn wav_stream_has_unsized_header() {
    let (status, headers, body) = post_speech(
        Stub::new(false),
        json!({"input": "ab cde", "voice": "af_bella", "response_format": "wav"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "audio/wav");
    assert_eq!(body.len(), 44 + 10);
    assert_eq!(&body[..4], b"RIFF");
    assert_eq!(&body[4..8], u32::MAX.to_le_bytes());
    assert_eq!(&body[24..28], SAMPLE_RATE.to_le_bytes());
    assert_eq!(&body[40..44], u32::MAX.to_le_bytes());
    assert_eq!(&body[44..], s16(5));
}

#[tokio::test]
async fn wav_full_has_exact_sizes() {
    let (status, _, body) = post_speech(
        Stub::new(false),
        json!({"input": "ab cde", "voice": "af_bella", "response_format": "wav", "stream": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[4..8], 46u32.to_le_bytes());
    assert_eq!(&body[40..44], 10u32.to_le_bytes());
    assert_eq!(&body[44..], s16(5));
}

#[tokio::test]
async fn defaults_apply() {
    let (status, headers, _) = post_speech(Stub::new(false), json!({"input": "hi"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "audio/wav");
}

#[tokio::test]
async fn mixed_voice_reaches_engine() {
    let stub = Stub::new(false);
    let (status, _, _) = post_speech(
        stub.clone(),
        json!({"input": "hi", "voice": "af_bella(2)+af_sky", "response_format": "pcm"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let seen = stub.seen.lock().expect("lock");
    match &seen[0].0 {
        VoiceSpec::Mix(entries) => assert_eq!(entries.len(), 2),
        other => panic!("expected mix, got {other:?}"),
    }
}

#[tokio::test]
async fn speed_clamped() {
    let stub = Stub::new(false);
    for sent in [9.0, 0.1] {
        let (status, _, _) = post_speech(
            stub.clone(),
            json!({"input": "hi", "voice": "af_bella", "response_format": "pcm", "speed": sent}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let seen = stub.seen.lock().expect("lock");
    assert_eq!(seen[0].1, 4.0);
    assert_eq!(seen[1].1, 0.25);
}

async fn expect_error(body: Value, status: StatusCode, error: &str) {
    let (got, _, bytes) = post_speech(Stub::new(false), body).await;
    assert_eq!(got, status);
    let parsed: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(parsed["error"], error);
    assert!(parsed["message"].is_string());
    assert!(parsed["type"].is_string());
}

#[tokio::test]
async fn invalid_model_rejected() {
    expect_error(
        json!({"input": "hi", "model": "gpt-5"}),
        StatusCode::BAD_REQUEST,
        "invalid_model",
    )
    .await;
}

#[tokio::test]
async fn empty_input_rejected() {
    expect_error(
        json!({"input": "  "}),
        StatusCode::BAD_REQUEST,
        "validation_error",
    )
    .await;
}

#[tokio::test]
async fn unknown_voice_rejected() {
    expect_error(
        json!({"input": "hi", "voice": "zz_nope"}),
        StatusCode::BAD_REQUEST,
        "validation_error",
    )
    .await;
}

#[tokio::test]
async fn bad_mix_rejected() {
    expect_error(
        json!({"input": "hi", "voice": "af_bella++af_sky"}),
        StatusCode::BAD_REQUEST,
        "validation_error",
    )
    .await;
}

#[tokio::test]
async fn unsupported_format_rejected() {
    expect_error(
        json!({"input": "hi", "response_format": "opus"}),
        StatusCode::BAD_REQUEST,
        "validation_error",
    )
    .await;
}

#[tokio::test]
async fn mp3_not_implemented() {
    expect_error(
        json!({"input": "hi", "response_format": "mp3"}),
        StatusCode::NOT_IMPLEMENTED,
        "not_implemented",
    )
    .await;
}

#[tokio::test]
async fn engine_failure_maps_to_processing_error() {
    let (status, _, bytes) = post_speech(
        Stub::new(true),
        json!({"input": "hi", "voice": "af_bella", "response_format": "pcm", "stream": false}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let parsed: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(parsed["error"], "processing_error");
    assert_eq!(parsed["type"], "server_error");
}

#[tokio::test]
async fn voices_listed() {
    let (status, body) = get_json("/v1/audio/voices").await;
    assert_eq!(status, StatusCode::OK);
    let voices = body["voices"].as_array().expect("array");
    assert_eq!(voices.len(), 4);
    assert_eq!(voices[0], json!({"id": "af_bella", "name": "af_bella"}));
}

#[tokio::test]
async fn models_listed() {
    let (status, body) = get_json("/v1/models").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["object"], "list");
    let ids: Vec<&str> = body["data"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(ids, ["tts-1", "tts-1-hd", "kokoro", "gpt-4o-mini-tts"]);
}

#[tokio::test]
async fn health_ok() {
    let (status, body) = get_json("/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "healthy");
}

#[tokio::test]
async fn lifecycle_start_serve_stop() {
    let server = Server::start(
        Stub::new(false),
        ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    )
    .await
    .expect("start");
    let addr = server.addr();
    assert_ne!(addr.port(), 0);

    let mut conn = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let payload = json!({"input": "ab cde", "voice": "af_bella", "response_format": "pcm"});
    let body = payload.to_string();
    let request = format!(
        "POST /v1/audio/speech HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    conn.write_all(request.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    conn.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw);
    assert!(text.starts_with("HTTP/1.1 200"));
    assert!(text.to_lowercase().contains("transfer-encoding: chunked"));
    assert!(raw.ends_with(b"0\r\n\r\n"));

    server.stop().await.expect("stop");
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}
