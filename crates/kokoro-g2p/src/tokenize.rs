use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub struct STok {
    pub text: String,
    pub tag: String,
    pub ws: String,
    pub chunk: usize,
}

static EXCEPTIONS: LazyLock<HashMap<String, Vec<String>>> = LazyLock::new(|| {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    let whole = [
        "'cause", "o'clock", "lemme", "a.m.", "p.m.", "e.g.", "i.e.", "vs.", "Jan.", "Feb.",
        "Mar.", "Apr.", "Jun.", "Jul.", "Aug.", "Sep.", "Sept.", "Oct.", "Nov.", "Dec.", "Dr.",
        "Mr.", "Mrs.", "Ms.", "Prof.", "Rev.", "Gov.", "St.", "Mt.", "Corp.", "Inc.", "Ltd.",
        "Jr.", "Ph.D.", "'d", "'s",
    ];
    for w in whole {
        m.insert(w.into(), vec![w.into()]);
    }
    let nt = [
        ("don't", "do"),
        ("doesn't", "does"),
        ("didn't", "did"),
        ("isn't", "is"),
        ("aren't", "are"),
        ("wasn't", "was"),
        ("weren't", "were"),
        ("hasn't", "has"),
        ("haven't", "have"),
        ("hadn't", "had"),
        ("won't", "wo"),
        ("wouldn't", "would"),
        ("couldn't", "could"),
        ("shouldn't", "should"),
        ("can't", "ca"),
        ("mustn't", "must"),
        ("needn't", "need"),
        ("shan't", "sha"),
        ("ain't", "ai"),
    ];
    let mut pairs: Vec<(String, Vec<String>)> = Vec::new();
    for (full, base) in nt {
        pairs.push((full.into(), vec![base.into(), "n't".into()]));
        pairs.push((
            format!("{full}'ve"),
            vec![base.into(), "n't".into(), "'ve".into()],
        ));
    }
    let pron_suff = [
        ("I", &["'m", "'ve", "'ll", "'d"][..]),
        ("you", &["'re", "'ve", "'ll", "'d"]),
        ("we", &["'re", "'ve", "'ll", "'d"]),
        ("they", &["'re", "'ve", "'ll", "'d"]),
        ("he", &["'ll", "'d"]),
        ("she", &["'ll", "'d"]),
        ("it", &["'ll", "'d"]),
        ("that", &["'ll", "'d"]),
        ("there", &["'ll", "'d", "'re"]),
        ("who", &["'ll", "'d", "'ve", "'re"]),
        ("what", &["'re", "'ll", "'ve"]),
        ("could", &["'ve"]),
        ("would", &["'ve"]),
        ("should", &["'ve"]),
        ("might", &["'ve"]),
        ("must", &["'ve"]),
    ];
    for (base, suffs) in pron_suff {
        for s in suffs {
            pairs.push((format!("{base}{s}"), vec![base.to_string(), s.to_string()]));
        }
    }
    pairs.push(("cannot".into(), vec!["can".into(), "not".into()]));
    pairs.push(("gonna".into(), vec!["gon".into(), "na".into()]));
    pairs.push(("gotta".into(), vec!["got".into(), "ta".into()]));
    pairs.push(("let's".into(), vec!["let".into(), "'s".into()]));
    pairs.push(("y'all".into(), vec!["y'".into(), "all".into()]));
    pairs.push(("Wed.".into(), vec!["We".into(), "d.".into()]));
    for (k, v) in pairs {
        let first = k.chars().next().unwrap_or(' ');
        if first.is_lowercase() {
            let cap: String = first.to_uppercase().collect::<String>() + &k[first.len_utf8()..];
            let mut cv = v.clone();
            if let Some(f) = cv.first_mut() {
                let c0 = f.chars().next().unwrap_or(' ');
                *f = c0.to_uppercase().collect::<String>() + &f[c0.len_utf8()..];
            }
            m.insert(cap, cv);
        }
        m.insert(k, v);
    }
    m
});

const PREFIX_CHARS: &str = "([{«“‘\"'$£€¥#§";
const SUFFIX_CHARS: &str = ")]}»”’\"',;!?:%°…";

fn url_match(s: &str) -> bool {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        fancy_regex::Regex::new(
            r#"^(?:[A-Za-z][\w+\-.]{1,}://)?(?:\S+(?::\S*)?@)?(?:[A-Za-z0-9][A-Za-z0-9_-]*\.)+[a-z]{2,}(?::\d{2,5})?(?:[/?#]\S*)?$"#,
        )
        .unwrap()
    });
    RE.is_match(s).unwrap_or(false)
}

fn find_prefix(s: &str) -> Option<usize> {
    let c = s.chars().next()?;
    if PREFIX_CHARS.contains(c) && s.chars().count() > 1 {
        Some(c.len_utf8())
    } else {
        None
    }
}

fn find_suffix(s: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let last = chars[chars.len() - 1];
    for suf in ["'s", "'S", "’s", "’S"] {
        if s.ends_with(suf) && chars.len() >= 3 && chars[chars.len() - 3].is_alphanumeric() {
            return Some(s.len() - suf.len());
        }
    }
    if last == '.' {
        let dots = chars.iter().rev().take_while(|c| **c == '.').count();
        if dots >= 2 && dots < chars.len() {
            return Some(s.len() - dots);
        }
        if dots == chars.len() {
            return None;
        }
        let prev = chars[chars.len() - 2];
        let peel = prev.is_ascii_digit()
            || prev.is_lowercase()
            || "%²-+)]}»”’\"'".contains(prev)
            || (prev.is_uppercase() && chars.len() >= 3 && chars[chars.len() - 3].is_uppercase());
        if peel {
            return Some(s.len() - 1);
        }
        return None;
    }
    if SUFFIX_CHARS.contains(last) {
        return Some(s.len() - last.len_utf8());
    }
    None
}

fn infix_split(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        let is_hyph = matches!(c, '-' | '–' | '—');
        if is_hyph && i + 1 < chars.len() && matches!(chars[i + 1], '-' | '–' | '—') {
            let start = i;
            while i < chars.len() && matches!(chars[i], '-' | '–' | '—') {
                i += 1;
            }
            if start > 0 && i < chars.len() {
                cuts.push((start, i));
            }
            continue;
        }
        if c == '.' && i + 1 < chars.len() && chars[i + 1] == '.' {
            let start = i;
            while i < chars.len() && chars[i] == '.' {
                i += 1;
            }
            if start > 0 && i < chars.len() {
                cuts.push((start, i));
            }
            continue;
        }
        let split_here = match c {
            '-' | '–' | '—' => {
                prev.is_some_and(|p| p.is_alphanumeric()) && next.is_some_and(|n| n.is_alphabetic())
                    || prev.is_some_and(|p| p.is_ascii_digit())
                        && next.is_some_and(|n| n.is_ascii_digit())
            }
            ':' | '<' | '>' | '=' | '/' => {
                prev.is_some_and(|p| p.is_alphanumeric()) && next.is_some_and(|n| n.is_alphabetic())
            }
            '…' => prev.is_some() && next.is_some(),
            '.' => prev.is_some_and(|p| p.is_lowercase()) && next.is_some_and(|n| n.is_uppercase()),
            ',' => {
                prev.is_some_and(|p| p.is_alphabetic()) && next.is_some_and(|n| n.is_alphabetic())
            }
            _ => false,
        };
        if split_here {
            cuts.push((i, i + 1));
        }
        i += 1;
    }
    if cuts.is_empty() {
        return vec![s.into()];
    }
    let mut out = Vec::new();
    let mut pos = 0;
    for (a, b) in cuts {
        if a > pos {
            out.push(chars[pos..a].iter().collect());
        }
        out.push(chars[a..b].iter().collect());
        pos = b;
    }
    if pos < chars.len() {
        out.push(chars[pos..].iter().collect());
    }
    out
}

fn segment(chunk: &str) -> Vec<String> {
    let mut front: Vec<String> = Vec::new();
    let mut back: Vec<String> = Vec::new();
    let mut s = chunk.to_string();
    loop {
        if s.is_empty() {
            break;
        }
        if let Some(parts) = EXCEPTIONS.get(&s) {
            front.extend(parts.clone());
            break;
        }
        if let Some(n) = find_prefix(&s) {
            front.push(s[..n].into());
            s = s[n..].into();
            continue;
        }
        if let Some(n) = find_suffix(&s) {
            back.push(s[n..].into());
            s = s[..n].into();
            continue;
        }
        if url_match(&s) {
            front.push(s);
            break;
        }
        let parts = infix_split(&s);
        if parts.len() == 1 {
            front.push(s);
        } else {
            for p in parts {
                if let Some(ex) = EXCEPTIONS.get(&p) {
                    front.extend(ex.clone());
                } else {
                    front.push(p);
                }
            }
        }
        break;
    }
    front.extend(back.into_iter().rev());
    front.retain(|t| !t.is_empty());
    front
}

pub fn tokenize(text: &str) -> Vec<STok> {
    let mut toks: Vec<STok> = Vec::new();
    let chunks: Vec<&str> = text.split_whitespace().collect();
    for (ci, chunk) in chunks.iter().enumerate() {
        let parts = segment(chunk);
        let n = parts.len();
        for (i, p) in parts.into_iter().enumerate() {
            let ws = if i + 1 == n && ci + 1 < chunks.len() {
                " "
            } else {
                ""
            };
            toks.push(STok {
                text: p,
                tag: String::new(),
                ws: ws.into(),
                chunk: ci,
            });
        }
    }
    toks
}

fn base_tag(text: &str, quote_open: &mut bool) -> Option<String> {
    let single: Vec<char> = text.chars().collect();
    if single.len() == 1 {
        let c = single[0];
        let t = match c {
            '.' | '!' | '?' => Some("."),
            ',' => Some(","),
            ';' | ':' | '…' => Some(":"),
            '-' | '–' | '—' => Some("HYPH"),
            '(' | '[' | '{' => Some("-LRB-"),
            ')' | ']' | '}' => Some("-RRB-"),
            '“' | '«' | '‘' => Some("``"),
            '”' | '»' | '’' | '\'' => Some("''"),
            '"' => {
                let t = if *quote_open { "''" } else { "``" };
                *quote_open = !*quote_open;
                Some(t)
            }
            '$' | '£' | '€' | '¥' | '¢' => Some("$"),
            '%' => Some("NN"),
            '&' => Some("CC"),
            '/' | '\\' | '=' | '<' | '>' | '+' | '*' | '^' | '~' | '|' => Some("SYM"),
            '#' => Some("$"),
            _ => None,
        };
        if let Some(t) = t {
            return Some(t.into());
        }
    }
    if text.chars().all(|c| ".!?".contains(c)) {
        return Some(if text.starts_with('.') && text.len() > 1 {
            ":".into()
        } else {
            ".".into()
        });
    }
    if text.chars().all(|c| matches!(c, '-' | '–' | '—')) {
        return Some(":".into());
    }
    None
}

fn word_tag(w: &str) -> Option<&'static str> {
    Some(match w {
        "the" | "a" | "an" | "this" | "these" | "those" | "no" | "each" | "every" | "either"
        | "neither" | "some" | "any" | "another" => "DT",
        "vs" | "vs." => "IN",
        "same" | "whole" | "only" | "own" | "other" | "old" => "JJ",
        "and" | "or" | "but" | "nor" | "yet" => "CC",
        "to" => "TO",
        "of" | "in" | "on" | "at" | "for" | "from" | "with" | "without" | "by" | "about"
        | "against" | "between" | "into" | "through" | "during" | "before" | "after" | "above"
        | "below" | "under" | "over" | "around" | "near" | "upon" | "since" | "until" | "while"
        | "per" | "via" | "onto" | "off" | "out" | "up" | "down" | "like" | "than" | "as"
        | "if" | "because" | "unless" | "although" | "though" | "whether" => "IN",
        "i" | "I" | "you" | "he" | "she" | "it" | "we" | "they" | "me" | "him" | "us" | "them"
        | "himself" | "herself" | "itself" | "themselves" | "myself" | "yourself" => "PRP",
        "my" | "your" | "his" | "its" | "our" | "their" | "her" => "PRP$",
        "is" | "does" | "has" => "VBZ",
        "was" | "did" | "were" => "VBD",
        "are" | "do" | "have" | "am" => "VBP",
        "had" => "VBD",
        "be" => "VB",
        "been" => "VBN",
        "being" => "VBG",
        "will" | "would" | "can" | "could" | "shall" | "should" | "may" | "might" | "must" => "MD",
        "not" | "n't" | "never" | "also" | "just" | "still" | "again" | "too" | "very" | "so"
        | "quite" | "rather" => "RB",
        "who" | "whom" => "WP",
        "which" | "whatever" => "WDT",
        "when" | "where" | "why" | "how" => "WRB",
        "there" => "EX",
        "'s" => "POS",
        "'re" | "'ve" => "VBP",
        "'ll" => "MD",
        "'m" => "VBP",
        "'d" => "MD",
        _ => return None,
    })
}

fn is_number_like(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ",.:/-+".contains(c) || c.is_ascii_lowercase())
}

const SCALE_WORDS: [&str; 6] = [
    "hundred", "thousand", "million", "billion", "trillion", "dozen",
];
const ABBREV_DOT: [&str; 8] = ["No", "Vol", "Sr", "Mon", "min", "max", "etc", "Ave"];

fn bare_domain(text: &str) -> bool {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        fancy_regex::Regex::new(
            r"^(?:https?://)?[A-Za-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:com|org|net|edu|gov|io)$",
        )
        .unwrap()
    });
    RE.is_match(text).unwrap_or(false)
}

pub fn tag_tokens(toks: &mut [STok], pos_words: &HashSet<String>, in_gold: &dyn Fn(&str) -> bool) {
    let mut quote_open = false;
    let n = toks.len();
    for i in 0..n {
        let text = toks[i].text.clone();
        if text == "." && i > 0 && ABBREV_DOT.contains(&toks[i - 1].text.as_str()) {
            toks[i].tag = "NNP".into();
            continue;
        }
        if text.len() > 1
            && ((text.starts_with('.') && text.chars().any(|c| c.is_alphabetic()))
                || (text.starts_with('/') && text.chars().any(|c| c.is_ascii_digit())))
        {
            toks[i].tag = ".".into();
            continue;
        }
        if let Some(t) = base_tag(&text, &mut quote_open) {
            toks[i].tag = t;
            continue;
        }
        if is_number_like(&text) {
            toks[i].tag = "CD".into();
            continue;
        }
        let lower = text.to_lowercase();
        if SCALE_WORDS.contains(&lower.as_str()) {
            let prev_cd = i > 0 && (toks[i - 1].tag == "CD" || toks[i - 1].tag == "$");
            let next_cd = toks.get(i + 1).is_some_and(|t| is_number_like(&t.text));
            if prev_cd || next_cd {
                toks[i].tag = "CD".into();
                continue;
            }
        }
        if bare_domain(&text) {
            toks[i].tag = "ADD".into();
            continue;
        }
        if let Some(t) = word_tag(&text) {
            toks[i].tag = t.into();
            continue;
        }
        let alpha = text
            .chars()
            .all(|c| c.is_alphabetic() || c == '\'' || c == '.');
        let has_alpha = text.chars().any(|c| c.is_alphabetic());
        if alpha
            && has_alpha
            && text == text.to_uppercase()
            && text.chars().filter(|c| c.is_alphabetic()).count() >= 2
        {
            toks[i].tag = if sentence_initial_at(toks, i) && in_gold(&lower) {
                "VBD".into()
            } else {
                "NNP".into()
            };
            continue;
        }
        if let Some(t) = word_tag(&lower) {
            toks[i].tag = t.into();
            continue;
        }
        toks[i].tag = String::new();
    }
    for i in 0..n {
        if !toks[i].tag.is_empty() {
            continue;
        }
        let text = toks[i].text.clone();
        let lower = text.to_lowercase();
        let sentence_initial = sentence_initial_at(toks, i);
        if lower == "that" {
            toks[i].tag = that_tag(toks, i, sentence_initial);
            continue;
        }
        if pos_words.contains(&text) || pos_words.contains(&lower) || lower == "used" {
            toks[i].tag = het_tag(toks, i, sentence_initial);
            continue;
        }
        let capped = text.chars().next().is_some_and(|c| c.is_uppercase());
        toks[i].tag = if capped && !sentence_initial {
            "NNP".into()
        } else if lower.ends_with("ly") {
            "RB".into()
        } else if lower.ends_with("ing") {
            "VBG".into()
        } else if lower.ends_with('s') {
            "NNS".into()
        } else {
            "NN".into()
        };
    }
}

fn sentence_initial_at(toks: &[STok], i: usize) -> bool {
    for j in (0..i).rev() {
        match toks[j].tag.as_str() {
            "``" | "''" | "-LRB-" => continue,
            "." | ":" => return true,
            _ => return false,
        }
    }
    true
}

fn that_tag(toks: &[STok], i: usize, sentence_initial: bool) -> String {
    let next = toks.get(i + 1);
    let next_text = next.map(|t| t.text.to_lowercase()).unwrap_or_default();
    let next_punct = next.is_none_or(|t| t.text.chars().all(|c| !c.is_alphanumeric()));
    let noun_next = matches!(
        next_text.as_str(),
        "morning"
            | "afternoon"
            | "evening"
            | "night"
            | "day"
            | "week"
            | "month"
            | "year"
            | "time"
            | "way"
            | "point"
            | "moment"
            | "one"
    );
    if sentence_initial || next_punct || noun_next {
        "DT".into()
    } else {
        "IN".into()
    }
}

fn het_tag(toks: &[STok], i: usize, sentence_initial: bool) -> String {
    let text = &toks[i].text;
    let lower = text.to_lowercase();
    let ends_s = lower.ends_with('s') && !lower.ends_with("ss");
    let prev = if i > 0 { Some(&toks[i - 1]) } else { None };
    let prev_text = prev.map(|t| t.text.to_lowercase()).unwrap_or_default();
    let prev_tag = prev.map(|t| t.tag.clone()).unwrap_or_default();
    let next = toks.get(i + 1);
    let next_text = next.map(|t| t.text.to_lowercase()).unwrap_or_default();
    let verb_trigger = matches!(
        prev_text.as_str(),
        "to" | "not"
            | "n't"
            | "please"
            | "then"
            | "will"
            | "would"
            | "can"
            | "could"
            | "shall"
            | "should"
            | "may"
            | "might"
            | "must"
            | "do"
            | "does"
            | "did"
    );
    let be_verb = matches!(
        prev_text.as_str(),
        "is" | "are" | "was" | "were" | "be" | "been" | "being" | "am"
    );
    let have_verb = matches!(prev_text.as_str(), "has" | "have" | "had" | "having");
    let degree = matches!(
        prev_text.as_str(),
        "too" | "very" | "so" | "quite" | "still" | "rather" | "both"
    );
    if verb_trigger {
        return "VB".into();
    }
    if have_verb {
        return "VBN".into();
    }
    if be_verb {
        return if matches!(lower.as_str(), "wound" | "read" | "used") {
            "VBN".into()
        } else {
            "JJ".into()
        };
    }
    if degree {
        return "JJ".into();
    }
    if prev_text == lower {
        return if matches!(prev_tag.as_str(), "VB" | "VBP" | "VBZ" | "VBD") {
            if ends_s { "NNS".into() } else { "NN".into() }
        } else {
            "VBD".into()
        };
    }
    if matches!(prev_tag.as_str(), "DT" | "PRP$" | "CD")
        || matches!(prev_text.as_str(), "more" | "most")
    {
        return if ends_s { "NNS".into() } else { "NN".into() };
    }
    if prev_tag == "JJ" {
        return if ends_s { "NNS".into() } else { "NN".into() };
    }
    {
        let mut j = i;
        let mut steps = 0;
        while j > 0 && steps < 3 && matches!(toks[j - 1].tag.as_str(), "NN" | "NNP") {
            j -= 1;
            steps += 1;
        }
        if steps > 0 && j > 0 && matches!(toks[j - 1].tag.as_str(), "DT" | "PRP$") {
            return if ends_s { "VBZ".into() } else { "NN".into() };
        }
    }
    if next_text == "to" {
        return "JJ".into();
    }
    if prev_tag == "PRP" || matches!(prev_text.as_str(), "who" | "that" | "which") {
        return if ends_s { "VBZ".into() } else { "VBD".into() };
    }
    if prev_tag == "NNS" {
        return if ends_s { "VBZ".into() } else { "VBP".into() };
    }
    if matches!(prev_text.as_str(), "and" | "or")
        && next.is_some_and(|t| {
            matches!(t.tag.as_str(), "DT" | "PRP$")
                || matches!(t.text.to_lowercase().as_str(), "the" | "a" | "an")
        })
    {
        return "VB".into();
    }
    if sentence_initial {
        let next_dt = next.is_some_and(|t| {
            matches!(
                t.text.to_lowercase().as_str(),
                "the" | "a" | "an" | "your" | "his" | "her" | "their" | "my" | "our" | "its"
            )
        });
        let next_rb = next_text.ends_with("ly");
        let next_obj = next
            .is_some_and(|t| t.tag.is_empty() && t.text.chars().all(|c| c.is_ascii_lowercase()));
        if next_dt || next_rb || next_obj {
            return "VB".into();
        }
    }
    if ends_s { "NNS".into() } else { "NN".into() }
}
