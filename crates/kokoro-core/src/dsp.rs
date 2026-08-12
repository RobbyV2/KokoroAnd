use crate::{CancelToken, Chunker, Engine, Error, Pcm, Result, SAMPLE_RATE, VoiceSpec};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DspFlags {
    pub emotion_tags: bool,
    pub smart_punct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmotionProfile {
    pub volume: f32,
    pub speed: f32,
    pub pitch: f32,
    pub attack_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emotion {
    Normal,
    Whisper,
    Angry,
    Sad,
    Sarcastic,
    Giggle,
}

impl Emotion {
    pub fn parse(tag: &str) -> Self {
        match tag.to_ascii_lowercase().as_str() {
            "whisper" | "whispers" => Self::Whisper,
            "angry" => Self::Angry,
            "sad" => Self::Sad,
            "sarcastic" | "sarcastically" => Self::Sarcastic,
            "giggle" | "giggles" => Self::Giggle,
            _ => Self::Normal,
        }
    }

    pub fn profile(&self, base_volume: f32, base_speed: f32, base_pitch: f32) -> EmotionProfile {
        let (v, s, p, a) = match self {
            Self::Normal => (1.0, 1.0, 1.0, 1500),
            Self::Whisper => (0.65, 0.95, 1.05, 2500),
            Self::Angry => (1.15, 1.05, 0.95, 1500),
            Self::Sad => (0.80, 0.92, 0.98, 2500),
            Self::Sarcastic => (1.0, 1.02, 0.95, 1500),
            Self::Giggle => (1.10, 1.05, 1.10, 1000),
        };
        EmotionProfile {
            volume: base_volume * v,
            speed: base_speed * s,
            pitch: base_pitch * p,
            attack_ms: a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Punct {
    Comma,
    Exclaim,
    Question,
    Period,
    Ellipsis,
}

impl Punct {
    pub fn base_ms(&self) -> u32 {
        match self {
            Self::Comma => 150,
            Self::Exclaim => 200,
            Self::Question => 250,
            Self::Period => 300,
            Self::Ellipsis => 450,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Tag(Emotion),
    Punct(Punct),
}

pub fn segment(text: &str, flags: DspFlags) -> Vec<Segment> {
    let DspFlags {
        emotion_tags,
        smart_punct,
    } = flags;
    let mut out = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<Segment>| match buf.trim().is_empty() {
        true => buf.clear(),
        false => out.push(Segment::Text(std::mem::take(buf))),
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' if emotion_tags => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&x| x == ']')
                    .map(|p| i + 1 + p);
                match end {
                    Some(e)
                        if e > i + 1 && chars[i + 1..e].iter().all(|x| x.is_ascii_alphabetic()) =>
                    {
                        flush(&mut buf, &mut out);
                        let tag: String = chars[i + 1..e].iter().collect();
                        out.push(Segment::Tag(Emotion::parse(&tag)));
                        i = e + 1;
                    }
                    _ => {
                        buf.push(c);
                        i += 1;
                    }
                }
            }
            '.' if smart_punct
                && chars.get(i + 1) == Some(&'.')
                && chars.get(i + 2) == Some(&'.') =>
            {
                flush(&mut buf, &mut out);
                out.push(Segment::Punct(Punct::Ellipsis));
                i += 3;
            }
            '.' | ',' | '!' | '?' | '।' if smart_punct => {
                flush(&mut buf, &mut out);
                let p = match c {
                    ',' => Punct::Comma,
                    '!' => Punct::Exclaim,
                    '?' => Punct::Question,
                    _ => Punct::Period,
                };
                out.push(Segment::Punct(p));
                i += 1;
            }
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    flush(&mut buf, &mut out);
    out
}

pub fn attack_envelope(
    samples: &mut [f32],
    from_volume: f32,
    to_volume: f32,
    attack_ms: u32,
    start: usize,
) {
    let ramp = ((SAMPLE_RATE * attack_ms) / 1000) as usize;
    for (i, s) in samples.iter_mut().enumerate() {
        let pos = start + i;
        let g = match ramp > 0 && pos < ramp {
            true => from_volume + (to_volume - from_volume) * pos as f32 / ramp as f32,
            false => to_volume,
        };
        *s = (*s * g).clamp(-1.0, 1.0);
    }
}

pub fn punct_silence_ms(punct: Punct, silence_scale: f32, speed: f32, seed: &mut u32) -> u32 {
    let speed = match speed > 1e-6 {
        true => speed,
        false => 1.0,
    };
    let base = punct.base_ms() as f32 * silence_scale * 2.0 / speed;
    let jitter = base * 0.10;
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    let r = (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
    (base + jitter * r).max(0.0) as u32
}

pub fn silence(ms: u32) -> Vec<f32> {
    vec![0.0; ((SAMPLE_RATE * ms) / 1000) as usize]
}

pub fn pitch_shift(samples: &[f32], factor: f32) -> Vec<f32> {
    if (factor - 1.0).abs() < 1e-3 || samples.is_empty() {
        return samples.to_vec();
    }
    let win = 960usize;
    let hop_out = win / 2;
    let hop_in = (hop_out as f32 / factor) as usize;
    let mut stretched = vec![0.0f32; (samples.len() as f32 * factor) as usize + win];
    let mut norm = vec![0.0f32; stretched.len()];
    let hann: Vec<f32> = (0..win)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / win as f32).cos())
        .collect();
    let mut in_pos = 0usize;
    let mut out_pos = 0usize;
    while in_pos + win <= samples.len() && out_pos + win <= stretched.len() {
        for i in 0..win {
            stretched[out_pos + i] += samples[in_pos + i] * hann[i];
            norm[out_pos + i] += hann[i];
        }
        in_pos += hop_in.max(1);
        out_pos += hop_out;
    }
    for (s, n) in stretched.iter_mut().zip(&norm) {
        if *n > 1e-6 {
            *s /= *n;
        }
    }
    (0..samples.len())
        .map(|i| {
            let pos = i as f32 * factor;
            let j = pos as usize;
            let frac = pos - j as f32;
            let a = stretched.get(j).copied().unwrap_or(0.0);
            let b = stretched.get(j + 1).copied().unwrap_or(0.0);
            a + (b - a) * frac
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Synth {
        text: String,
        profile: EmotionProfile,
        attack_from: f32,
    },
    Silence {
        ms: u32,
    },
}

pub fn seed_from(text: &str) -> u32 {
    text.bytes()
        .fold(2_166_136_261u32, |h, b| {
            (h ^ u32::from(b)).wrapping_mul(16_777_619)
        })
        .max(1)
}

pub fn plan(text: &str, flags: DspFlags, base_speed: f32) -> Vec<Step> {
    let mut seed = seed_from(text);
    let mut profile = Emotion::Normal.profile(1.0, base_speed, 1.0);
    let mut prev_volume = profile.volume;
    let mut steps = Vec::new();
    for seg in segment(text, flags) {
        match seg {
            Segment::Text(text) => {
                steps.push(Step::Synth {
                    text,
                    profile,
                    attack_from: prev_volume,
                });
                prev_volume = profile.volume;
            }
            Segment::Tag(e) => profile = e.profile(1.0, base_speed, 1.0),
            Segment::Punct(p) => steps.push(Step::Silence {
                ms: punct_silence_ms(p, 0.5, profile.speed, &mut seed),
            }),
        }
    }
    steps
}

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub fn synthesize_pipeline(
        &self,
        text: &str,
        voice: &VoiceSpec,
        speed: f32,
        flags: DspFlags,
        chunker: &Chunker,
        cancel: &CancelToken,
        phonemize: &dyn Fn(&str) -> Result<String>,
        on_chunk: &mut dyn FnMut(Pcm) -> Result<()>,
    ) -> Result<()> {
        if flags == DspFlags::default() {
            let phonemes = phonemize(text)?;
            return self.synthesize_streaming(&phonemes, voice, speed, chunker, cancel, on_chunk);
        }
        let mut emitted = false;
        for step in plan(text, flags, speed) {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match step {
                Step::Silence { ms } => {
                    on_chunk(Pcm {
                        samples: silence(ms),
                        sample_rate: SAMPLE_RATE,
                    })?;
                    emitted = true;
                }
                Step::Synth {
                    text,
                    profile:
                        EmotionProfile {
                            volume,
                            speed,
                            pitch,
                            attack_ms,
                        },
                    attack_from,
                } => {
                    let mut pos = 0usize;
                    for chunk in chunker.split(&phonemize(&text)?) {
                        if cancel.is_cancelled() {
                            return Err(Error::Cancelled);
                        }
                        let mut pcm = self.synth_chunk(&chunk, voice, speed)?;
                        pcm.samples = pitch_shift(&pcm.samples, pitch);
                        attack_envelope(&mut pcm.samples, attack_from, volume, attack_ms, pos);
                        pos += pcm.samples.len();
                        on_chunk(pcm)?;
                        emitted = true;
                    }
                }
            }
        }
        match emitted {
            true => Ok(()),
            false => Err(Error::Input("no synthesizable tokens".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: DspFlags = DspFlags {
        emotion_tags: true,
        smart_punct: true,
    };

    #[test]
    fn plan_boundaries_profiles_envelope_sources() {
        let steps = plan("hi [whispers] there... ok", BOTH, 1.0);
        match steps.as_slice() {
            [
                Step::Synth {
                    text: t0,
                    profile: p0,
                    attack_from: a0,
                },
                Step::Synth {
                    text: t1,
                    profile: p1,
                    attack_from: a1,
                },
                Step::Silence { ms },
                Step::Synth {
                    text: t2,
                    profile: p2,
                    attack_from: a2,
                },
            ] => {
                assert_eq!((t0.as_str(), *a0), ("hi ", 1.0));
                assert_eq!(*p0, Emotion::Normal.profile(1.0, 1.0, 1.0));
                assert_eq!((t1.as_str(), *a1), (" there", 1.0));
                assert_eq!(*p1, Emotion::Whisper.profile(1.0, 1.0, 1.0));
                let base = 450.0 * 0.5 * 2.0 / p1.speed;
                let (lo, hi) = ((base * 0.9) as u32, (base * 1.1) as u32 + 1);
                assert!((lo..=hi).contains(ms), "{ms} outside {lo}..={hi}");
                assert_eq!(t2.as_str(), " ok");
                assert_eq!(p2, p1);
                assert!((a2 - p1.volume).abs() < 1e-6);
            }
            other => panic!("unexpected plan {other:?}"),
        }
    }

    #[test]
    fn plan_jitter_reproducible_from_input_hash() {
        let text = "one, two... three. four!";
        assert_eq!(plan(text, BOTH, 1.0), plan(text, BOTH, 1.0));
        let a = plan(text, BOTH, 1.0);
        let b = plan("one, two... three. four?", BOTH, 1.0);
        assert_ne!(a, b);
    }

    #[test]
    fn flags_gate_token_classes() {
        let punct_only = DspFlags {
            emotion_tags: false,
            smart_punct: true,
        };
        assert_eq!(
            segment("hi [whispers] there.", punct_only),
            vec![
                Segment::Text("hi [whispers] there".into()),
                Segment::Punct(Punct::Period),
            ]
        );
        let tags_only = DspFlags {
            emotion_tags: true,
            smart_punct: false,
        };
        assert_eq!(
            segment("hi, there [sad] friend.", tags_only),
            vec![
                Segment::Text("hi, there ".into()),
                Segment::Tag(Emotion::Sad),
                Segment::Text(" friend.".into()),
            ]
        );
    }

    #[test]
    fn envelope_ramps_across_chunk_offsets() {
        let ramp = SAMPLE_RATE as usize;
        let mut a = vec![1.0f32; ramp / 2];
        let mut b = vec![1.0f32; ramp];
        attack_envelope(&mut a, 0.0, 1.0, 1000, 0);
        attack_envelope(&mut b, 0.0, 1.0, 1000, ramp / 2);
        assert_eq!(a[0], 0.0);
        assert!((a[ramp / 4] - 0.25).abs() < 1e-3);
        assert!((b[0] - 0.5).abs() < 1e-3);
        assert_eq!(b[ramp - 1], 1.0);
    }

    #[test]
    fn silence_sample_count() {
        assert_eq!(silence(100).len(), SAMPLE_RATE as usize / 10);
    }
}
