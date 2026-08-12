mod fallback;
mod ja;
mod lexicon;
mod normalize;
mod num;
mod pipeline;
mod tokenize;
mod zh;

use flate2::read::GzDecoder;
use lexicon::{Dict, Lex, Lexicon, grow_dictionary};
use std::collections::BTreeMap;
use std::io::Read;

pub use lexicon::EntryVal;
pub use normalize::normalize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("dict: {0}")]
    Dict(String),
    #[error("unsupported language {0:?}")]
    Unsupported(Lang),
    #[error("input: {0}")]
    Input(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    EnUs,
    EnGb,
    Ja,
    Zh,
    Es,
    Fr,
    Hi,
    It,
    PtBr,
}

impl Lang {
    pub fn for_voice(voice: &str) -> Self {
        match voice.get(..3) {
            Some("jf_") | Some("jm_") => Lang::Ja,
            Some("zf_") | Some("zm_") => Lang::Zh,
            _ => Lang::EnUs,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomDict(pub BTreeMap<String, EntryVal>);

impl CustomDict {
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| Error::Dict(e.to_string()))
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::Dict(e.to_string()))
    }

    fn grown(&self) -> Dict {
        grow_dictionary(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), Some(v.clone())))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EnOptions {
    pub normalize: bool,
    pub fallback: bool,
}

impl Default for EnOptions {
    fn default() -> Self {
        Self {
            normalize: true,
            fallback: true,
        }
    }
}

pub struct G2p {
    lexicon: Lexicon,
    ja: ja::JaG2p,
    zh: zh::ZhG2p,
}

pub(crate) fn gunzip(gz: &[u8]) -> Result<String> {
    let mut raw = String::new();
    GzDecoder::new(gz)
        .read_to_string(&mut raw)
        .map_err(|e| Error::Dict(e.to_string()))?;
    Ok(raw)
}

fn load_dict(gz: &[u8]) -> Result<Dict> {
    serde_json::from_str(&gunzip(gz)?).map_err(|e| Error::Dict(e.to_string()))
}

impl G2p {
    pub fn new() -> Result<Self> {
        let golds = load_dict(include_bytes!(concat!(env!("OUT_DIR"), "/us_gold.json.gz")))?;
        let silvers = load_dict(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/us_silver.json.gz"
        )))?;
        let zh_json = gunzip(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/zh_pinyin.json.gz"
        )))?;
        Ok(Self {
            lexicon: Lexicon::new(golds, silvers),
            ja: ja::JaG2p::new()?,
            zh: zh::ZhG2p::new(&zh_json)?,
        })
    }

    pub fn phonemize(
        &self,
        text: &str,
        lang: Lang,
        custom_dict: Option<&CustomDict>,
    ) -> Result<String> {
        match lang {
            Lang::EnUs => self.phonemize_en(text, custom_dict, EnOptions::default()),
            Lang::Ja => self.ja.phonemize(text),
            Lang::Zh => self.zh.phonemize(text),
            other => Err(Error::Unsupported(other)),
        }
    }

    pub fn phonemize_en(
        &self,
        text: &str,
        custom_dict: Option<&CustomDict>,
        opts: EnOptions,
    ) -> Result<String> {
        if text.trim().is_empty() {
            return Err(Error::Input("empty text".into()));
        }
        let custom = custom_dict.map(CustomDict::grown);
        let en = pipeline::EnG2p {
            lex: Lex {
                lex: &self.lexicon,
                custom: custom.as_ref(),
            },
            unk: String::new(),
            use_fallback: opts.fallback,
        };
        let text = if opts.normalize {
            normalize_outside_links(text)
        } else {
            text.to_string()
        };
        Ok(en.run(&text))
    }
}

fn normalize_outside_links(text: &str) -> String {
    let re = fancy_regex::Regex::new(r"\[[^\]]+\]\([^\)]*\)").unwrap_or_else(|e| panic!("{e}"));
    let mut parts: Vec<String> = Vec::new();
    let mut last = 0;
    for m in re.find_iter(text).flatten() {
        let seg = normalize::normalize(&text[last..m.start()]);
        if !seg.is_empty() {
            parts.push(seg);
        }
        parts.push(m.as_str().to_string());
        last = m.end();
    }
    if last == 0 {
        return normalize::normalize(text);
    }
    let tail = normalize::normalize(&text[last..]);
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts.join(" ")
}
