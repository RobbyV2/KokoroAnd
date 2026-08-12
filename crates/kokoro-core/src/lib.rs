pub mod dsp;
pub mod vocab;
mod voices;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};

pub const SAMPLE_RATE: u32 = 24_000;
pub const MAX_TOKENS: usize = 510;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("model: {0}")]
    Model(String),
    #[error("voice: {0}")]
    Voice(String),
    #[error("input: {0}")]
    Input(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadConfig {
    pub inter: usize,
    pub intra: Option<usize>,
}

impl Default for ThreadConfig {
    fn default() -> Self {
        Self {
            inter: 1,
            intra: None,
        }
    }
}

pub fn power_cores() -> usize {
    let freqs: Vec<u64> = std::fs::read_dir("/sys/devices/system/cpu")
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .filter_map(|e| {
                    std::fs::read_to_string(e.path().join("cpufreq/cpuinfo_max_freq"))
                        .ok()?
                        .trim()
                        .parse()
                        .ok()
                })
                .collect()
        })
        .unwrap_or_default();
    match freqs.iter().max() {
        Some(&max) => freqs
            .iter()
            .filter(|&&f| f * 10 >= max * 7)
            .count()
            .min(freqs.len().saturating_sub(2))
            .max(2),
        None => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .saturating_sub(2)
            .clamp(2, 8),
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub model_path: PathBuf,
    pub voices_path: PathBuf,
    pub threads: ThreadConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct VoiceId(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceSpec {
    Single(VoiceId),
    Mix(Vec<(VoiceId, f32)>),
}

impl VoiceSpec {
    pub fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
        let parse_part = |part: &str| -> Result<(VoiceId, f32)> {
            match part.strip_suffix(')').and_then(|p| p.split_once('(')) {
                Some((name, w)) => {
                    let weight: f32 = w
                        .trim()
                        .parse()
                        .map_err(|_| Error::Voice(format!("bad weight in {part}")))?;
                    match weight > 0.0 && !name.trim().is_empty() {
                        true => Ok((VoiceId(name.trim().to_string()), weight)),
                        false => Err(Error::Voice(format!("bad component {part}"))),
                    }
                }
                None => match part.is_empty() || part.contains('(') {
                    true => Err(Error::Voice(format!("bad component {part}"))),
                    false => Ok((VoiceId(part.to_string()), 1.0)),
                },
            }
        };
        match parts.as_slice() {
            [] => Err(Error::Voice("empty spec".into())),
            [single] if !single.contains('(') => match single.is_empty() {
                true => Err(Error::Voice("empty spec".into())),
                false => Ok(Self::Single(VoiceId(single.to_string()))),
            },
            many => Ok(Self::Mix(
                many.iter()
                    .map(|p| parse_part(p))
                    .collect::<Result<Vec<_>>>()?,
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Pcm {
    pub fn to_i16(&self) -> Vec<i16> {
        self.samples
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect()
    }

    pub fn to_s16le(&self) -> Vec<u8> {
        self.to_i16().iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    pub fn to_wav(&self) -> Vec<u8> {
        let data = self.to_s16le();
        let mut wav = wav_header(self.sample_rate, Some(data.len() as u32)).to_vec();
        wav.extend(data);
        wav
    }
}

pub fn wav_header(sample_rate: u32, data_len: Option<u32>) -> [u8; 44] {
    let (riff, data) = match data_len {
        Some(n) => (36 + n, n),
        None => (u32::MAX, u32::MAX),
    };
    let mut h = [0u8; 44];
    h[..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&1u16.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct Chunker {
    pub lead_tokens: usize,
    pub target_tokens: usize,
    pub max_tokens: usize,
}

impl Default for Chunker {
    fn default() -> Self {
        Self {
            lead_tokens: 40,
            target_tokens: 250,
            max_tokens: 450,
        }
    }
}

impl Chunker {
    pub fn split(&self, phonemes: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut cur = String::new();
        let mut cur_tokens = 0;
        let flush = |cur: &mut String, cur_tokens: &mut usize, chunks: &mut Vec<String>| {
            let trimmed = cur.trim();
            match trimmed.is_empty() {
                true => {}
                false => chunks.push(trimmed.to_string()),
            }
            cur.clear();
            *cur_tokens = 0;
        };
        let target = |chunks: &Vec<String>| match chunks.is_empty() {
            true => self.lead_tokens,
            false => self.target_tokens,
        };
        for sentence in split_sentences(phonemes) {
            for piece in self.hard_split(&sentence) {
                let n = vocab::token_count(&piece);
                if cur_tokens > 0 && cur_tokens + n > target(&chunks) {
                    flush(&mut cur, &mut cur_tokens, &mut chunks);
                }
                cur.push_str(&piece);
                cur_tokens += n;
                if cur_tokens >= target(&chunks) {
                    flush(&mut cur, &mut cur_tokens, &mut chunks);
                }
            }
        }
        flush(&mut cur, &mut cur_tokens, &mut chunks);
        chunks
    }

    fn hard_split(&self, sentence: &str) -> Vec<String> {
        match vocab::token_count(sentence) <= self.max_tokens {
            true => vec![sentence.to_string()],
            false => {
                let mut pieces = Vec::new();
                let mut cur = String::new();
                let mut n = 0;
                for word in sentence.split_inclusive(' ') {
                    let wn = vocab::token_count(word);
                    if n > 0 && n + wn > self.max_tokens {
                        pieces.push(std::mem::take(&mut cur));
                        n = 0;
                    }
                    cur.push_str(word);
                    n += wn;
                }
                match cur.is_empty() {
                    true => {}
                    false => pieces.push(cur),
                }
                pieces
            }
        }
    }
}

fn split_sentences(phonemes: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = phonemes.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let boundary = matches!(c, '.' | '!' | '?' | ';' | '…')
            && chars.peek().map(|&n| n == ' ' || n == '\n').unwrap_or(true);
        if boundary {
            out.push(std::mem::take(&mut cur));
        }
    }
    match cur.trim().is_empty() {
        true => {}
        false => out.push(cur),
    }
    out
}

fn style_row(
    voices: &HashMap<String, Vec<f32>>,
    spec: &VoiceSpec,
    token_count: usize,
) -> Result<Vec<f32>> {
    if !(1..voices::STYLE_ROWS).contains(&token_count) {
        return Err(Error::Input(format!(
            "token count {token_count} out of range"
        )));
    }
    let row = |VoiceId(name): &VoiceId| -> Result<&[f32]> {
        let v = voices
            .get(name)
            .ok_or_else(|| Error::Voice(format!("unknown voice {name}")))?;
        Ok(&v[token_count * voices::STYLE_DIM..(token_count + 1) * voices::STYLE_DIM])
    };
    match spec {
        VoiceSpec::Single(id) => Ok(row(id)?.to_vec()),
        VoiceSpec::Mix(parts) => {
            let total: f32 = parts.iter().map(|(_, w)| w).sum();
            if parts.is_empty() || total <= 0.0 {
                return Err(Error::Voice("empty mix".into()));
            }
            let mut out = vec![0.0f32; voices::STYLE_DIM];
            for (id, w) in parts {
                let r = row(id)?;
                for (o, s) in out.iter_mut().zip(r) {
                    *o += s * w / total;
                }
            }
            Ok(out)
        }
    }
}

pub struct Engine {
    session: Mutex<Session>,
    voices: HashMap<String, Vec<f32>>,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self> {
        let EngineConfig {
            model_path,
            voices_path,
            threads: ThreadConfig { inter, intra },
        } = config;
        let build = || -> std::result::Result<Session, String> {
            let err = |e: &dyn std::fmt::Display| e.to_string();
            Session::builder()
                .map_err(|e| err(&e))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| err(&e))?
                .with_inter_threads(inter)
                .map_err(|e| err(&e))?
                .with_intra_threads(intra.unwrap_or_else(power_cores))
                .map_err(|e| err(&e))?
                .with_config_entry("session.intra_op.allow_spinning", "0")
                .map_err(|e| err(&e))?
                .with_config_entry("session.inter_op.allow_spinning", "0")
                .map_err(|e| err(&e))?
                .commit_from_file(&model_path)
                .map_err(|e| err(&e))
        };
        let session = build().map_err(Error::Model)?;
        Ok(Self {
            session: Mutex::new(session),
            voices: voices::load(&voices_path)?,
        })
    }

    pub fn voices(&self) -> Vec<VoiceId> {
        let mut names: Vec<String> = self.voices.keys().cloned().collect();
        names.sort();
        names.into_iter().map(VoiceId).collect()
    }

    fn synth_chunk(&self, phonemes: &str, voice: &VoiceSpec, speed: f32) -> Result<Pcm> {
        let tokens = vocab::tokenize(phonemes);
        let style = style_row(&self.voices, voice, tokens.len())?;
        let mut padded = Vec::with_capacity(tokens.len() + 2);
        padded.push(0i64);
        padded.extend(tokens);
        padded.push(0);
        let n = padded.len();
        let ort_err = |e: ort::Error| Error::Model(e.to_string());
        let tokens_t = Tensor::from_array(([1usize, n], padded)).map_err(ort_err)?;
        let style_t = Tensor::from_array(([1usize, voices::STYLE_DIM], style)).map_err(ort_err)?;
        let speed_t = Tensor::from_array(([1usize], vec![speed])).map_err(ort_err)?;
        let inputs: Vec<(Cow<'static, str>, SessionInputValue<'static>)> = vec![
            (
                Cow::Borrowed("tokens"),
                SessionInputValue::Owned(Value::from(tokens_t)),
            ),
            (
                Cow::Borrowed("style"),
                SessionInputValue::Owned(Value::from(style_t)),
            ),
            (
                Cow::Borrowed("speed"),
                SessionInputValue::Owned(Value::from(speed_t)),
            ),
        ];
        let mut session = self
            .session
            .lock()
            .map_err(|_| Error::Model("session poisoned".into()))?;
        let outputs = session.run(SessionInputs::from(inputs)).map_err(ort_err)?;
        let (_, data) = outputs["audio"]
            .try_extract_tensor::<f32>()
            .or_else(|_| outputs["waveforms"].try_extract_tensor::<f32>())
            .map_err(ort_err)?;
        Ok(Pcm {
            samples: data.to_vec(),
            sample_rate: SAMPLE_RATE,
        })
    }

    pub fn synthesize(&self, phonemes: &str, voice: &VoiceSpec, speed: f32) -> Result<Pcm> {
        let mut samples = Vec::new();
        self.synthesize_streaming(
            phonemes,
            voice,
            speed,
            &Chunker::default(),
            &CancelToken::default(),
            &mut |chunk| {
                samples.extend(chunk.samples);
                Ok(())
            },
        )?;
        Ok(Pcm {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }

    pub fn synthesize_streaming(
        &self,
        phonemes: &str,
        voice: &VoiceSpec,
        speed: f32,
        chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> Result<()>,
    ) -> Result<()> {
        let chunks = chunker.split(phonemes);
        if chunks.is_empty() {
            return Err(Error::Input("no synthesizable tokens".into()));
        }
        for chunk in chunks {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            on_chunk(self.synth_chunk(&chunk, voice, speed)?)?;
        }
        Ok(())
    }

    pub fn warm_up(&self) -> Result<()> {
        let name = self
            .voices
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| Error::Voice("no voices".into()))?;
        self.synth_chunk(
            "həlˈoʊ, ðˈɪs ɪz kˈoʊkəɹoʊ ɹˈʌnɪŋ ˈɑːn ðə dɪvˈaɪs.",
            &VoiceSpec::Single(VoiceId(name)),
            1.0,
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunker_packs_toward_target() {
        let sentence = "ab cd ef. ";
        let text = sentence.repeat(60);
        let c = Chunker::default();
        let chunks = c.split(&text);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(vocab::token_count(chunk) <= c.max_tokens);
        }
        let rejoined: usize = chunks.iter().map(|s| vocab::token_count(s)).sum();
        assert!(rejoined >= vocab::token_count(&text) - chunks.len() * 2);
    }

    #[test]
    fn chunker_leading_chunk_small() {
        let sentence = "ab cd ef. ";
        let c = Chunker::default();
        let chunks = c.split(&sentence.repeat(60));
        assert!(chunks.len() >= 2);
        let first = vocab::token_count(&chunks[0]);
        assert!(first <= c.lead_tokens);
        assert!(first > c.lead_tokens - vocab::token_count(sentence));
        assert!(vocab::token_count(&chunks[1]) > c.lead_tokens);
        assert!(chunks[0].ends_with('.'));
    }

    #[test]
    fn chunker_hard_splits_long_sentence() {
        let text = "ab ".repeat(400);
        let c = Chunker::default();
        let chunks = c.split(&text);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(vocab::token_count(chunk) <= c.max_tokens);
        }
    }

    #[test]
    fn chunker_short_input_single_chunk() {
        let chunks = Chunker::default().split("həlˈoʊ wˈɜːld.");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn voice_spec_parse() {
        assert_eq!(
            VoiceSpec::parse("af_bella").unwrap(),
            VoiceSpec::Single(VoiceId("af_bella".into()))
        );
        assert_eq!(
            VoiceSpec::parse("af_sarah(0.4)+af_nicole(0.6)").unwrap(),
            VoiceSpec::Mix(vec![
                (VoiceId("af_sarah".into()), 0.4),
                (VoiceId("af_nicole".into()), 0.6),
            ])
        );
        assert_eq!(
            VoiceSpec::parse("a+b").unwrap(),
            VoiceSpec::Mix(vec![(VoiceId("a".into()), 1.0), (VoiceId("b".into()), 1.0)])
        );
        assert!(VoiceSpec::parse("").is_err());
        assert!(VoiceSpec::parse("a(x)").is_err());
        assert!(VoiceSpec::parse("a(0)+b").is_err());
    }

    #[test]
    fn style_mixing_blends_weighted() {
        let mut voices = HashMap::new();
        voices.insert(
            "a".to_string(),
            vec![1.0f32; voices::STYLE_ROWS * voices::STYLE_DIM],
        );
        voices.insert(
            "b".to_string(),
            vec![3.0f32; voices::STYLE_ROWS * voices::STYLE_DIM],
        );
        let spec = VoiceSpec::Mix(vec![(VoiceId("a".into()), 1.0), (VoiceId("b".into()), 3.0)]);
        let row = style_row(&voices, &spec, 10).unwrap();
        assert_eq!(row.len(), voices::STYLE_DIM);
        for v in row {
            assert!((v - 2.5).abs() < 1e-6);
        }
        let single = style_row(&voices, &VoiceSpec::Single(VoiceId("a".into())), 10).unwrap();
        assert!(single.iter().all(|&v| v == 1.0));
        assert!(style_row(&voices, &spec, 0).is_err());
        assert!(style_row(&voices, &spec, voices::STYLE_ROWS).is_err());
        assert!(style_row(&voices, &VoiceSpec::Single(VoiceId("z".into())), 10).is_err());
    }

    #[test]
    fn dsp_segment_and_silence() {
        use dsp::{DspFlags, Emotion, Punct, Segment, punct_silence_ms, segment};
        let segs = segment(
            "hi [whispers] there... ok, done.",
            DspFlags {
                emotion_tags: true,
                smart_punct: true,
            },
        );
        assert_eq!(
            segs,
            vec![
                Segment::Text("hi ".into()),
                Segment::Tag(Emotion::Whisper),
                Segment::Text(" there".into()),
                Segment::Punct(Punct::Ellipsis),
                Segment::Text(" ok".into()),
                Segment::Punct(Punct::Comma),
                Segment::Text(" done".into()),
                Segment::Punct(Punct::Period),
            ]
        );
        let mut seed = 42;
        let ms = punct_silence_ms(Punct::Period, 0.5, 1.0, &mut seed);
        assert!((270..=330).contains(&ms));
    }
}
