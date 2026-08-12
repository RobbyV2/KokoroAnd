use app_lib::Tts;
use app_lib::android::{TTS, set_always_pin};
use kokoro_core::{
    CancelToken, Chunker, Engine, EngineConfig, SAMPLE_RATE, ThreadConfig, VoiceSpec,
};
use kokoro_g2p::{G2p, Lang};
use kokoro_server::{Server, ServerConfig};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn assert_wav(bytes: &[u8]) -> f64 {
    assert!(bytes.len() > 44, "wav too small: {} bytes", bytes.len());
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    (bytes.len() - 44) as f64 / 2.0 / SAMPLE_RATE as f64
}

fn curl(args: &[&str]) -> Vec<u8> {
    let out = Command::new("curl")
        .args(["-s", "--max-time", "300"])
        .args(args)
        .output()
        .expect("curl spawn");
    assert!(out.status.success(), "curl failed: {:?}", out.status);
    out.stdout
}

const PARAGRAPH: &str = "The quick brown fox jumps over the lazy dog near the riverbank. \
    Yesterday, forty-two researchers published a detailed study on speech synthesis quality. \
    Each sentence in this paragraph exists to stretch the chunker across several boundaries. \
    Modern text to speech systems convert graphemes into phonemes before generating audio. \
    The model then predicts a waveform at twenty-four kilohertz from those phoneme tokens. \
    Streaming delivery means the first sentence plays while later ones are still rendering.";

#[test]
fn desktop_end_to_end() {
    let models = root().join("temp/models");
    if !models.join("kokoro-v1.0.onnx").exists() {
        eprintln!("temp/models missing, skipping e2e");
        return;
    }
    let tts = Arc::new(Tts {
        engine: Engine::new(EngineConfig {
            model_path: models.join("kokoro-v1.0.onnx"),
            voices_path: models.join("voices-v1.0.bin"),
            threads: ThreadConfig::default(),
        })
        .expect("engine"),
        g2p: G2p::new().expect("g2p"),
        dict: Arc::default(),
        flags: Arc::default(),
    });
    assert_eq!(tts.engine.voices().len(), 54);

    let phonemes = tts
        .phonemize("Hello, this is a TTS test.", Lang::EnUs)
        .expect("g2p run");
    assert!(
        phonemes.contains('ə') || phonemes.contains('ˈ'),
        "{phonemes}"
    );
    let spec = VoiceSpec::parse("af_bella").expect("voice");
    let start = Instant::now();
    let pcm = tts.engine.synthesize(&phonemes, &spec, 1.0).expect("synth");
    let demo_elapsed = start.elapsed().as_secs_f64();
    let demo_secs = assert_wav(&pcm.to_wav());
    assert!(demo_secs > 0.5, "demo audio {demo_secs:.2}s");
    println!(
        "demo: {demo_secs:.2}s audio in {demo_elapsed:.2}s, rtf {:.3}",
        demo_elapsed / demo_secs
    );

    let rt = tokio::runtime::Runtime::new().expect("rt");
    let server = rt
        .block_on(Server::start(
            tts.clone(),
            ServerConfig {
                host: "127.0.0.1".into(),
                port: 8471,
            },
        ))
        .expect("server start");
    let url = format!("http://{}/v1/audio/speech", server.addr());

    let body = serde_json::json!({
        "model": "kokoro",
        "input": "Hello, this is a TTS test.",
        "voice": "af_bella",
        "stream": false,
    })
    .to_string();
    let buffered = curl(&[
        "-X",
        "POST",
        &url,
        "-H",
        "content-type: application/json",
        "-d",
        &body,
    ]);
    let buffered_secs = assert_wav(&buffered);
    let declared = u32::from_le_bytes([buffered[40], buffered[41], buffered[42], buffered[43]]);
    assert_eq!(declared as usize, buffered.len() - 44);
    println!(
        "buffered: {} bytes, {buffered_secs:.2}s audio",
        buffered.len()
    );

    let voices = curl(&[&format!("http://{}/v1/audio/voices", server.addr())]);
    assert!(String::from_utf8_lossy(&voices).contains("af_bella"));

    let headers_path = root().join("temp/e2e_headers.txt");
    let body = serde_json::json!({
        "model": "kokoro",
        "input": PARAGRAPH,
        "voice": "af_bella",
    })
    .to_string();
    let start = Instant::now();
    let streamed = curl(&[
        "-N",
        "-D",
        headers_path.to_str().expect("path"),
        "-X",
        "POST",
        &url,
        "-H",
        "content-type: application/json",
        "-d",
        &body,
    ]);
    let elapsed = start.elapsed().as_secs_f64();
    let headers = std::fs::read_to_string(&headers_path).expect("headers");
    assert!(
        headers
            .to_lowercase()
            .contains("transfer-encoding: chunked"),
        "{headers}"
    );
    let secs = assert_wav(&streamed);
    assert!(secs > 15.0, "paragraph audio only {secs:.2}s");
    println!(
        "paragraph: {secs:.2}s audio in {elapsed:.2}s, rtf {:.3}, {} bytes chunked",
        elapsed / secs,
        streamed.len()
    );

    rt.block_on(server.stop()).expect("server stop");
}

#[test]
fn demo_in_both_engine_modes() {
    let models = root().join("temp/models");
    if !models.join("kokoro-v1.0.onnx").exists() {
        eprintln!("temp/models missing, skipping slot modes");
        return;
    }
    let tts = Arc::new(
        Tts::load(
            &models,
            ThreadConfig::default(),
            Arc::default(),
            Arc::default(),
        )
        .expect("tts load"),
    );
    let spec = VoiceSpec::parse("af_bella").expect("voice");
    let synth = |tts: &Tts| {
        let mut samples = 0usize;
        tts.synthesize_dsp(
            "Hello, this is a TTS test.",
            &spec,
            1.0,
            &Chunker::default(),
            &CancelToken::default(),
            &mut |pcm| {
                samples += pcm.samples.len();
                Ok(())
            },
        )
        .expect("synth");
        assert!(samples > SAMPLE_RATE as usize / 2, "{samples} samples");
    };

    assert!(TTS.peek().is_none());
    let lease = TTS.install_leased(tts.clone());
    synth(&lease);
    drop(lease);
    assert!(TTS.peek().is_none(), "on-demand engine kept resident");

    set_always_pin(true);
    TTS.install(tts.clone());
    let lease = TTS.lease().expect("resident lease");
    synth(&lease);
    drop(lease);
    assert!(TTS.peek().is_some(), "always-on engine released");
    set_always_pin(false);
    assert!(TTS.peek().is_none(), "engine kept after always-on off");
}
