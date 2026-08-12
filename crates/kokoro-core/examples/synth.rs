use kokoro_core::dsp::DspFlags;
use kokoro_core::{
    CancelToken, Chunker, Engine, EngineConfig, Pcm, SAMPLE_RATE, ThreadConfig, VoiceSpec,
};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let engine = Engine::new(EngineConfig {
        model_path: root.join("temp/models/kokoro-v1.0.onnx"),
        voices_path: root.join("temp/models/voices-v1.0.bin"),
        threads: ThreadConfig::default(),
    })?;
    println!("voices: {}", engine.voices().len());
    let phonemes = "həlˈoʊ, ðˈɪs ɪz kˈoʊkəɹoʊ ɹˈʌnɪŋ ˈɑːn ðə dɪvˈaɪs. \
        ɪt tˈɜːnz fˈoʊniːmz ˈɪntʊ twˈɛnti fˈoːɹ kˌɪloʊhˈɜːts ˈɑːdiːoʊ wɪð nˈoʊ nˈɛtwɜːk æt ˈɔːl.";
    let voice = VoiceSpec::parse("af_bella")?;
    let start = Instant::now();
    let pcm = engine.synthesize(phonemes, &voice, 1.0)?;
    let elapsed = start.elapsed().as_secs_f64();
    let duration = pcm.samples.len() as f64 / SAMPLE_RATE as f64;
    let peak = pcm.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    std::fs::write(root.join("temp/out_core.wav"), pcm.to_wav())?;
    println!("chunks: {}", Chunker::default().split(phonemes).len());
    println!(
        "audio: {duration:.2}s  peak: {peak:.3}  synth: {elapsed:.2}s  rtf: {:.3}",
        elapsed / duration
    );

    let tagged = format!("[sad] {phonemes}");
    let flags = DspFlags {
        emotion_tags: true,
        smart_punct: true,
    };
    let mut samples = Vec::new();
    engine.synthesize_pipeline(
        &tagged,
        &voice,
        1.0,
        flags,
        &Chunker::default(),
        &CancelToken::default(),
        &|s| Ok(s.to_string()),
        &mut |chunk| {
            samples.extend(chunk.samples);
            Ok(())
        },
    )?;
    let dsp_pcm = Pcm {
        samples,
        sample_rate: SAMPLE_RATE,
    };
    let dsp_duration = dsp_pcm.samples.len() as f64 / SAMPLE_RATE as f64;
    std::fs::write(root.join("temp/out_core_dsp.wav"), dsp_pcm.to_wav())?;
    println!("dsp tagged audio: {dsp_duration:.2}s  control: {duration:.2}s");
    Ok(())
}
