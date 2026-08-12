use crate::num;
use fancy_regex::{Captures, Regex};
use std::sync::LazyLock;

const VALID_TLDS: [&str; 43] = [
    "com",
    "org",
    "net",
    "edu",
    "gov",
    "mil",
    "int",
    "biz",
    "info",
    "name",
    "pro",
    "coop",
    "museum",
    "travel",
    "jobs",
    "mobi",
    "tel",
    "asia",
    "cat",
    "xxx",
    "aero",
    "arpa",
    "bg",
    "br",
    "ca",
    "cn",
    "de",
    "es",
    "eu",
    "fr",
    "in",
    "it",
    "jp",
    "mx",
    "nl",
    "ru",
    "uk",
    "us",
    "io",
    "co",
    "localhost",
    "rs",
    "onnx",
];

fn unit_word(u: &str) -> Option<&'static str> {
    Some(match u.to_lowercase().as_str() {
        "m" => "meter",
        "cm" => "centimeter",
        "mm" => "millimeter",
        "km" => "kilometer",
        "ft" => "foot",
        "yd" => "yard",
        "mi" => "mile",
        "g" => "gram",
        "kg" => "kilogram",
        "mg" => "milligram",
        "ms" => "millisecond",
        "min" => "minutes",
        "h" => "hour",
        "l" => "liter",
        "ml" => "milliliter",
        "kph" => "kilometer per hour",
        "mph" => "mile per hour",
        "hz" => "hertz",
        "khz" => "kilohertz",
        "mhz" => "megahertz",
        "ghz" => "gigahertz",
        "v" => "volt",
        "kv" => "kilovolt",
        "w" => "watt",
        "kw" => "kilowatt",
        "mw" => "megawatt",
        "lb" => "pound",
        "lbs" => "pounds",
        "oz" => "ounce",
        "kb" => "kilobit",
        "mb" => "megabit",
        "gb" => "gigabit",
        "tb" => "terabit",
        "kbps" => "kilobit per second",
        "mbps" => "megabit per second",
        "px" => "pixel",
        _ => return None,
    })
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("regex {p}: {e}"))
}

fn cardinal_str(s: &str) -> String {
    s.parse::<u64>()
        .map(num::cardinal)
        .unwrap_or_else(|_| s.into())
}

fn number_words(int_part: &str, frac_part: Option<&str>) -> String {
    match frac_part {
        None | Some("") => cardinal_str(int_part),
        Some(f) => {
            let digits: Vec<String> = f
                .chars()
                .map(|c| {
                    c.to_digit(10)
                        .map(|d| num::cardinal(d as u64))
                        .unwrap_or_default()
                })
                .collect();
            format!("{} point {}", cardinal_str(int_part), digits.join(" "))
        }
    }
}

fn multiplier_word(m: &str) -> String {
    match m.trim().to_lowercase().as_str() {
        "k" => "thousand".into(),
        "m" => "million".into(),
        "b" => "billion".into(),
        "t" => "trillion".into(),
        other => other.into(),
    }
}

fn handle_money(caps: &Captures) -> String {
    let neg = caps.get(1).map(|m| m.as_str()).unwrap_or_default() == "-";
    let sym = caps.get(2).map(|m| m.as_str()).unwrap_or("$");
    let (bill, coin) = match sym {
        "£" => ("pound", "pence"),
        "€" => ("euro", "cent"),
        _ => ("dollar", "cent"),
    };
    let number = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
    let multiplier = multiplier_word(caps.get(4).map(|m| m.as_str()).unwrap_or_default());
    let prefix = if neg { "minus " } else { "" };
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (number, None),
    };
    let is_int = frac_part.is_none_or(|f| f.chars().all(|c| c == '0'));
    if is_int || !multiplier.is_empty() {
        let words = number_words(int_part, if is_int { None } else { frac_part });
        let plural = if int_part == "1" && frac_part.is_none() && multiplier.is_empty() {
            bill.to_string()
        } else {
            format!("{bill}s")
        };
        let mult = if multiplier.is_empty() {
            String::new()
        } else {
            format!(" {multiplier}")
        };
        format!("{prefix}{words}{mult} {plural}")
    } else {
        let cents_str = frac_part.unwrap_or("0");
        let padded = format!("{:0<2}", cents_str);
        let cents: u64 = padded[..2].parse().unwrap_or(0);
        let bills = format!("{}{}", prefix, cardinal_str(int_part));
        let bill_word = if int_part == "1" {
            bill.to_string()
        } else {
            format!("{bill}s")
        };
        let coin_word = if cents == 1 || coin == "pence" {
            coin.to_string()
        } else {
            format!("{coin}s")
        };
        format!(
            "{bills} {bill_word} and {} {coin_word}",
            num::cardinal(cents)
        )
    }
}

fn handle_number(caps: &Captures) -> String {
    let neg = caps.get(1).map(|m| m.as_str()).unwrap_or_default() == "-";
    let number = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
    let multiplier = multiplier_word(caps.get(3).map(|m| m.as_str()).unwrap_or_default());
    let prefix = if neg { "minus " } else { "" };
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (number, None),
    };
    if multiplier.is_empty()
        && frac_part.is_none()
        && int_part.len() == 4
        && let Ok(n) = int_part.parse::<u64>()
        && n > 1500
        && n % 1000 > 9
    {
        return format!(
            "{prefix}{} {}",
            cardinal_str(&int_part[..2]),
            cardinal_str(&int_part[2..])
        );
    }
    let words = number_words(int_part, frac_part);
    let mult = if multiplier.is_empty() {
        String::new()
    } else {
        format!(" {multiplier}")
    };
    format!("{prefix}{words}{mult}")
}

fn handle_time(caps: &Captures) -> String {
    let hhmm = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let ampm = caps.get(4).map(|m| m.as_str().trim().to_string());
    let parts: Vec<&str> = hhmm.split(':').map(str::trim).collect();
    let mut out: Vec<String> = Vec::new();
    out.push(cardinal_str(parts.first().unwrap_or(&"0")));
    let minutes: u64 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
    if minutes < 10 {
        if minutes != 0 {
            out.push(format!("oh {}", num::cardinal(minutes)));
        }
    } else {
        out.push(num::cardinal(minutes));
    }
    match parts.get(2) {
        Some(sec) => {
            let s: u64 = sec.parse().unwrap_or(0);
            let word = if s == 1 { "second" } else { "seconds" };
            out.push(format!("and {} {word}", num::cardinal(s)));
        }
        None => match &ampm {
            Some(half) => return format!("{} {half}", out.join(" ")),
            None => {
                if minutes == 0 {
                    out.push("o'clock".into());
                }
            }
        },
    }
    out.join(" ")
}

fn handle_email(caps: &Captures) -> String {
    let email = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
    match email.split_once('@') {
        Some((user, domain)) => format!("{user} at {}", domain.replace('.', " dot ")),
        None => email.into(),
    }
}

fn handle_url(caps: &Captures) -> String {
    let url = caps
        .get(0)
        .map(|m| m.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let url = re(r"^https?://").replace(&url, |c: &Captures| {
        if c.get(0)
            .map(|m| m.as_str().contains("https"))
            .unwrap_or(false)
        {
            "https "
        } else {
            "http "
        }
        .to_string()
    });
    let url = re(r"^www\.").replace(&url, "www ").to_string();
    let url = re(r":(\d+)(?=/|$)")
        .replace_all(&url, " colon $1")
        .to_string();
    let (domain, path) = match url.split_once('/') {
        Some((d, p)) => (d.to_string(), Some(p.to_string())),
        None => (url, None),
    };
    let domain = domain.replace('.', " dot ");
    let mut u = match path {
        Some(p) => format!("{domain} slash {p}"),
        None => domain,
    };
    for (a, b) in [
        ("-", " dash "),
        ("_", " underscore "),
        ("?", " question-mark "),
        ("=", " equals "),
        ("&", " ampersand "),
        ("%", " percent "),
        (":", " colon "),
        ("/", " slash "),
    ] {
        u = u.replace(a, b);
    }
    re(r"\s+").replace_all(&u, " ").trim().to_string()
}

fn replace_fn(rx: &Regex, text: &str, f: impl Fn(&Captures) -> String) -> String {
    rx.replace_all(text, |c: &Captures| f(c)).to_string()
}

static UNIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)((?<!\w)([+-]?)(\d{1,3}(,\d{3})*|\d+)(\.\d+)?)\s*(kbps|mbps|khz|mhz|ghz|lbs|min|kph|mph|mm|cm|km|kg|mg|ms|ml|hz|kv|kw|mw|kb|mb|gb|tb|px|ft|yd|mi|lb|oz|m|g|h|l|v|w)(?=[^\w\d]|$)",
    )
});

static TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)([0-9]{1,2} ?: ?[0-9]{2}( ?: ?[0-9]{2})?)( ?(pm|am)\b)?"));

static MONEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(-?)([$£€])(\d+(?:\.\d+)?)((?: hundred| thousand| (?:[bm]|tr|quadr)illion|k|m|b|t)*)\b",
    )
});

static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(-?)(\d+(?:\.\d+)?)((?: hundred| thousand| (?:[bm]|tr|quadr)illion|k|m|b)*)\b")
});

static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b[a-z0-9._%+-]{1,64}@[a-z0-9.-]{1,253}\.[a-z]{2,}\b"));

static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)(?<![a-zA-Z0-9.-])(?:https?://)?(?:www\.)?(?:localhost|[a-zA-Z0-9.-]{{1,253}}(?:\.(?:{}))+|[0-9]{{1,3}}\.[0-9]{{1,3}}\.[0-9]{{1,3}}\.[0-9]{{1,3}})(?::[0-9]+)?(?:[/?][^\s]*)?",
        VALID_TLDS.join("|")
    ))
});

pub fn normalize(text: &str) -> String {
    let mut t = text.to_string();
    t = re(r"(?i)\b(how|what|where|who|when|why|there|these|those)['’]re\b")
        .replace_all(&t, "$1 are")
        .to_string();
    t = re(r"(?i)\b(how|what|where|when|why|there|these|those|you|they)re\b")
        .replace_all(&t, "$1 are")
        .to_string();
    t = replace_fn(&EMAIL_RE, &t, handle_email);
    t = replace_fn(&URL_RE, &t, handle_url);
    t = replace_fn(&UNIT_RE, &t, |c| {
        let unit_str = c.get(6).map(|m| m.as_str()).unwrap_or_default();
        let number = c.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
        match unit_word(unit_str) {
            Some(u) => {
                let mut parts: Vec<String> = u.split(' ').map(String::from).collect();
                if parts.first().is_some_and(|p| p.ends_with("bit"))
                    && unit_str
                        .chars()
                        .nth(unit_str.len().min(2) - 1)
                        .is_some_and(|c| c == 'B')
                    && let Some(f) = parts.first_mut()
                {
                    *f = format!("{}byte", &f[..f.len() - 3]);
                }
                let plural = number != "1" && number != "+1" && number != "-1";
                if plural && let Some(f) = parts.first_mut() {
                    if *f == "foot" {
                        *f = "feet".into();
                    } else if !f.ends_with(['s', 'z', 'x']) {
                        f.push('s');
                    }
                }
                format!("{number} {}", parts.join(" "))
            }
            None => c.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
        }
    });
    t = t.replace("(s)", "s");
    t = t.replace(['‘', '’'], "'");
    t = t.replace('«', "\u{201c}").replace('»', "\u{201d}");
    t = t.replace(['\u{201c}', '\u{201d}'], "\"");
    for (a, b) in "、。！，：；？–".chars().zip(",.!,:;?-".chars()) {
        t = t.replace(a, &format!("{b} "));
    }
    t = replace_fn(&TIME_RE, &t, handle_time);
    t = re(r"[^\S \n]").replace_all(&t, " ").to_string();
    t = re(r"  +").replace_all(&t, " ").to_string();
    t = t.replace(['\n', '\r'], " ");
    t = re(r"\bD[Rr]\.(?= [A-Z])")
        .replace_all(&t, "Doctor")
        .to_string();
    t = re(r"\b(?:Mr\.|MR\.(?= [A-Z]))")
        .replace_all(&t, "Mister")
        .to_string();
    t = re(r"\b(?:Ms\.|MS\.(?= [A-Z]))")
        .replace_all(&t, "Miss")
        .to_string();
    t = re(r"\b(?:Mrs\.|MRS\.(?= [A-Z]))")
        .replace_all(&t, "Mrs")
        .to_string();
    t = re(r"\betc\.(?! [A-Z])").replace_all(&t, "etc").to_string();
    t = re(r"(?i)\b(y)eah?\b").replace_all(&t, "$1e'a").to_string();
    t = re(r"(?<=\d),(?=\d)").replace_all(&t, "").to_string();
    t = re(r"(?<=\d)-(?=\d)").replace_all(&t, " to ").to_string();
    t = replace_fn(&re(r"\d+(?:\.\d+){2,}"), &t, |c| {
        c.get(0)
            .map(|m| {
                m.as_str()
                    .split('.')
                    .map(cardinal_str)
                    .collect::<Vec<_>>()
                    .join(" point ")
            })
            .unwrap_or_default()
    });
    t = replace_fn(&MONEY_RE, &t, handle_money);
    t = replace_fn(&NUMBER_RE, &t, handle_number);
    t = replace_fn(&re(r"(?<!\d)\d*\.\d+"), &t, |c| {
        let s = c.get(0).map(|m| m.as_str()).unwrap_or_default();
        match s.split_once('.') {
            Some((a, b)) => format!(
                "{a} point {}",
                b.chars()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            None => s.into(),
        }
    });
    for (sym, rep) in [
        ("~", " "),
        ("@", " at "),
        ("#", " number "),
        ("$", " dollar "),
        ("%", " percent "),
        ("^", " "),
        ("&", " and "),
        ("*", " "),
        ("_", " "),
        ("|", " "),
        ("\\", " "),
        ("/", " slash "),
        ("=", " equals "),
        ("+", " plus "),
    ] {
        t = t.replace(sym, rep);
    }
    t = re(r"(?<=\d)S").replace_all(&t, " S").to_string();
    t = re(r"(?<=[BCDFGHJ-NP-TV-Z])'?s\b")
        .replace_all(&t, "'S")
        .to_string();
    t = re(r"(?<=X')S\b").replace_all(&t, "s").to_string();
    t = replace_fn(&re(r"(?:[A-Za-z]\.){2,12} [a-z]"), &t, |c| {
        c.get(0)
            .map(|m| m.as_str().replace('.', "-"))
            .unwrap_or_default()
    });
    t = re(r"(?i)(?<=[A-Z])\.(?=[A-Z])")
        .replace_all(&t, "-")
        .to_string();
    t = re(r"\s{2,}").replace_all(&t, " ").to_string();
    t.trim().to_string()
}
