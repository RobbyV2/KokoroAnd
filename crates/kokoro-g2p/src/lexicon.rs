use crate::num;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PRIMARY: char = 'ˈ';
pub const SECONDARY: char = 'ˌ';
pub const VOWELS: &str = "AIOQWYaiuæɑɒɔəɛɜɪʊʌᵻ";
pub const CONSONANTS: &str = "bdfhjklmnpstvwzðŋɡɹɾʃʒʤʧθ";
pub const PUNCTS: &str = ";:,.!?—…\"“”";
pub const NON_QUOTE_PUNCTS: &str = ";:,.!?—…";
pub const SUBTOKEN_JUNKS: &str = "',-._‘’/";
const DIPHTHONGS: &str = "AIOQWYʤʧ";
const US_TAUS: &str = "AIOWYiuæɑəɛɪɹʊʌ";
const ORDINALS: [&str; 4] = ["st", "nd", "rd", "th"];

pub fn currency_units(c: char) -> Option<(&'static str, &'static str)> {
    match c {
        '$' => Some(("dollar", "cent")),
        '£' => Some(("pound", "pence")),
        '€' => Some(("euro", "cent")),
        _ => None,
    }
}

pub fn add_symbol(w: &str) -> Option<&'static str> {
    match w {
        "." => Some("dot"),
        "/" => Some("slash"),
        _ => None,
    }
}

pub fn symbol(w: &str) -> Option<&'static str> {
    match w {
        "%" => Some("percent"),
        "&" => Some("and"),
        "+" => Some("plus"),
        "@" => Some("at"),
        _ => None,
    }
}

pub fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn lexicon_ord(c: char) -> bool {
    matches!(c, '\'' | '-' | 'A'..='Z' | 'a'..='z')
}

pub fn stress_weight(ps: &str) -> usize {
    ps.chars()
        .map(|c| if DIPHTHONGS.contains(c) { 2 } else { 1 })
        .sum()
}

fn restress(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut items: Vec<(f64, char)> = chars
        .iter()
        .enumerate()
        .map(|(i, c)| (i as f64, *c))
        .collect();
    for i in 0..chars.len() {
        if (chars[i] == PRIMARY || chars[i] == SECONDARY)
            && let Some(j) = (i..chars.len()).find(|&j| VOWELS.contains(chars[j]))
        {
            items[i].0 = j as f64 - 0.5;
        }
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.into_iter().map(|(_, c)| c).collect()
}

pub fn apply_stress(ps: &str, stress: Option<f64>) -> String {
    let has = |c: char| ps.contains(c);
    let no_marks = !has(PRIMARY) && !has(SECONDARY);
    let no_vowels = !ps.chars().any(|c| VOWELS.contains(c));
    let s = match stress {
        None => return ps.into(),
        Some(s) => s,
    };
    if s < -1.0 {
        ps.replace([PRIMARY, SECONDARY], "")
    } else if s == -1.0 || ((s == 0.0 || s == -0.5) && has(PRIMARY)) {
        ps.replace(SECONDARY, "")
            .replace(PRIMARY, &SECONDARY.to_string())
    } else if (s == 0.0 || s == 0.5 || s == 1.0) && no_marks {
        if no_vowels {
            ps.into()
        } else {
            restress(&format!("{SECONDARY}{ps}"))
        }
    } else if s >= 1.0 && !has(PRIMARY) && has(SECONDARY) {
        ps.replace(SECONDARY, &PRIMARY.to_string())
    } else if s > 1.0 && no_marks {
        if no_vowels {
            ps.into()
        } else {
            restress(&format!("{PRIMARY}{ps}"))
        }
    } else {
        ps.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EntryVal {
    Ps(String),
    Pos(BTreeMap<String, Option<String>>),
}

pub type Dict = HashMap<String, Option<EntryVal>>;

fn capitalize(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &it.as_str().to_lowercase(),
        None => String::new(),
    }
}

pub fn grow_dictionary(d: Dict) -> Dict {
    let mut e = Dict::new();
    for (k, v) in &d {
        if k.chars().count() < 2 {
            continue;
        }
        let lower = k.to_lowercase();
        let cap = capitalize(k);
        if *k == lower {
            if *k != cap {
                e.insert(cap, v.clone());
            }
        } else if *k == capitalize(&lower) {
            e.insert(lower, v.clone());
        }
    }
    for (k, v) in d {
        e.insert(k, v);
    }
    e
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Ctx {
    pub future_vowel: Option<bool>,
    pub future_to: bool,
}

pub type Res = (Option<String>, Option<i32>);

pub struct WordQuery<'a> {
    pub text: &'a str,
    pub alias: Option<&'a str>,
    pub tag: &'a str,
    pub stress: Option<f64>,
    pub currency: Option<char>,
    pub is_head: bool,
    pub num_flags: &'a str,
}

pub struct Lexicon {
    pub golds: Dict,
    pub silvers: Dict,
    pub cap_stresses: (f64, f64),
    pub pos_words: HashSet<String>,
}

pub struct Lex<'a> {
    pub lex: &'a Lexicon,
    pub custom: Option<&'a Dict>,
}

impl Lexicon {
    pub fn new(golds: Dict, silvers: Dict) -> Self {
        let golds = grow_dictionary(golds);
        let pos_words = golds
            .iter()
            .filter(|(_, v)| matches!(v, Some(EntryVal::Pos(_))))
            .map(|(k, _)| k.clone())
            .collect();
        Self {
            golds,
            silvers: grow_dictionary(silvers),
            cap_stresses: (0.5, 2.0),
            pos_words,
        }
    }
}

fn parent_tag(tag: Option<&str>) -> Option<String> {
    let t = tag?;
    Some(if t.starts_with("VB") {
        "VERB".into()
    } else if t.starts_with("NN") {
        "NOUN".into()
    } else if t.starts_with("ADV") || t.starts_with("RB") {
        "ADV".into()
    } else if t.starts_with("ADJ") || t.starts_with("JJ") {
        "ADJ".into()
    } else {
        t.into()
    })
}

impl<'a> Lex<'a> {
    fn gold_raw(&self, w: &str) -> Option<&Option<EntryVal>> {
        match self.custom.and_then(|c| c.get(w)) {
            Some(v) => Some(v),
            None => self.lex.golds.get(w),
        }
    }

    fn gold_has(&self, w: &str) -> bool {
        self.custom.is_some_and(|c| c.contains_key(w)) || self.lex.golds.contains_key(w)
    }

    fn gold_str(&self, w: &str) -> Option<String> {
        match self.gold_raw(w) {
            Some(Some(EntryVal::Ps(s))) => Some(s.clone()),
            _ => None,
        }
    }

    fn gold_pos(&self, w: &str, key: &str) -> Option<String> {
        match self.gold_raw(w) {
            Some(Some(EntryVal::Pos(m))) => m.get(key).cloned().flatten(),
            _ => None,
        }
    }

    pub fn get_nnp(&self, word: &str) -> Res {
        let mut joined = String::new();
        for c in word.chars().filter(|c| c.is_alphabetic()) {
            let up: String = c.to_uppercase().collect();
            match self.gold_str(&up) {
                Some(p) => joined.push_str(&p),
                None => return (None, None),
            }
        }
        let ps = apply_stress(&joined, Some(0.0));
        let ps = match ps.rfind(SECONDARY) {
            Some(i) => {
                let mut s = ps;
                s.replace_range(i..i + SECONDARY.len_utf8(), &PRIMARY.to_string());
                s
            }
            None => ps,
        };
        (Some(ps), Some(3))
    }

    fn get_special_case(&self, word: &str, tag: &str, stress: Option<f64>, ctx: &Ctx) -> Res {
        if tag == "ADD" && add_symbol(word).is_some() {
            return self.lookup(
                add_symbol(word).unwrap_or_default(),
                None,
                Some(-0.5),
                Some(ctx),
            );
        }
        if let Some(s) = symbol(word) {
            return self.lookup(s, None, None, Some(ctx));
        }
        let stripped = word.trim_matches('.');
        if stripped.contains('.')
            && word.replace('.', "").chars().all(|c| c.is_alphabetic())
            && !word.replace('.', "").is_empty()
            && word
                .split('.')
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
                < 3
        {
            return self.get_nnp(word);
        }
        match word {
            "a" | "A" => {
                return (
                    Some(if tag == "DT" {
                        "ɐ".into()
                    } else {
                        "ˈA".into()
                    }),
                    Some(4),
                );
            }
            "am" | "Am" | "AM" => {
                if tag.starts_with("NN") {
                    return self.get_nnp(word);
                }
                if ctx.future_vowel.is_none() || word != "am" || stress.is_some_and(|s| s > 0.0) {
                    return (self.gold_str("am"), Some(4));
                }
                return (Some("ɐm".into()), Some(4));
            }
            "an" | "An" | "AN" => {
                if word == "AN" && tag.starts_with("NN") {
                    return self.get_nnp(word);
                }
                return (Some("ɐn".into()), Some(4));
            }
            "I" if tag == "PRP" => return (Some(format!("{SECONDARY}I")), Some(4)),
            "by" | "By" | "BY" if parent_tag(Some(tag)).as_deref() == Some("ADV") => {
                return (Some("bˈI".into()), Some(4));
            }
            "to" | "To" => {
                return (
                    Some(match ctx.future_vowel {
                        None => self.gold_str("to").unwrap_or_default(),
                        Some(false) => "tə".into(),
                        Some(true) => "tʊ".into(),
                    }),
                    Some(4),
                );
            }
            "TO" if tag == "TO" || tag == "IN" => {
                return (
                    Some(match ctx.future_vowel {
                        None => self.gold_str("to").unwrap_or_default(),
                        Some(false) => "tə".into(),
                        Some(true) => "tʊ".into(),
                    }),
                    Some(4),
                );
            }
            "in" | "In" => {
                let st = if ctx.future_vowel.is_none() || tag != "IN" {
                    PRIMARY.to_string()
                } else {
                    String::new()
                };
                return (Some(format!("{st}ɪn")), Some(4));
            }
            "IN" if tag != "NNP" => {
                let st = if ctx.future_vowel.is_none() || tag != "IN" {
                    PRIMARY.to_string()
                } else {
                    String::new()
                };
                return (Some(format!("{st}ɪn")), Some(4));
            }
            "the" | "The" => {
                return (
                    Some(if ctx.future_vowel == Some(true) {
                        "ði".into()
                    } else {
                        "ðə".into()
                    }),
                    Some(4),
                );
            }
            "THE" if tag == "DT" => {
                return (
                    Some(if ctx.future_vowel == Some(true) {
                        "ði".into()
                    } else {
                        "ðə".into()
                    }),
                    Some(4),
                );
            }
            _ => {}
        }
        if tag == "IN" && (word.eq_ignore_ascii_case("vs") || word.eq_ignore_ascii_case("vs.")) {
            return self.lookup("versus", None, None, Some(ctx));
        }
        if matches!(word, "used" | "Used" | "USED") {
            let key = if matches!(tag, "VBD" | "JJ") && ctx.future_to {
                "VBD"
            } else {
                "DEFAULT"
            };
            return (self.gold_pos("used", key), Some(4));
        }
        (None, None)
    }

    pub fn is_known(&self, word: &str) -> bool {
        if self.gold_has(word) || symbol(word).is_some() || self.lex.silvers.contains_key(word) {
            return true;
        }
        if !word.chars().all(|c| c.is_alphabetic())
            || !word.chars().all(lexicon_ord)
            || word.is_empty()
        {
            return false;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if word == word.to_uppercase() && self.gold_has(&word.to_lowercase()) {
            return true;
        }
        let rest: String = word.chars().skip(1).collect();
        rest == rest.to_uppercase()
    }

    pub fn lookup(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<&Ctx>,
    ) -> Res {
        let mut word = word.to_string();
        let mut is_nnp = false;
        if word == word.to_uppercase() && !self.gold_has(&word) {
            word = word.to_lowercase();
            is_nnp = tag == Some("NNP");
        }
        let (mut ps, mut rating): (Option<EntryVal>, i32) =
            (self.gold_raw(&word).cloned().flatten(), 4);
        if ps.is_none() && !is_nnp {
            ps = self.lex.silvers.get(&word).cloned().flatten();
            rating = 3;
        }
        let ps: Option<String> = match ps {
            Some(EntryVal::Ps(s)) => Some(s),
            Some(EntryVal::Pos(m)) => {
                let key: String =
                    if ctx.is_some_and(|c| c.future_vowel.is_none()) && m.contains_key("None") {
                        "None".into()
                    } else if tag.is_some_and(|t| m.contains_key(t)) {
                        tag.unwrap_or_default().into()
                    } else {
                        parent_tag(tag).unwrap_or_default()
                    };
                match m.get(&key) {
                    Some(v) => v.clone(),
                    None => m.get("DEFAULT").cloned().flatten(),
                }
            }
            None => None,
        };
        if ps.is_none() || (is_nnp && !ps.as_deref().unwrap_or_default().contains(PRIMARY)) {
            let (nps, nrating) = self.get_nnp(&word);
            if nps.is_some() {
                return (nps, nrating);
            }
        }
        (ps.map(|p| apply_stress(&p, stress)), Some(rating))
    }

    fn suffix_s(&self, stem: Option<String>) -> Option<String> {
        let stem = stem?;
        let last = stem.chars().last()?;
        Some(if "ptkfθ".contains(last) {
            format!("{stem}s")
        } else if "szʃʒʧʤ".contains(last) {
            format!("{stem}ᵻz")
        } else {
            format!("{stem}z")
        })
    }

    fn suffix_ed(&self, stem: Option<String>) -> Option<String> {
        let stem = stem?;
        let chars: Vec<char> = stem.chars().collect();
        let last = *chars.last()?;
        Some(if "pkfθʃsʧ".contains(last) {
            format!("{stem}t")
        } else if last == 'd' {
            format!("{stem}ᵻd")
        } else if last != 't' {
            format!("{stem}d")
        } else if chars.len() < 2 {
            format!("{stem}ɪd")
        } else if US_TAUS.contains(chars[chars.len() - 2]) {
            let head: String = chars[..chars.len() - 1].iter().collect();
            format!("{head}ɾᵻd")
        } else {
            format!("{stem}ᵻd")
        })
    }

    fn suffix_ing(&self, stem: Option<String>) -> Option<String> {
        let stem = stem?;
        let chars: Vec<char> = stem.chars().collect();
        if chars.len() > 1 && *chars.last()? == 't' && US_TAUS.contains(chars[chars.len() - 2]) {
            let head: String = chars[..chars.len() - 1].iter().collect();
            return Some(format!("{head}ɾɪŋ"));
        }
        Some(format!("{stem}ɪŋ"))
    }

    pub fn stem_s(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<&Ctx>,
    ) -> Res {
        if word.chars().count() < 3 || !word.ends_with('s') {
            return (None, None);
        }
        let stem = if !word.ends_with("ss") && self.is_known(&word[..word.len() - 1]) {
            word[..word.len() - 1].to_string()
        } else if (word.ends_with("'s")
            || (word.chars().count() > 4 && word.ends_with("es") && !word.ends_with("ies")))
            && self.is_known(&word[..word.len() - 2])
        {
            word[..word.len() - 2].to_string()
        } else if word.chars().count() > 4
            && word.ends_with("ies")
            && self.is_known(&format!("{}y", &word[..word.len() - 3]))
        {
            format!("{}y", &word[..word.len() - 3])
        } else {
            return (None, None);
        };
        let (ps, rating) = self.lookup(&stem, tag, stress, ctx);
        (self.suffix_s(ps), rating)
    }

    pub fn stem_ed(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<&Ctx>,
    ) -> Res {
        if word.chars().count() < 4 || !word.ends_with('d') {
            return (None, None);
        }
        let stem = if !word.ends_with("dd") && self.is_known(&word[..word.len() - 1]) {
            word[..word.len() - 1].to_string()
        } else if word.chars().count() > 4
            && word.ends_with("ed")
            && !word.ends_with("eed")
            && self.is_known(&word[..word.len() - 2])
        {
            word[..word.len() - 2].to_string()
        } else {
            return (None, None);
        };
        let (ps, rating) = self.lookup(&stem, tag, stress, ctx);
        (self.suffix_ed(ps), rating)
    }

    pub fn stem_ing(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f64>,
        ctx: Option<&Ctx>,
    ) -> Res {
        if word.chars().count() < 5 || !word.ends_with("ing") {
            return (None, None);
        }
        let stem = if word.chars().count() > 5 && self.is_known(&word[..word.len() - 3]) {
            word[..word.len() - 3].to_string()
        } else if self.is_known(&format!("{}e", &word[..word.len() - 3])) {
            format!("{}e", &word[..word.len() - 3])
        } else if word.chars().count() > 5
            && double_consonant_ing(word)
            && self.is_known(&word[..word.len() - 4])
        {
            word[..word.len() - 4].to_string()
        } else {
            return (None, None);
        };
        let (ps, rating) = self.lookup(&stem, tag, stress, ctx);
        (self.suffix_ing(ps), rating)
    }

    pub fn get_word(&self, word: &str, tag: &str, stress: Option<f64>, ctx: &Ctx) -> Res {
        let (ps, rating) = self.get_special_case(word, tag, stress, ctx);
        if ps.is_some() {
            return (ps, rating);
        }
        let wl = word.to_lowercase();
        let mut word = word.to_string();
        if word.chars().count() > 1
            && word.replace('\'', "").chars().all(|c| c.is_alphabetic())
            && !word.replace('\'', "").is_empty()
            && word != word.to_lowercase()
            && (tag != "NNP" || word.chars().count() > 7)
            && !self.gold_has(&word)
            && !self.lex.silvers.contains_key(&word)
            && (word == word.to_uppercase() || {
                let rest: String = word.chars().skip(1).collect();
                rest == rest.to_lowercase()
            })
            && (self.gold_has(&wl)
                || self.lex.silvers.contains_key(&wl)
                || self.stem_s(&wl, Some(tag), stress, Some(ctx)).0.is_some()
                || self.stem_ed(&wl, Some(tag), stress, Some(ctx)).0.is_some()
                || self.stem_ing(&wl, Some(tag), stress, Some(ctx)).0.is_some())
        {
            word = wl;
        }
        if self.is_known(&word) {
            return self.lookup(&word, Some(tag), stress, Some(ctx));
        }
        if word.ends_with("s'") && self.is_known(&format!("{}'s", &word[..word.len() - 2])) {
            return self.lookup(
                &format!("{}'s", &word[..word.len() - 2]),
                Some(tag),
                stress,
                Some(ctx),
            );
        }
        if word.ends_with('\'') && self.is_known(&word[..word.len() - 1]) {
            return self.lookup(&word[..word.len() - 1], Some(tag), stress, Some(ctx));
        }
        let (s, rating) = self.stem_s(&word, Some(tag), stress, Some(ctx));
        if s.is_some() {
            return (s, rating);
        }
        let (ed, rating) = self.stem_ed(&word, Some(tag), stress, Some(ctx));
        if ed.is_some() {
            return (ed, rating);
        }
        let (ing, rating) = self.stem_ing(&word, Some(tag), Some(stress.unwrap_or(0.5)), Some(ctx));
        if ing.is_some() {
            return (ing, rating);
        }
        (None, None)
    }

    fn is_currency(word: &str) -> bool {
        match word.split_once('.') {
            None => true,
            Some((_, cents)) => !cents.contains('.') && cents.chars().count() < 3,
        }
    }

    pub fn get_number(
        &self,
        word: &str,
        currency: Option<char>,
        is_head: bool,
        num_flags: &str,
    ) -> Res {
        let suffix_start = word
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_lowercase() || *c == '\'')
            .last()
            .map(|(i, _)| i);
        let (word, suffix): (String, Option<String>) = match suffix_start {
            Some(i) if i < word.len() => (word[..i].to_string(), Some(word[i..].to_string())),
            _ => (word.to_string(), None),
        };
        let mut result: Vec<(String, i32)> = Vec::new();
        let push = |r: Res, result: &mut Vec<(String, i32)>| {
            if let (Some(p), rating) = r {
                result.push((p, rating.unwrap_or(3)));
            }
        };
        let mut word = word;
        if let Some(rest) = word.strip_prefix('-') {
            push(self.lookup("minus", None, None, None), &mut result);
            word = rest.to_string();
        }
        let extend_num = |num: &str, first: bool, escape: bool, result: &mut Vec<(String, i32)>| {
            let words_str = if escape {
                num.to_string()
            } else {
                num.parse::<u64>().map(num::cardinal).unwrap_or_default()
            };
            let splits: Vec<&str> = words_str
                .split(|c: char| !c.is_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            for (i, w) in splits.iter().enumerate() {
                if *w != "and" || num_flags.contains('&') {
                    if first && i == 0 && splits.len() > 1 && *w == "one" && num_flags.contains('a')
                    {
                        result.push(("ə".into(), 4));
                    } else {
                        let stress = if *w == "point" { Some(-2.0) } else { None };
                        if let (Some(p), r) = self.lookup(w, None, stress, None) {
                            result.push((p, r.unwrap_or(3)));
                        }
                    }
                } else if *w == "and"
                    && num_flags.contains('n')
                    && let Some(last) = result.last_mut()
                {
                    last.0.push_str("ən");
                }
            }
        };
        let ordinal_suffix = suffix.as_deref().is_some_and(|s| ORDINALS.contains(&s));
        if is_digits(&word) && ordinal_suffix {
            let w = word.parse::<u64>().map(num::ordinal).unwrap_or_default();
            extend_num(&w, true, true, &mut result);
        } else if result.is_empty()
            && word.chars().count() == 4
            && currency.and_then(currency_units).is_none()
            && is_digits(&word)
        {
            let w = word.parse::<u64>().map(num::year).unwrap_or_default();
            extend_num(&w, true, true, &mut result);
        } else if !is_head && !word.contains('.') {
            let numstr = word.replace(',', "");
            if numstr.starts_with('0') || numstr.chars().count() > 3 {
                for c in numstr.chars() {
                    extend_num(&c.to_string(), false, false, &mut result);
                }
            } else if numstr.chars().count() == 3 && !numstr.ends_with("00") {
                extend_num(&numstr[..1], true, false, &mut result);
                if numstr[1..2] == *"0" {
                    push(self.lookup("O", None, Some(-2.0), None), &mut result);
                    extend_num(&numstr[2..], false, false, &mut result);
                } else {
                    extend_num(&numstr[1..], false, false, &mut result);
                }
            } else {
                extend_num(&numstr, true, false, &mut result);
            }
        } else if word.matches('.').count() > 1 || !is_head {
            let mut first = true;
            for numpart in word.replace(',', "").split('.') {
                if numpart.is_empty() {
                } else if numpart.starts_with('0')
                    || (numpart.chars().count() != 2 && numpart.chars().skip(1).any(|c| c != '0'))
                {
                    for c in numpart.chars() {
                        extend_num(&c.to_string(), false, false, &mut result);
                    }
                } else {
                    extend_num(numpart, first, false, &mut result);
                }
                first = false;
            }
        } else if let Some(units) = currency.and_then(currency_units) {
            if Self::is_currency(&word) {
                let unit_names = [units.0, units.1];
                let mut pairs: Vec<(i64, &str)> = word
                    .replace(',', "")
                    .split('.')
                    .zip(unit_names)
                    .map(|(n, u)| (n.parse::<i64>().unwrap_or(0), u))
                    .collect();
                if pairs.len() > 1 {
                    if pairs[1].0 == 0 {
                        pairs.truncate(1);
                    } else if pairs[0].0 == 0 {
                        pairs.remove(0);
                    }
                }
                for (i, (n, unit)) in pairs.iter().enumerate() {
                    if i > 0 {
                        push(self.lookup("and", None, None, None), &mut result);
                    }
                    extend_num(&n.to_string(), i == 0, false, &mut result);
                    let r = if n.abs() != 1 && *unit != "pence" {
                        self.stem_s(&format!("{unit}s"), None, None, None)
                    } else {
                        self.lookup(unit, None, None, None)
                    };
                    push(r, &mut result);
                }
            } else {
                return (None, None);
            }
        } else {
            let w = if is_digits(&word) {
                word.parse::<u64>().map(num::cardinal).unwrap_or_default()
            } else if !word.contains('.') {
                let n = word.replace(',', "").parse::<u64>().unwrap_or(0);
                if ordinal_suffix {
                    num::ordinal(n)
                } else {
                    num::cardinal(n)
                }
            } else {
                let cleaned = word.replace(',', "");
                if let Some(rest) = cleaned.strip_prefix('.') {
                    let digits: Vec<String> = rest
                        .chars()
                        .map(|c| {
                            c.to_digit(10)
                                .map(|d| num::cardinal(d as u64))
                                .unwrap_or_default()
                        })
                        .collect();
                    format!("point {}", digits.join(" "))
                } else {
                    num::float_words(&cleaned).unwrap_or_default()
                }
            };
            extend_num(&w, true, true, &mut result);
        }
        if result.is_empty() {
            return (None, None);
        }
        let rating = result.iter().map(|(_, r)| *r).min().unwrap_or(3);
        let joined = result
            .iter()
            .map(|(p, _)| p.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let ps = match suffix.as_deref() {
            Some("s") | Some("'s") => self.suffix_s(Some(joined)),
            Some("ed") | Some("'d") => self.suffix_ed(Some(joined)),
            Some("ing") => self.suffix_ing(Some(joined)),
            _ => Some(joined),
        };
        (ps, Some(rating))
    }

    pub fn append_currency(&self, ps: &str, currency: Option<char>) -> String {
        let units = match currency.and_then(currency_units) {
            Some(u) => u,
            None => return ps.into(),
        };
        match self.stem_s(&format!("{}s", units.0), None, None, None).0 {
            Some(c) => format!("{ps} {c}"),
            None => ps.into(),
        }
    }

    pub fn is_number(word: &str, is_head: bool) -> bool {
        if !word.chars().any(|c| c.is_ascii_digit()) {
            return false;
        }
        let suffixes = ["ing", "'d", "ed", "'s", "st", "nd", "rd", "th", "s"];
        let mut w = word;
        for s in suffixes {
            if let Some(stripped) = w.strip_suffix(s) {
                w = stripped;
                break;
            }
        }
        w.chars().enumerate().all(|(i, c)| {
            c.is_ascii_digit() || c == ',' || c == '.' || (is_head && i == 0 && c == '-')
        })
    }

    pub fn call(&self, q: &WordQuery<'_>, ctx: &Ctx) -> Res {
        let WordQuery {
            text,
            alias,
            tag,
            stress,
            currency,
            is_head,
            num_flags,
        } = *q;
        let word = alias.unwrap_or(text).replace(['‘', '’'], "'");
        let cap_stress = if word == word.to_lowercase() {
            None
        } else if word == word.to_uppercase() {
            Some(self.lex.cap_stresses.1)
        } else {
            Some(self.lex.cap_stresses.0)
        };
        let (ps, rating) = self.get_word(&word, tag, cap_stress, ctx);
        if let Some(p) = ps {
            return (
                Some(apply_stress(&self.append_currency(&p, currency), stress)),
                rating,
            );
        }
        if Self::is_number(&word, is_head) {
            let (ps, rating) = self.get_number(&word, currency, is_head, num_flags);
            return (ps.map(|p| apply_stress(&p, stress)), rating);
        }
        if !word.chars().all(lexicon_ord) {
            return (None, None);
        }
        (None, None)
    }
}

fn double_consonant_ing(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if word.ends_with("cking") {
        return true;
    }
    if chars.len() < 5 {
        return false;
    }
    let a = chars[chars.len() - 5];
    let b = chars[chars.len() - 4];
    a == b && "bcdgklmnprstvxz".contains(a)
}
