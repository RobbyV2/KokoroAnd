use crate::{Error, Result};
use jieba_rs::Jieba;
use std::collections::{HashMap, HashSet};

const NUM_LOW: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const UNIT_LOW: [&str; 16] = [
    "", "十", "百", "千", "万", "十", "百", "千", "亿", "十", "百", "千", "万", "十", "百", "千",
];

fn integer_convert(digits: &str) -> String {
    let trimmed = digits.trim_start_matches('0');
    let data = match trimmed.is_empty() {
        true => "0",
        false => trimmed,
    };
    let len = data.len();
    if len > UNIT_LOW.len() {
        return digits.to_string();
    }
    let mut out = String::new();
    for (i, c) in data.chars().enumerate() {
        let d = c as usize - '0' as usize;
        let order = len - i - 1;
        if d != 0 {
            out.push_str(NUM_LOW[d]);
            out.push_str(UNIT_LOW[order]);
        } else {
            if order % 4 == 0 {
                out.push_str(NUM_LOW[d]);
                out.push_str(UNIT_LOW[order]);
            }
            if i > 0 && !out.ends_with('零') {
                out.push_str(NUM_LOW[d]);
            }
        }
    }
    let mut out = out
        .replace("零零", "零")
        .replace("零万", "万")
        .replace("零亿", "亿")
        .replace("亿万", "亿");
    out = out.trim_matches('零').to_string();
    let re = fancy_regex::Regex::new(r"([万亿])零([一二三四五六七八九][千])").unwrap();
    out = re.replace_all(&out, "$1$2").to_string();
    if let Some(rest) = out.strip_prefix("一十") {
        out = format!("十{rest}");
    }
    match out.is_empty() {
        true => "零".to_string(),
        false => out,
    }
}

fn an2cn_low(num: &str) -> String {
    let (sign, num) = match num.strip_prefix('-') {
        Some(rest) => ("负", rest),
        None => ("", num),
    };
    let body = match num.split_once('.') {
        Some((int, dec)) => {
            let mut s = integer_convert(int);
            s.push('点');
            for c in dec.chars().take(16) {
                s.push_str(NUM_LOW[c as usize - '0' as usize]);
            }
            s
        }
        None => integer_convert(num),
    };
    format!("{sign}{body}")
}

fn an2cn_direct(num: &str) -> String {
    num.chars()
        .map(|c| match c {
            '.' => "点",
            d => NUM_LOW[d as usize - '0' as usize],
        })
        .collect()
}

fn an2cn_transform(text: &str) -> String {
    let sub = |text: &str, pat: &str, f: &dyn Fn(&str) -> String| -> String {
        let re = fancy_regex::Regex::new(pat).unwrap();
        re.replace_all(text, |caps: &fancy_regex::Captures| f(&caps[0]))
            .to_string()
    };
    let text = sub(text, r"\d{2,4}(?=年)", &|m| an2cn_direct(m));
    let text = sub(&text, r"\d{1,2}(?=[月日])", &|m| an2cn_low(m));
    let text = sub(&text, r"\d+/\d+", &|m| match m.split_once('/') {
        Some((a, b)) => format!("{}分之{}", an2cn_low(b), an2cn_low(a)),
        None => m.to_string(),
    });
    let text = sub(&text, r"-?(\d+\.)?\d+%", &|m| {
        format!("百分之{}", an2cn_low(&m[..m.len() - 1]))
    });
    sub(&text, r"-?(\d+\.)?\d+", &|m| an2cn_low(m))
}

fn map_punctuation(text: &str) -> String {
    let pairs = [
        ("、", ", "),
        ("，", ", "),
        ("。", ". "),
        ("．", ". "),
        ("！", "! "),
        ("：", ": "),
        ("；", "; "),
        ("？", "? "),
        ("«", " “"),
        ("»", "” "),
        ("《", " “"),
        ("》", "” "),
        ("「", " “"),
        ("」", "” "),
        ("【", " “"),
        ("】", "” "),
        ("（", " ("),
        ("）", ") "),
    ];
    let mut out = text.to_string();
    for (from, to) in pairs {
        out = out.replace(from, to);
    }
    out.trim().to_string()
}

enum Syllable<'a> {
    Special(String, usize),
    Plain(Option<&'a str>, String, usize),
}

fn split_tone(syl: &str) -> (String, usize) {
    match syl.chars().last().and_then(|c| c.to_digit(10)) {
        Some(d) => (syl[..syl.len() - 1].to_string(), d as usize),
        None => (syl.to_string(), 5),
    }
}

const INITIALS: [&str; 21] = [
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s",
];

fn parse_syllable(syl: &str) -> Option<Syllable<'_>> {
    let (normal, tone) = split_tone(syl);
    if !normal.chars().all(|c| c.is_ascii_lowercase() || c == 'ê') {
        return None;
    }
    if matches!(
        normal.as_str(),
        "io" | "ê" | "er" | "o" | "hm" | "hng" | "m" | "n" | "ng"
    ) {
        return Some(Syllable::Special(normal, tone));
    }
    let initial = INITIALS
        .iter()
        .find(|i| normal.starts_with(**i) && normal.len() > i.len())
        .copied();
    let rest = &normal[initial.map_or(0, str::len)..];
    let final_ = match initial {
        None => match rest.strip_prefix('y') {
            Some(t) => match t.chars().next() {
                Some('u') => format!("ü{}", &t[1..]),
                Some('i') => t.to_string(),
                _ => format!("i{t}"),
            },
            None => match rest.strip_prefix('w') {
                Some("u") => "u".to_string(),
                Some(t) => format!("u{t}"),
                None => rest.to_string(),
            },
        },
        Some(i) => {
            let t = match rest {
                "iu" => "iou",
                "ui" => "uei",
                "un" if !matches!(i, "j" | "q" | "x") => "uen",
                other => other,
            };
            let t = t.replace('v', "ü");
            match matches!(i, "j" | "q" | "x") && t.starts_with('u') {
                true => format!("ü{}", &t[1..]),
                false => t,
            }
        }
    };
    Some(Syllable::Plain(initial, final_, tone))
}

fn final_ipa(initial: Option<&str>, final_: &str) -> Option<&'static [&'static str]> {
    let zh_group = matches!(initial, Some("zh" | "ch" | "sh" | "r"));
    let z_group = matches!(initial, Some("z" | "c" | "s"));
    let mapped: &[&str] = match final_ {
        "i" if zh_group || z_group => &["ɨ0"],
        "a" => &["a0"],
        "ai" => &["ai0"],
        "an" => &["a0", "n"],
        "ang" => &["a0", "ŋ"],
        "ao" => &["au0"],
        "e" => &["ɤ0"],
        "ei" => &["ei0"],
        "en" => &["ə0", "n"],
        "eng" => &["ə0", "ŋ"],
        "i" => &["i0"],
        "ia" => &["j", "a0"],
        "ian" => &["j", "ɛ0", "n"],
        "iang" => &["j", "a0", "ŋ"],
        "iao" => &["j", "au0"],
        "ie" => &["j", "e0"],
        "in" => &["i0", "n"],
        "iou" => &["j", "ou0"],
        "ing" => &["i0", "ŋ"],
        "iong" => &["j", "ʊ0", "ŋ"],
        "ong" => &["ʊ0", "ŋ"],
        "ou" => &["ou0"],
        "u" => &["u0"],
        "uei" => &["w", "ei0"],
        "ua" => &["w", "a0"],
        "uai" => &["w", "ai0"],
        "uan" => &["w", "a0", "n"],
        "uen" => &["w", "ə0", "n"],
        "uang" => &["w", "a0", "ŋ"],
        "ueng" => &["w", "ə0", "ŋ"],
        "uo" | "o" => &["w", "o0"],
        "ü" => &["y0"],
        "üe" => &["ɥ", "e0"],
        "üan" => &["ɥ", "ɛ0", "n"],
        "ün" => &["y0", "n"],
        _ => return None,
    };
    Some(mapped)
}

fn special_ipa(final_: &str) -> Option<&'static [&'static str]> {
    let mapped: &[&str] = match final_ {
        "io" => &["j", "ɔ0"],
        "ê" => &["ɛ0"],
        "er" => &["ɚ0"],
        "o" => &["ɔ0"],
        "hm" => &["h", "m0"],
        "hng" => &["h", "ŋ0"],
        "m" => &["m0"],
        "n" => &["n0"],
        "ng" => &["ŋ0"],
        _ => return None,
    };
    Some(mapped)
}

fn initial_ipa(initial: &str) -> &'static str {
    match initial {
        "b" => "p",
        "c" => "ʦʰ",
        "ch" => "\u{AB67}ʰ",
        "d" => "t",
        "f" => "f",
        "g" => "k",
        "h" => "x",
        "j" => "ʨ",
        "k" => "kʰ",
        "l" => "l",
        "m" => "m",
        "n" => "n",
        "p" => "pʰ",
        "q" => "ʨʰ",
        "r" => "ɻ",
        "s" => "s",
        "sh" => "ʂ",
        "t" => "tʰ",
        "x" => "ɕ",
        "z" => "ʦ",
        "zh" => "\u{AB67}",
        _ => "",
    }
}

fn tone_mark(tone: usize) -> &'static str {
    match tone {
        1 => "→",
        2 => "↗",
        3 => "↓",
        4 => "↘",
        _ => "",
    }
}

fn syl_to_ipa(syl: &str) -> String {
    let (phonemes, tone): (Vec<&str>, usize) = match parse_syllable(syl) {
        Some(Syllable::Special(normal, tone)) => match special_ipa(&normal) {
            Some(p) => (p.to_vec(), tone),
            None => return String::new(),
        },
        Some(Syllable::Plain(initial, final_, tone)) => {
            let mut v = Vec::new();
            if let Some(i) = initial {
                v.push(initial_ipa(i));
            }
            match final_ipa(initial, &final_) {
                Some(f) => v.extend_from_slice(f),
                None => return String::new(),
            }
            (v, tone)
        }
        None => return String::new(),
    };
    let mark = tone_mark(tone);
    phonemes
        .iter()
        .map(|p| p.replace('0', mark))
        .collect::<String>()
}

pub struct ZhG2p {
    jieba: Jieba,
    chars: HashMap<char, String>,
    phrases: HashMap<String, String>,
    prefixes: HashSet<String>,
}

#[derive(serde::Deserialize)]
struct PinyinData {
    chars: HashMap<String, String>,
    phrases: HashMap<String, String>,
}

impl ZhG2p {
    pub fn new(data_json: &str) -> Result<Self> {
        let data: PinyinData =
            serde_json::from_str(data_json).map_err(|e| Error::Dict(e.to_string()))?;
        let chars = data
            .chars
            .iter()
            .filter_map(|(k, v)| k.chars().next().map(|c| (c, v.clone())))
            .collect();
        let mut prefixes = HashSet::new();
        for phrase in data.phrases.keys() {
            let cs: Vec<char> = phrase.chars().collect();
            for i in 1..=cs.len() {
                prefixes.insert(cs[..i].iter().collect());
            }
        }
        Ok(Self {
            jieba: Jieba::new(),
            chars,
            phrases: data.phrases,
            prefixes,
        })
    }

    fn mmseg(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut remain: Vec<char> = word.chars().collect();
        while !remain.is_empty() {
            let mut last_valid_len = 0;
            let mut broke = false;
            for index in 0..remain.len() {
                let cand: String = remain[..=index].iter().collect();
                if self.prefixes.contains(&cand) {
                    if self.phrases.contains_key(&cand) {
                        last_valid_len = index + 1;
                    }
                } else {
                    match last_valid_len {
                        0 => {
                            out.push(remain[0].to_string());
                            remain.drain(..1);
                        }
                        n => {
                            out.push(remain[..n].iter().collect());
                            remain.drain(..n);
                        }
                    }
                    broke = true;
                    break;
                }
            }
            if !broke {
                match last_valid_len {
                    0 => {
                        for c in &remain {
                            out.push(c.to_string());
                        }
                        remain.clear();
                    }
                    n => {
                        out.push(remain[..n].iter().collect());
                        remain.drain(..n);
                    }
                }
            }
        }
        out
    }

    fn word_syllables(&self, word: &str) -> Vec<String> {
        let mut syls = Vec::new();
        for group in self.mmseg(word) {
            let stored = self.phrases.get(&group).map(String::as_str);
            match stored {
                Some(readings) if !readings.is_empty() => {
                    syls.extend(readings.split(' ').map(str::to_string));
                }
                _ => {
                    for c in group.chars() {
                        if let Some(s) = self.chars.get(&c) {
                            syls.push(s.clone());
                        }
                    }
                }
            }
        }
        syls
    }

    fn word_to_ipa(&self, word: &str) -> String {
        self.word_syllables(word)
            .iter()
            .map(|s| syl_to_ipa(s))
            .collect()
    }

    pub fn phonemize(&self, text: &str) -> Result<String> {
        if text.trim().is_empty() {
            return Err(Error::Input("empty text".into()));
        }
        let text = an2cn_transform(text);
        let text = map_punctuation(&text);
        let is_han = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        while let Some(&first) = chars.peek() {
            let han = is_han(first);
            let mut run = String::new();
            while let Some(&c) = chars.peek() {
                if is_han(c) != han {
                    break;
                }
                run.push(c);
                chars.next();
            }
            match han {
                true => {
                    let words = self.jieba.cut(&run, true);
                    let ipa: Vec<String> = words.iter().map(|t| self.word_to_ipa(t.word)).collect();
                    result.push_str(&ipa.join(" "));
                }
                false => result.push_str(&run),
            }
        }
        Ok(result.replace('\u{032F}', ""))
    }
}
