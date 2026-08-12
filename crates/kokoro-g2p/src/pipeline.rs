use crate::lexicon::{
    CONSONANTS, Ctx, Lex, NON_QUOTE_PUNCTS, PRIMARY, PUNCTS, SUBTOKEN_JUNKS, VOWELS, apply_stress,
    is_digits, stress_weight,
};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Default)]
pub struct MToken {
    pub text: String,
    pub tag: String,
    pub whitespace: String,
    pub phonemes: Option<String>,
    pub is_head: bool,
    pub alias: Option<String>,
    pub stress: Option<f64>,
    pub currency: Option<char>,
    pub num_flags: String,
    pub prespace: bool,
    pub rating: Option<i32>,
}

const PUNCT_TAGS: [&str; 11] = [
    ".", ",", "-LRB-", "-RRB-", "``", "\"\"", "''", ":", "$", "#", "NFP",
];

fn punct_tag_phoneme(tag: &str) -> Option<&'static str> {
    match tag {
        "-LRB-" => Some("("),
        "-RRB-" => Some(")"),
        "``" => Some("\u{201c}"),
        "\"\"" | "''" => Some("\u{201d}"),
        _ => None,
    }
}

static SUBTOKEN_RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
        r"^['‘’]+|\p{Lu}(?=\p{Lu}\p{Ll})|(?:^-)?(?:\d?[,.]?\d)+|[-_]+|['‘’]{2,}|\p{L}*?(?:['‘’]\p{L})*?\p{Ll}(?=\p{Lu})|\p{L}+(?:['‘’]\p{L})*|[^-_\p{L}'‘’\d]|['‘’]+$",
    )
    .unwrap()
});

pub fn subtokenize(word: &str) -> Vec<String> {
    SUBTOKEN_RE
        .find_iter(word)
        .filter_map(|m| m.ok().map(|m| m.as_str().to_string()))
        .collect()
}

static LINK_RE: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(r"\[([^\]]+)\]\(([^\)]*)\)").unwrap());

#[derive(Debug, Clone)]
pub enum Feat {
    Stress(f64),
    Ipa(String),
    NumFlags(String),
}

pub fn preprocess(text: &str) -> (String, Vec<String>, HashMap<usize, Feat>) {
    let mut result = String::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut features: HashMap<usize, Feat> = HashMap::new();
    let text = text.trim_start();
    let mut last_end = 0;
    for m in LINK_RE.captures_iter(text).flatten() {
        let whole = m.get(0).map(|g| (g.start(), g.end())).unwrap_or((0, 0));
        let seg = &text[last_end..whole.0];
        result.push_str(seg);
        tokens.extend(seg.split_whitespace().map(String::from));
        let f = m.get(2).map(|g| g.as_str()).unwrap_or_default();
        let feat = parse_feature(f);
        if let Some(feat) = feat {
            features.insert(tokens.len(), feat);
        }
        let word = m.get(1).map(|g| g.as_str()).unwrap_or_default();
        result.push_str(word);
        tokens.push(word.into());
        last_end = whole.1;
    }
    if last_end < text.len() {
        let seg = &text[last_end..];
        result.push_str(seg);
        tokens.extend(seg.split_whitespace().map(String::from));
    }
    (result, tokens, features)
}

fn parse_feature(f: &str) -> Option<Feat> {
    let digits_part = match f.strip_prefix(['-', '+']) {
        Some(rest) => rest,
        None => f,
    };
    if is_digits(digits_part) && !digits_part.is_empty() {
        return f.parse::<f64>().ok().map(Feat::Stress);
    }
    match f {
        "0.5" | "+0.5" => return Some(Feat::Stress(0.5)),
        "-0.5" => return Some(Feat::Stress(-0.5)),
        _ => {}
    }
    if f.len() > 1 && f.starts_with('/') && f.ends_with('/') {
        return Some(Feat::Ipa(f[1..].trim_end_matches('/').to_string()));
    }
    if f.len() > 1 && f.starts_with('#') && f.ends_with('#') {
        return Some(Feat::NumFlags(f[1..].trim_end_matches('#').to_string()));
    }
    None
}

fn join_text(tokens: &[MToken]) -> String {
    tokens[..tokens.len() - 1]
        .iter()
        .map(|t| format!("{}{}", t.text, t.whitespace))
        .collect::<String>()
        + &tokens[tokens.len() - 1].text
}

pub fn merge_tokens(tokens: &[MToken], unk: Option<&str>) -> MToken {
    let stresses: Vec<f64> = {
        let mut v: Vec<f64> = tokens.iter().filter_map(|t| t.stress).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v.dedup();
        v
    };
    let currencies: Vec<char> = {
        let mut v: Vec<char> = tokens.iter().filter_map(|t| t.currency).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let ratings: Vec<Option<i32>> = tokens.iter().map(|t| t.rating).collect();
    let phonemes = unk.map(|u| {
        let mut ps = String::new();
        for tk in tokens {
            if tk.prespace
                && !ps.is_empty()
                && !ps.ends_with(char::is_whitespace)
                && tk.phonemes.as_deref().is_some_and(|p| !p.is_empty())
            {
                ps.push(' ');
            }
            match &tk.phonemes {
                Some(p) => ps.push_str(p),
                None => ps.push_str(u),
            }
        }
        ps
    });
    let text = join_text(tokens);
    let tag = tokens
        .iter()
        .max_by_key(|t| {
            t.text
                .chars()
                .map(|c| {
                    if c.to_lowercase().to_string() == c.to_string() {
                        1usize
                    } else {
                        2
                    }
                })
                .sum::<usize>()
        })
        .map(|t| t.tag.clone())
        .unwrap_or_default();
    MToken {
        text,
        tag,
        whitespace: tokens[tokens.len() - 1].whitespace.clone(),
        phonemes,
        is_head: tokens[0].is_head,
        alias: None,
        stress: if stresses.len() == 1 {
            Some(stresses[0])
        } else {
            None
        },
        currency: currencies.last().copied(),
        num_flags: {
            let mut flags: Vec<char> = tokens.iter().flat_map(|t| t.num_flags.chars()).collect();
            flags.sort_unstable();
            flags.dedup();
            flags.into_iter().collect()
        },
        prespace: tokens[0].prespace,
        rating: if ratings.iter().any(|r| r.is_none()) {
            None
        } else {
            ratings.iter().filter_map(|r| *r).min()
        },
    }
}

enum Word {
    Single(MToken),
    Group(Vec<MToken>),
}

fn retokenize(tokens: Vec<MToken>) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut currency: Option<char> = None;
    let token_tags: Vec<String> = tokens.iter().map(|t| t.tag.clone()).collect();
    for (i, token) in tokens.into_iter().enumerate() {
        let mut tks: Vec<MToken> = if token.alias.is_none() && token.phonemes.is_none() {
            subtokenize(&token.text)
                .into_iter()
                .map(|t| MToken {
                    text: t,
                    tag: token.tag.clone(),
                    whitespace: String::new(),
                    phonemes: None,
                    is_head: true,
                    alias: None,
                    stress: token.stress,
                    currency: None,
                    num_flags: token.num_flags.clone(),
                    prespace: false,
                    rating: None,
                })
                .collect()
        } else {
            vec![token.clone()]
        };
        if tks.is_empty() {
            continue;
        }
        let last_idx = tks.len() - 1;
        tks[last_idx].whitespace = token.whitespace.clone();
        let tks_len = tks.len();
        let two_alias: Vec<bool> = (0..tks_len)
            .map(|j| {
                j > 0
                    && j < tks_len - 1
                    && tks[j].text == "2"
                    && tks[j - 1]
                        .text
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphabetic())
                    && tks[j + 1]
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic())
            })
            .collect();
        for (j, tk) in tks.into_iter().enumerate() {
            let mut tk = tk;
            if tk.alias.is_some() || tk.phonemes.is_some() {
            } else if tk.tag == "$" && matches!(tk.text.as_str(), "$" | "£" | "€") {
                currency = tk.text.chars().next();
                tk.phonemes = Some(String::new());
                tk.rating = Some(4);
            } else if tk.tag == ":" && (tk.text == "-" || tk.text == "–") {
                tk.phonemes = Some("—".into());
                tk.rating = Some(3);
            } else if PUNCT_TAGS.contains(&tk.tag.as_str())
                && !tk
                    .text
                    .chars()
                    .all(|c| c.to_lowercase().all(|lc| lc.is_ascii_lowercase()))
            {
                tk.phonemes = Some(
                    punct_tag_phoneme(&tk.tag)
                        .map(String::from)
                        .unwrap_or_else(|| {
                            tk.text.chars().filter(|c| PUNCTS.contains(*c)).collect()
                        }),
                );
                tk.rating = Some(4);
            } else if let Some(cur) = currency {
                if tk.tag != "CD" {
                    currency = None;
                } else if j + 1 == tks_len
                    && token_tags.get(i + 1).map(|t| t.as_str()) != Some("CD")
                {
                    tk.currency = Some(cur);
                }
            } else if two_alias[j] {
                tk.alias = Some("to".into());
            }
            let done = tk.alias.is_some() || tk.phonemes.is_some();
            if done {
                words.push(Word::Single(tk));
            } else {
                let join = match words.last() {
                    Some(Word::Group(g)) => g.last().is_some_and(|l| l.whitespace.is_empty()),
                    _ => false,
                };
                if join {
                    tk.is_head = false;
                    if let Some(Word::Group(g)) = words.last_mut() {
                        g.push(tk)
                    }
                } else if tk.whitespace.is_empty() {
                    words.push(Word::Group(vec![tk]));
                } else {
                    words.push(Word::Single(tk));
                }
            }
        }
    }
    words
        .into_iter()
        .map(|w| match w {
            Word::Group(mut g) if g.len() == 1 => Word::Single(g.remove(0)),
            other => other,
        })
        .collect()
}

fn token_context(ctx: Ctx, ps: Option<&str>, text: &str, tag: &str) -> Ctx {
    let mut vowel = ctx.future_vowel;
    if let Some(ps) = ps {
        for c in ps.chars() {
            if VOWELS.contains(c) || CONSONANTS.contains(c) || NON_QUOTE_PUNCTS.contains(c) {
                vowel = if NON_QUOTE_PUNCTS.contains(c) {
                    None
                } else {
                    Some(VOWELS.contains(c))
                };
                break;
            }
        }
    }
    let future_to = text == "to" || text == "To" || (text == "TO" && (tag == "TO" || tag == "IN"));
    Ctx {
        future_vowel: vowel,
        future_to,
    }
}

fn resolve_tokens(tokens: &mut [MToken]) {
    let text = join_text(tokens);
    let classes: std::collections::HashSet<u8> = text
        .chars()
        .filter(|c| !SUBTOKEN_JUNKS.contains(*c))
        .map(|c| {
            if c.is_alphabetic() {
                0
            } else if c.is_ascii_digit() {
                1
            } else {
                2
            }
        })
        .collect();
    let prespace = text.contains(' ') || text.contains('/') || classes.len() > 1;
    let n = tokens.len();
    for (i, tk) in tokens.iter_mut().enumerate() {
        if tk.phonemes.is_none() {
            if i == n - 1
                && tk.text.chars().count() == 1
                && NON_QUOTE_PUNCTS.contains(tk.text.chars().next().unwrap_or(' '))
            {
                tk.phonemes = Some(tk.text.clone());
                tk.rating = Some(3);
            } else if tk.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c)) {
                tk.phonemes = Some(String::new());
                tk.rating = Some(3);
            }
        } else if i > 0 {
            tk.prespace = prespace;
        }
    }
    if prespace {
        return;
    }
    let indices: Vec<(bool, usize, usize)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, tk)| tk.phonemes.as_deref().is_some_and(|p| !p.is_empty()))
        .map(|(i, tk)| {
            let p = tk.phonemes.as_deref().unwrap_or_default();
            (p.contains(PRIMARY), stress_weight(p), i)
        })
        .collect();
    if indices.len() == 2 && tokens[indices[0].2].text.chars().count() == 1 {
        let i = indices[1].2;
        if let Some(p) = tokens[i].phonemes.clone() {
            tokens[i].phonemes = Some(apply_stress(&p, Some(-0.5)));
        }
        return;
    }
    if indices.len() < 2
        || indices.iter().filter(|(b, _, _)| *b).count() <= indices.len().div_ceil(2)
    {
        return;
    }
    let mut sorted = indices.clone();
    sorted.sort();
    for (_, _, i) in sorted.into_iter().take(indices.len() / 2) {
        if let Some(p) = tokens[i].phonemes.clone() {
            tokens[i].phonemes = Some(apply_stress(&p, Some(-0.5)));
        }
    }
}

pub struct EnG2p<'a> {
    pub lex: Lex<'a>,
    pub unk: String,
    pub use_fallback: bool,
}

impl<'a> EnG2p<'a> {
    fn lexicon_call(&self, tk: &MToken, ctx: &Ctx) -> (Option<String>, Option<i32>) {
        self.lex.call(
            &crate::lexicon::WordQuery {
                text: &tk.text,
                alias: tk.alias.as_deref(),
                tag: &tk.tag,
                stress: tk.stress,
                currency: tk.currency,
                is_head: tk.is_head,
                num_flags: &tk.num_flags,
            },
            ctx,
        )
    }

    pub fn run(&self, text: &str) -> String {
        let (text, _spl_tokens, features) = preprocess(text);
        let mut stoks = crate::tokenize::tokenize(&text);
        let golds = &self.lex.lex.golds;
        let in_gold = |w: &str| golds.contains_key(w);
        crate::tokenize::tag_tokens(&mut stoks, &self.lex.lex.pos_words, &in_gold);
        let mut tokens: Vec<MToken> = Vec::new();
        let mut chunk_first: HashMap<usize, usize> = HashMap::new();
        for st in &stoks {
            let idx_in_chunk = {
                let e = chunk_first.entry(st.chunk).or_insert(0);
                let v = *e;
                *e += 1;
                v
            };
            let mut tk = MToken {
                text: st.text.clone(),
                tag: st.tag.clone(),
                whitespace: st.ws.clone(),
                is_head: true,
                ..Default::default()
            };
            match features.get(&st.chunk) {
                Some(Feat::Stress(v)) => tk.stress = Some(*v),
                Some(Feat::Ipa(p)) => {
                    tk.is_head = idx_in_chunk == 0;
                    tk.phonemes = Some(if idx_in_chunk == 0 {
                        p.clone()
                    } else {
                        String::new()
                    });
                    tk.rating = Some(5);
                }
                Some(Feat::NumFlags(f)) => tk.num_flags = f.clone(),
                None => {}
            }
            tokens.push(tk);
        }
        let folded: Vec<MToken> = {
            let mut result: Vec<MToken> = Vec::new();
            for tk in tokens {
                if !tk.is_head && !result.is_empty() {
                    let prev = result.pop().unwrap_or_default();
                    result.push(merge_tokens(&[prev, tk], Some(&self.unk)));
                } else {
                    result.push(tk);
                }
            }
            result
        };
        let mut words = retokenize(folded);
        let mut ctx = Ctx::default();
        for wi in (0..words.len()).rev() {
            match &mut words[wi] {
                Word::Single(w) => {
                    if w.phonemes.is_none() {
                        let (ps, rating) = self.lexicon_call(w, &ctx);
                        w.phonemes = ps;
                        w.rating = rating;
                    }
                    if w.phonemes.is_none() && self.use_fallback {
                        let (ps, rating) = crate::fallback::letter_to_sound(&w.text);
                        w.phonemes = ps;
                        w.rating = rating;
                    }
                    ctx = token_context(ctx, w.phonemes.as_deref(), &w.text, &w.tag);
                }
                Word::Group(w) => {
                    let (mut left, mut right) = (0usize, w.len());
                    let mut should_fallback = false;
                    while left < right {
                        let blocked = w[left..right]
                            .iter()
                            .any(|tk| tk.alias.is_some() || tk.phonemes.is_some());
                        let merged = if blocked {
                            None
                        } else {
                            Some(merge_tokens(&w[left..right], None))
                        };
                        let (ps, rating) = match &merged {
                            Some(tk) => self.lexicon_call(tk, &ctx),
                            None => (None, None),
                        };
                        if let Some(ps) = ps {
                            w[left].phonemes = Some(ps.clone());
                            w[left].rating = rating;
                            for x in w[left + 1..right].iter_mut() {
                                x.phonemes = Some(String::new());
                                x.rating = rating;
                            }
                            if let Some(m) = merged {
                                ctx = token_context(ctx, Some(&ps), &m.text, &m.tag);
                            }
                            right = left;
                            left = 0;
                        } else if left + 1 < right {
                            left += 1;
                        } else {
                            right -= 1;
                            let tk = &mut w[right];
                            if tk.phonemes.is_none() {
                                if tk.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c)) {
                                    tk.phonemes = Some(String::new());
                                    tk.rating = Some(3);
                                } else if self.use_fallback {
                                    should_fallback = true;
                                    break;
                                }
                            }
                            left = 0;
                        }
                    }
                    if should_fallback {
                        let merged = merge_tokens(w, None);
                        let (ps, rating) = crate::fallback::letter_to_sound(&merged.text);
                        w[0].phonemes = ps;
                        w[0].rating = rating;
                        for x in w[1..].iter_mut() {
                            x.phonemes = Some(String::new());
                            x.rating = rating;
                        }
                    } else {
                        resolve_tokens(w);
                    }
                }
            }
        }
        let final_tokens: Vec<MToken> = words
            .into_iter()
            .map(|w| match w {
                Word::Single(t) => t,
                Word::Group(g) => merge_tokens(&g, Some(&self.unk)),
            })
            .collect();
        let mut result = String::new();
        for tk in &final_tokens {
            let ps = match &tk.phonemes {
                Some(p) => p.replace('ɾ', "T").replace('ʔ', "t"),
                None => self.unk.clone(),
            };
            result.push_str(&ps);
            result.push_str(&tk.whitespace);
        }
        result
    }
}
