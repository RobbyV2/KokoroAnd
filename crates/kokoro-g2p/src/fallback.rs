use crate::lexicon::{PRIMARY, VOWELS, apply_stress};

const RULES: [(&str, &str); 52] = [
    ("tion", "ʃən"),
    ("sion", "ʒən"),
    ("ture", "ʧəɹ"),
    ("ough", "ˈO"),
    ("augh", "ˈæf"),
    ("igh", "ˈI"),
    ("tch", "ʧ"),
    ("dge", "ʤ"),
    ("sch", "sk"),
    ("ph", "f"),
    ("gh", "ɡ"),
    ("ch", "ʧ"),
    ("sh", "ʃ"),
    ("th", "θ"),
    ("wh", "w"),
    ("ck", "k"),
    ("ng", "ŋ"),
    ("qu", "kw"),
    ("wr", "ɹ"),
    ("kn", "n"),
    ("ps", "s"),
    ("oo", "u"),
    ("ee", "i"),
    ("ea", "i"),
    ("ai", "A"),
    ("ay", "A"),
    ("oa", "O"),
    ("ow", "W"),
    ("ou", "W"),
    ("oi", "Y"),
    ("oy", "Y"),
    ("au", "ɔ"),
    ("aw", "ɔ"),
    ("ew", "u"),
    ("ue", "u"),
    ("ie", "i"),
    ("a", "æ"),
    ("b", "b"),
    ("c", "k"),
    ("d", "d"),
    ("e", "ɛ"),
    ("f", "f"),
    ("g", "ɡ"),
    ("h", "h"),
    ("i", "ɪ"),
    ("j", "ʤ"),
    ("k", "k"),
    ("l", "l"),
    ("m", "m"),
    ("n", "n"),
    ("o", "ɑ"),
    ("p", "p"),
];

fn letter(c: char) -> Option<&'static str> {
    Some(match c {
        'q' => "k",
        'r' => "ɹ",
        's' => "s",
        't' => "t",
        'u' => "ʌ",
        'v' => "v",
        'w' => "w",
        'x' => "ks",
        'y' => "j",
        'z' => "z",
        _ => return None,
    })
}

pub fn letter_to_sound(word: &str) -> (Option<String>, Option<i32>) {
    let w: String = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || *c == '\'')
        .collect();
    if w.is_empty() {
        return (None, None);
    }
    let mut out = String::new();
    let mut rest = w.as_str();
    'outer: while !rest.is_empty() {
        if rest.starts_with('\'') {
            rest = &rest[1..];
            continue;
        }
        if rest.starts_with('e') && rest.len() == 1 && out.chars().any(|c| VOWELS.contains(c)) {
            break;
        }
        for (pat, ph) in RULES {
            if rest.starts_with(pat) {
                out.push_str(ph);
                rest = &rest[pat.len()..];
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap_or('a');
        if let Some(ph) = letter(c) {
            out.push_str(ph);
        }
        rest = &rest[c.len_utf8()..];
    }
    if out.is_empty() {
        return (None, None);
    }
    let stressed = if out.contains(PRIMARY) {
        out
    } else {
        apply_stress(&out, Some(2.0))
    };
    (Some(stressed), Some(1))
}
