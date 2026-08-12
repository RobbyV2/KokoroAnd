use crate::SynthEngine;
use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use kokoro_core::{CancelToken, Chunker, Pcm, SAMPLE_RATE, VoiceId, VoiceSpec, wav_header};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

pub const MODELS: [&str; 4] = ["tts-1", "tts-1-hd", "kokoro", "gpt-4o-mini-tts"];

const OPENAI_VOICES: [(&str, &str); 9] = [
    ("alloy", "am_adam"),
    ("ash", "af_nicole"),
    ("coral", "bf_emma"),
    ("echo", "af_bella"),
    ("fable", "af_sarah"),
    ("onyx", "bm_george"),
    ("nova", "bf_isabella"),
    ("sage", "am_michael"),
    ("shimmer", "af_sky"),
];

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<dyn SynthEngine>,
}

#[derive(Debug)]
pub enum ApiError {
    InvalidModel(String),
    Validation(String),
    NotImplemented(String),
    Processing(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, kind, message) = match self {
            Self::InvalidModel(m) => (
                StatusCode::BAD_REQUEST,
                "invalid_model",
                "invalid_request_error",
                m,
            ),
            Self::Validation(m) => (
                StatusCode::BAD_REQUEST,
                "validation_error",
                "invalid_request_error",
                m,
            ),
            Self::NotImplemented(m) => (
                StatusCode::NOT_IMPLEMENTED,
                "not_implemented",
                "invalid_request_error",
                m,
            ),
            Self::Processing(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "processing_error",
                "server_error",
                m,
            ),
        };
        (
            status,
            Json(json!({"error": error, "message": message, "type": kind})),
        )
            .into_response()
    }
}

#[derive(Clone, Copy, PartialEq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Wav,
    Pcm,
    Mp3,
}

impl Format {
    fn wire(self) -> Result<Wire, ApiError> {
        match self {
            Self::Wav => Ok(Wire::Wav),
            Self::Pcm => Ok(Wire::Pcm),
            Self::Mp3 => Err(ApiError::NotImplemented(
                "mp3 encoding unavailable; use wav or pcm".into(),
            )),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wire {
    Wav,
    Pcm,
}

impl Wire {
    fn mime(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Pcm => "audio/pcm",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Pcm => "pcm",
        }
    }
}

#[derive(serde::Deserialize)]
pub struct SpeechRequest {
    #[serde(default = "default_model")]
    pub model: String,
    pub input: String,
    #[serde(default = "default_voice")]
    pub voice: String,
    #[serde(default)]
    pub response_format: Format,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_stream")]
    pub stream: bool,
}

fn default_model() -> String {
    "kokoro".into()
}

fn default_voice() -> String {
    "af_heart".into()
}

fn default_speed() -> f32 {
    1.0
}

fn default_stream() -> bool {
    true
}

pub fn parse_voice(spec: &str) -> Result<VoiceSpec, ApiError> {
    let err = |m: &str| ApiError::Validation(format!("invalid voice '{spec}': {m}"));
    let compact: String = spec.chars().filter(|c| !c.is_whitespace()).collect();
    let mut parts: Vec<(f32, String)> = Vec::new();
    let mut cur = String::new();
    let mut op = 1.0f32;
    for c in compact.chars() {
        match c {
            '+' | '-' => {
                if cur.is_empty() {
                    return Err(err("misplaced separator"));
                }
                parts.push((op, std::mem::take(&mut cur)));
                op = match c {
                    '+' => 1.0,
                    _ => -1.0,
                };
            }
            _ => cur.push(c),
        }
    }
    if cur.is_empty() {
        return Err(err("empty voice name"));
    }
    parts.push((op, cur));
    let mut entries: Vec<(VoiceId, f32, f32)> = Vec::new();
    for (op, part) in parts {
        let (name, weight) = match (part.find('('), part.ends_with(')')) {
            (Some(i), true) => {
                let weight: f32 = part[i + 1..part.len() - 1]
                    .parse()
                    .map_err(|_| err("weight must be a number"))?;
                if !weight.is_finite() || weight <= 0.0 {
                    return Err(err("weight must be a finite positive number"));
                }
                (part[..i].to_string(), weight)
            }
            (None, false) => (part, 1.0),
            (_, _) => return Err(err("malformed weight parentheses")),
        };
        if name.is_empty() {
            return Err(err("empty voice name"));
        }
        let name = OPENAI_VOICES
            .iter()
            .find(|(alias, _)| *alias == name)
            .map_or(name, |(_, mapped)| (*mapped).into());
        entries.push((VoiceId(name), op * weight, weight));
    }
    match entries.as_slice() {
        [(id, _, _)] => Ok(VoiceSpec::Single(id.clone())),
        _ => {
            let total: f32 = entries.iter().map(|(_, _, w)| w).sum();
            Ok(VoiceSpec::Mix(
                entries
                    .into_iter()
                    .map(|(id, signed, _)| (id, signed / total))
                    .collect(),
            ))
        }
    }
}

fn pcm_bytes(pcm: &Pcm) -> Bytes {
    Bytes::from(pcm.to_s16le())
}

fn audio_response(wire: Wire, streaming: bool, body: Body) -> Response {
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, wire.mime())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=speech.{}", wire.ext()),
        );
    if streaming {
        builder = builder
            .header(header::CACHE_CONTROL, "no-cache")
            .header("x-accel-buffering", "no");
    }
    match builder.body(body) {
        Ok(response) => response,
        Err(e) => ApiError::Processing(e.to_string()).into_response(),
    }
}

pub async fn speech(
    State(state): State<AppState>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(body) = body.map_err(|e| ApiError::Validation(e.to_string()))?;
    let request: SpeechRequest =
        serde_json::from_value(body).map_err(|e| ApiError::Validation(e.to_string()))?;
    let SpeechRequest {
        model,
        input,
        voice,
        response_format,
        speed,
        stream,
    } = request;
    if !MODELS.contains(&model.as_str()) {
        return Err(ApiError::InvalidModel(format!("model '{model}' not found")));
    }
    if input.trim().is_empty() {
        return Err(ApiError::Validation("input must not be empty".into()));
    }
    let spec = parse_voice(&voice)?;
    let known: HashSet<VoiceId> = state.engine.voices().into_iter().collect();
    let ids: Vec<&VoiceId> = match &spec {
        VoiceSpec::Single(id) => vec![id],
        VoiceSpec::Mix(entries) => entries.iter().map(|(id, _)| id).collect(),
    };
    for id in ids {
        if !known.contains(id) {
            return Err(ApiError::Validation(format!("voice '{}' not found", id.0)));
        }
    }
    let speed = speed.clamp(0.25, 4.0);
    let wire = response_format.wire()?;
    match stream {
        true => Ok(stream_response(state, input, spec, speed, wire)),
        false => full_response(state, input, spec, speed, wire).await,
    }
}

fn stream_response(
    state: AppState,
    input: String,
    spec: VoiceSpec,
    speed: f32,
    wire: Wire,
) -> Response {
    let AppState { engine } = state;
    let (tx, rx) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(8);
    if wire == Wire::Wav {
        let _ = tx.try_send(Ok(Bytes::copy_from_slice(&wav_header(SAMPLE_RATE, None))));
    }
    tokio::task::spawn_blocking(move || {
        let result = engine.synthesize_streaming(
            &input,
            &spec,
            speed,
            &Chunker::default(),
            &CancelToken::default(),
            &mut |pcm| {
                tx.blocking_send(Ok(pcm_bytes(&pcm)))
                    .map_err(|_| kokoro_core::Error::Cancelled)
            },
        );
        match result {
            Ok(()) | Err(kokoro_core::Error::Cancelled) => {}
            Err(e) => {
                let _ = tx.blocking_send(Err(std::io::Error::other(e.to_string())));
            }
        }
    });
    audio_response(wire, true, Body::from_stream(ReceiverStream::new(rx)))
}

async fn full_response(
    state: AppState,
    input: String,
    spec: VoiceSpec,
    speed: f32,
    wire: Wire,
) -> Result<Response, ApiError> {
    let AppState { engine } = state;
    let samples = tokio::task::spawn_blocking(move || {
        let mut buf: Vec<u8> = Vec::new();
        engine.synthesize_streaming(
            &input,
            &spec,
            speed,
            &Chunker::default(),
            &CancelToken::default(),
            &mut |pcm| {
                buf.extend_from_slice(&pcm_bytes(&pcm));
                Ok(())
            },
        )?;
        Ok::<Vec<u8>, kokoro_core::Error>(buf)
    })
    .await
    .map_err(|e| ApiError::Processing(e.to_string()))?
    .map_err(|e| ApiError::Processing(e.to_string()))?;
    let body = match wire {
        Wire::Wav => {
            let mut out = wav_header(SAMPLE_RATE, Some(samples.len() as u32)).to_vec();
            out.extend_from_slice(&samples);
            out
        }
        Wire::Pcm => samples,
    };
    Ok(audio_response(wire, false, Body::from(body)))
}

pub async fn voices(State(state): State<AppState>) -> Json<Value> {
    let AppState { engine } = state;
    let mut ids: Vec<String> = engine.voices().into_iter().map(|VoiceId(v)| v).collect();
    ids.sort();
    Json(json!({
        "voices": ids.iter().map(|v| json!({"id": v, "name": v})).collect::<Vec<_>>()
    }))
}

pub async fn models() -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": MODELS.iter().map(|m| json!({
            "id": m, "object": "model", "created": 1_715_000_000, "owned_by": "kokoro"
        })).collect::<Vec<_>>()
    }))
}

pub async fn health() -> Json<Value> {
    Json(json!({"status": "healthy"}))
}

#[cfg(test)]
mod tests {
    use super::parse_voice;
    use kokoro_core::{VoiceId, VoiceSpec};

    fn id(s: &str) -> VoiceId {
        VoiceId(s.into())
    }

    #[test]
    fn single() {
        assert_eq!(
            parse_voice("af_bella").expect("parse"),
            VoiceSpec::Single(id("af_bella"))
        );
    }

    #[test]
    fn single_weighted_is_single() {
        assert_eq!(
            parse_voice("af_bella(2)").expect("parse"),
            VoiceSpec::Single(id("af_bella"))
        );
    }

    #[test]
    fn mix_weights_normalized() {
        match parse_voice("af_bella(2)+af_sky").expect("parse") {
            VoiceSpec::Mix(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].0, id("af_bella"));
                assert!((entries[0].1 - 2.0 / 3.0).abs() < 1e-6);
                assert_eq!(entries[1].0, id("af_sky"));
                assert!((entries[1].1 - 1.0 / 3.0).abs() < 1e-6);
            }
            other => panic!("expected mix, got {other:?}"),
        }
    }

    #[test]
    fn subtract_negates_weight() {
        match parse_voice("af_bella-af_sky").expect("parse") {
            VoiceSpec::Mix(entries) => {
                assert!((entries[0].1 - 0.5).abs() < 1e-6);
                assert!((entries[1].1 + 0.5).abs() < 1e-6);
            }
            other => panic!("expected mix, got {other:?}"),
        }
    }

    #[test]
    fn spaces_stripped() {
        assert!(matches!(
            parse_voice(" af_bella (2) + af_sky "),
            Ok(VoiceSpec::Mix(_))
        ));
    }

    #[test]
    fn openai_names_mapped() {
        assert_eq!(
            parse_voice("echo").expect("parse"),
            VoiceSpec::Single(id("af_bella"))
        );
        match parse_voice("alloy+shimmer").expect("parse") {
            VoiceSpec::Mix(entries) => {
                assert_eq!(entries[0].0, id("am_adam"));
                assert_eq!(entries[1].0, id("af_sky"));
            }
            other => panic!("expected mix, got {other:?}"),
        }
    }

    #[test]
    fn rejects_malformed() {
        for bad in [
            "",
            "+af_bella",
            "af_bella+",
            "af_bella++af_sky",
            "af_bella(x)",
            "af_bella(0)",
            "af_bella(2",
            "(2)",
        ] {
            assert!(parse_voice(bad).is_err(), "should reject {bad:?}");
        }
    }
}
