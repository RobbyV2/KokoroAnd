use crate::{Error, Result};
use jpreprocess::{
    DefaultTokenizer, JPreprocess, SystemDictionaryConfig, kind::JPreprocessDictionaryKind,
};
use jpreprocess_core::pos::POS;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

const HEPBURN: [(&str, &str); 189] = [
    ("«", "“"),
    ("»", "”"),
    ("—", "—"),
    ("、", ","),
    ("。", "."),
    ("《", "("),
    ("》", ")"),
    ("「", "“"),
    ("」", "”"),
    ("『", "“"),
    ("』", "”"),
    ("【", "["),
    ("】", "]"),
    ("〜", "—"),
    ("ぁ", "a"),
    ("あ", "a"),
    ("ぃ", "i"),
    ("い", "i"),
    ("いぇ", "je"),
    ("ぅ", "ɯ"),
    ("う", "ɯ"),
    ("うぃ", "βi"),
    ("うぇ", "βe"),
    ("うぉ", "βo"),
    ("ぇ", "e"),
    ("え", "e"),
    ("ぉ", "o"),
    ("お", "o"),
    ("か", "ka"),
    ("が", "ɡa"),
    ("き", "kʲi"),
    ("きぇ", "kʲe"),
    ("きゃ", "kʲa"),
    ("きゅ", "kʲɨ"),
    ("きょ", "kʲo"),
    ("ぎ", "ɡʲi"),
    ("ぎゃ", "ɡʲa"),
    ("ぎゅ", "ɡʲɨ"),
    ("ぎょ", "ɡʲo"),
    ("く", "kɯ"),
    ("くぁ", "kᵝa"),
    ("くぃ", "kᵝi"),
    ("くぇ", "kᵝe"),
    ("くぉ", "kᵝo"),
    ("ぐ", "ɡɯ"),
    ("ぐぁ", "ɡᵝa"),
    ("ぐぃ", "ɡᵝi"),
    ("ぐぇ", "ɡᵝe"),
    ("ぐぉ", "ɡᵝo"),
    ("け", "ke"),
    ("げ", "ɡe"),
    ("こ", "ko"),
    ("ご", "ɡo"),
    ("さ", "sa"),
    ("ざ", "ʣa"),
    ("し", "ɕi"),
    ("しぇ", "ɕe"),
    ("しゃ", "ɕa"),
    ("しゅ", "ɕɨ"),
    ("しょ", "ɕo"),
    ("じ", "ʥi"),
    ("じぇ", "ʥe"),
    ("じゃ", "ʥa"),
    ("じゅ", "ʥɨ"),
    ("じょ", "ʥo"),
    ("す", "sɨ"),
    ("ず", "zɨ"),
    ("せ", "se"),
    ("ぜ", "ʣe"),
    ("そ", "so"),
    ("ぞ", "ʣo"),
    ("た", "ta"),
    ("だ", "da"),
    ("ち", "ʨi"),
    ("ちぇ", "ʨe"),
    ("ちゃ", "ʨa"),
    ("ちゅ", "ʨɨ"),
    ("ちょ", "ʨo"),
    ("ぢ", "ʥi"),
    ("ぢゃ", "ʥa"),
    ("ぢゅ", "ʥɨ"),
    ("ぢょ", "ʥo"),
    ("つ", "ʦɨ"),
    ("つぁ", "ʦa"),
    ("つぃ", "ʦʲi"),
    ("つぇ", "ʦe"),
    ("つぉ", "ʦo"),
    ("づ", "zɨ"),
    ("て", "te"),
    ("てぃ", "tʲi"),
    ("てゅ", "tʲɨ"),
    ("で", "de"),
    ("でぃ", "dʲi"),
    ("でゅ", "dʲɨ"),
    ("と", "to"),
    ("とぅ", "tɯ"),
    ("ど", "do"),
    ("どぅ", "dɯ"),
    ("な", "na"),
    ("に", "ɲi"),
    ("にぇ", "ɲe"),
    ("にゃ", "ɲa"),
    ("にゅ", "ɲɨ"),
    ("にょ", "ɲo"),
    ("ぬ", "nɯ"),
    ("ね", "ne"),
    ("の", "no"),
    ("は", "ha"),
    ("ば", "ba"),
    ("ぱ", "pa"),
    ("ひ", "çi"),
    ("ひぇ", "çe"),
    ("ひゃ", "ça"),
    ("ひゅ", "çɨ"),
    ("ひょ", "ço"),
    ("び", "bʲi"),
    ("びゃ", "bʲa"),
    ("びゅ", "bʲɨ"),
    ("びょ", "bʲo"),
    ("ぴ", "pʲi"),
    ("ぴゃ", "pʲa"),
    ("ぴゅ", "pʲɨ"),
    ("ぴょ", "pʲo"),
    ("ふ", "ɸɯ"),
    ("ふぁ", "ɸa"),
    ("ふぃ", "ɸʲi"),
    ("ふぇ", "ɸe"),
    ("ふぉ", "ɸo"),
    ("ふゅ", "ɸʲɨ"),
    ("ふょ", "ɸʲo"),
    ("ぶ", "bɯ"),
    ("ぷ", "pɯ"),
    ("へ", "he"),
    ("べ", "be"),
    ("ぺ", "pe"),
    ("ほ", "ho"),
    ("ぼ", "bo"),
    ("ぽ", "po"),
    ("ま", "ma"),
    ("み", "mʲi"),
    ("みゃ", "mʲa"),
    ("みゅ", "mʲɨ"),
    ("みょ", "mʲo"),
    ("む", "mɯ"),
    ("め", "me"),
    ("も", "mo"),
    ("ゃ", "ja"),
    ("や", "ja"),
    ("ゅ", "jɯ"),
    ("ゆ", "jɯ"),
    ("ょ", "jo"),
    ("よ", "jo"),
    ("ら", "ɾa"),
    ("り", "ɾʲi"),
    ("りゃ", "ɾʲa"),
    ("りゅ", "ɾʲɨ"),
    ("りょ", "ɾʲo"),
    ("る", "ɾɯ"),
    ("れ", "ɾe"),
    ("ろ", "ɾo"),
    ("ゎ", "βa"),
    ("わ", "βa"),
    ("ゐ", "i"),
    ("ゑ", "e"),
    ("を", "o"),
    ("ゔ", "vɯ"),
    ("ゔぁ", "va"),
    ("ゔぃ", "vʲi"),
    ("ゔぇ", "ve"),
    ("ゔぉ", "vo"),
    ("ゔゅ", "bʲɨ"),
    ("ゔょ", "bʲo"),
    ("ゕ", "ka"),
    ("ゖ", "ke"),
    ("゙", ""),
    ("゚", ""),
    ("ヷ", "va"),
    ("ヸ", "vʲi"),
    ("ヹ", "ve"),
    ("ヺ", "vo"),
    ("・", " "),
    ("！", "!"),
    ("（", "("),
    ("）", ")"),
    ("，", ","),
    ("：", ":"),
    ("；", ";"),
    ("？", "?"),
    ("～", "—"),
];
const KATA_EXT: [(char, char); 16] = [
    ('ㇰ', 'ク'),
    ('ㇱ', 'シ'),
    ('ㇲ', 'ス'),
    ('ㇳ', 'ト'),
    ('ㇴ', 'ヌ'),
    ('ㇵ', 'ハ'),
    ('ㇶ', 'ヒ'),
    ('ㇷ', 'フ'),
    ('ㇸ', 'ヘ'),
    ('ㇹ', 'ホ'),
    ('ㇺ', 'ム'),
    ('ㇻ', 'ラ'),
    ('ㇼ', 'リ'),
    ('ㇽ', 'ル'),
    ('ㇾ', 'レ'),
    ('ㇿ', 'ロ'),
];

static TABLE: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| HEPBURN.iter().copied().collect());

const SUTEGANA: &str = "\u{3083}\u{3085}\u{3087}\u{3041}\u{3043}\u{3045}\u{3047}\u{3049}";

const HIRA_DIGITS: [&str; 10] = [
    "\u{30bc}\u{30ed}",
    "いち",
    "に",
    "さん",
    "よん",
    "ご",
    "ろく",
    "なな",
    "はち",
    "きゅう",
];

fn num_two(s: &[u8]) -> String {
    let d = |b: u8| HIRA_DIGITS[(b - b'0') as usize].to_string();
    match (s[0], s[1]) {
        (b'0', b) => d(b),
        (b'1', b'0') => "じゅう".into(),
        (b'1', b) => format!("じゅう{}", d(b)),
        (a, b'0') => format!("{}じゅう", d(a)),
        (a, b) => format!("{}じゅう{}", d(a), d(b)),
    }
}

fn num_three(s: &[u8]) -> String {
    let head = match s[0] {
        b'1' => "ひゃく".to_string(),
        b'3' => "さんびゃく".to_string(),
        b'6' => "ろっぴゃく".to_string(),
        b'8' => "はっぴゃく".to_string(),
        b => format!("{}ひゃく", HIRA_DIGITS[(b - b'0') as usize]),
    };
    let tail = match &s[1..] {
        [b'0', b'0'] => String::new(),
        [b'0', b] => HIRA_DIGITS[(b - b'0') as usize].to_string(),
        rest => num_two(rest),
    };
    format!("{head}{tail}")
}

fn num_four(s: &[u8], standalone: bool) -> String {
    if s == b"0000" {
        return String::new();
    }
    let s = &s[s.iter().position(|&b| b != b'0').unwrap_or(s.len() - 1)..];
    match s.len() {
        1 => HIRA_DIGITS[(s[0] - b'0') as usize].to_string(),
        2 => num_two(s),
        3 => num_three(s),
        _ => {
            let head = match (s[0], standalone) {
                (b'1', true) => "せん".to_string(),
                (b'1', false) => "いっせん".to_string(),
                (b'3', _) => "さんぜん".to_string(),
                (b'8', _) => "はっせん".to_string(),
                (b, _) => format!("{}せん", HIRA_DIGITS[(b - b'0') as usize]),
            };
            let tail = match &s[1..] {
                [b'0', b'0', b'0'] => String::new(),
                [b'0', rest @ ..] => num_two(rest),
                rest => num_three(rest),
            };
            format!("{head}{tail}")
        }
    }
}

fn num_to_kana(digits: &str) -> String {
    if digits.len() > 9 {
        return digits.to_string();
    }
    let trimmed = digits.trim_start_matches('0');
    let s = match trimmed.is_empty() {
        true => "0",
        false => trimmed,
    };
    let b = s.as_bytes();
    match b.len() {
        1 => HIRA_DIGITS[(b[0] - b'0') as usize].to_string(),
        2 => num_two(b),
        3 => num_three(b),
        4 => num_four(b, true),
        n => {
            let (head, last4) = b.split_at(n - 4);
            let front = match head.len() {
                1 => format!("{}まん", HIRA_DIGITS[(head[0] - b'0') as usize]),
                2 => format!("{}まん", num_two(head)),
                3 => format!("{}まん", num_three(head)),
                4 => format!("{}まん", num_four(head, false)),
                _ => {
                    let man = &head[1..];
                    let man_part = match man == b"0000" {
                        true => String::new(),
                        false => format!("{}まん", num_four(man, false)),
                    };
                    format!("{}おく{}", HIRA_DIGITS[(head[0] - b'0') as usize], man_part)
                }
            };
            format!("{front}{}", num_four(last4, false))
        }
    }
}

fn kata2hira(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{30a1}'..='\u{30f6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            other => other,
        })
        .collect()
}

fn collapse_vowels(hira: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in hira.chars() {
        let vowels: &[char] = match c {
            'あ' => &['a'],
            'い' => &['i', 'e'],
            'う' => &['ɯ'],
            'え' => &['e'],
            'お' => &['o'],
            _ => &[],
        };
        let long = match prev {
            Some(p) => lookup(p.to_string().as_str())
                .and_then(|r| r.chars().last())
                .map(|last| vowels.contains(&last))
                .unwrap_or(false),
            None => false,
        };
        match long {
            true => out.push('ー'),
            false => out.push(c),
        }
        prev = Some(match long {
            true => 'ー',
            false => c,
        });
    }
    out
}

fn normalize(text: &str) -> String {
    let re = fancy_regex::Regex::new(r"[\u{301C}\u{FF5E}](?=\d)").unwrap();
    let text = re.replace_all(text, "から").to_string();
    let text: String = text
        .chars()
        .map(|c| {
            KATA_EXT
                .iter()
                .find(|(k, _)| *k == c)
                .map_or(c, |(_, v)| *v)
        })
        .collect();
    let text: String = text.nfkc().collect();
    let re = fancy_regex::Regex::new(r"\d+").unwrap();
    let mut out = String::new();
    let mut last = 0;
    for m in re.find_iter(&text).flatten() {
        out.push_str(&text[last..m.start()]);
        out.push(' ');
        out.push_str(&num_to_kana(m.as_str()));
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

fn map_kana(hira: &str) -> String {
    let chars: Vec<char> = hira.chars().collect();
    let mut out = String::new();
    for (i, &kk) in chars.iter().enumerate() {
        let pk = match i {
            0 => None,
            _ => Some(chars[i - 1]),
        };
        let nk = chars.get(i + 1).copied();
        out.push_str(&single_mapping(pk, kk, nk));
    }
    out
}

fn lookup(k: &str) -> Option<&'static str> {
    TABLE.get(k).copied()
}

fn lookup2(a: char, b: char) -> Option<&'static str> {
    let mut s = String::new();
    s.push(a);
    s.push(b);
    lookup(&s)
}

fn single_mapping(pk: Option<char>, kk: char, nk: Option<char>) -> String {
    if "\u{309d}\u{30fd}\u{309e}\u{30fe}\u{3005}\u{3003}".contains(kk) {
        return String::new();
    }
    if let Some(p) = pk
        && let Some(v) = lookup2(p, kk)
    {
        return v.to_string();
    }
    if let Some(n) = nk
        && lookup2(kk, n).is_some()
    {
        return String::new();
    }
    if let Some(n) = nk
        && SUTEGANA.contains(n)
    {
        if kk == '\u{3063}' {
            return String::new();
        }
        let base = lookup(kk.to_string().as_str()).unwrap_or("");
        let ext = lookup(n.to_string().as_str()).unwrap_or("");
        let mut chars: Vec<char> = base.chars().collect();
        chars.pop();
        let mut s: String = chars.into_iter().collect();
        s.push_str(ext);
        return s;
    }
    if SUTEGANA.contains(kk) {
        return String::new();
    }
    if kk == '\u{30fc}' {
        return "\u{2d0}".to_string();
    }
    if kk == '\u{3063}' {
        return "\u{294}".to_string();
    }
    if kk == '\u{3093}' {
        let tnk = nk.and_then(|n| lookup(n.to_string().as_str()));
        if let Some(t) = tnk {
            let first = t.chars().next().unwrap_or(' ');
            if "mpb".contains(first) {
                return "m".to_string();
            }
            if "k\u{261}".contains(first) {
                return "\u{14b}".to_string();
            }
            if t.starts_with('\u{272}') || t.starts_with('\u{2a5}') || t.starts_with('\u{2a8}') {
                return "\u{272}".to_string();
            }
            if "ntd\u{27e}z".contains(first) {
                return "n".to_string();
            }
        }
        return "\u{274}".to_string();
    }
    lookup(kk.to_string().as_str())
        .map(str::to_string)
        .unwrap_or_default()
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{30ff}')
}

pub struct JaG2p {
    jpre: JPreprocess<DefaultTokenizer>,
    ja_words: HashSet<String>,
    max_word: usize,
}

impl JaG2p {
    pub fn new() -> Result<Self> {
        let system = SystemDictionaryConfig::Bundled(JPreprocessDictionaryKind::NaistJdic)
            .load()
            .map_err(|e| Error::Dict(e.to_string()))?;
        let raw = crate::gunzip(include_bytes!(concat!(env!("OUT_DIR"), "/ja_words.txt.gz")))?;
        let ja_words: HashSet<String> = raw.lines().map(str::to_string).collect();
        let max_word = ja_words
            .iter()
            .map(|w| w.chars().count())
            .max()
            .unwrap_or(0);
        Ok(Self {
            jpre: JPreprocess::with_dictionaries(system, None),
            ja_words,
            max_word,
        })
    }

    pub fn phonemize(&self, text: &str) -> Result<String> {
        let text = normalize(text);
        let mut njd = self
            .jpre
            .text_to_njd(&text)
            .map_err(|e| Error::Input(e.to_string()))?;
        njd.preprocess();
        let mut items: Vec<(String, String)> = Vec::new();
        for node in &njd.nodes {
            let surface = node.get_string().to_string();
            let hira = match node.get_pos() {
                POS::Kigou(_) => String::new(),
                _ => {
                    let mut pron = node.get_pron().to_pure_string();
                    if surface.ends_with('日')
                        && let Some(stem) = pron.strip_suffix('ビ')
                    {
                        pron = format!("{stem}ヒ");
                    }
                    let source = match pron.is_empty() {
                        true => match surface.chars().all(is_kana) {
                            true => surface.clone(),
                            false => String::new(),
                        },
                        false => pron,
                    };
                    collapse_vowels(&kata2hira(&source))
                }
            };
            if surface == "ので" {
                items.push(("の".to_string(), "の".to_string()));
                items.push(("で".to_string(), "で".to_string()));
                continue;
            }
            items.push((surface, hira));
        }
        for i in 0..items.len().saturating_sub(1) {
            if items[i].0 == "何" && matches!(items[i + 1].0.as_str(), "も" | "で") {
                items[i].1 = "なん".to_string();
            }
        }
        let mut tokens: Vec<(String, bool)> = Vec::new();
        let mut i = 0;
        while i < items.len() {
            let mut end = i + 1;
            let mut joined = items[i].0.clone();
            for (next, (surface, _)) in items.iter().enumerate().skip(i + 1) {
                joined.push_str(surface);
                if joined.chars().count() > self.max_word {
                    break;
                }
                if self.ja_words.contains(&joined) {
                    end = next + 1;
                }
            }
            let surface: String = items[i..end].iter().map(|(s, _)| s.as_str()).collect();
            let hira: String = items[i..end].iter().map(|(_, h)| h.as_str()).collect();
            i = end;
            let folded: String = surface.nfkc().collect();
            let roma = match folded.is_ascii() {
                true => folded,
                false => map_kana(&hira),
            };
            let space = match roma.as_str() {
                "" | "(" | "[" => {
                    if let Some(last) = tokens.last_mut() {
                        last.1 = true;
                    }
                    false
                }
                ")" | "]" | "." | "," | "?" | "!" | ":" => {
                    if let Some(last) = tokens.last_mut() {
                        last.1 = false;
                    }
                    true
                }
                " " => false,
                _ => true,
            };
            tokens.push((roma, space));
        }
        let mut out = String::new();
        for (roma, space) in &tokens {
            out.push_str(roma);
            if *space {
                out.push(' ');
            }
        }
        let re = fancy_regex::Regex::new(r"\s+").unwrap();
        let ps = re.replace_all(out.trim(), " ").to_string();
        let ps = ps.replace('(', "\u{ab}").replace(')', "\u{bb}");
        let re = fancy_regex::Regex::new(
            "(?<![!\",.:;?\u{bb}\u{2014}\u{2026}\u{201d}]) (?=\u{294})|(?<=\u{294}) (?![\"\u{ab}\u{201c}])",
        )
        .unwrap();
        Ok(re.replace_all(&ps, "").to_string())
    }
}
