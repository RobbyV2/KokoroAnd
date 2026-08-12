const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(u64, &str); 4] = [
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

fn under_100(n: u64) -> String {
    match n {
        0..=19 => ONES[n as usize].into(),
        _ => match n % 10 {
            0 => TENS[(n / 10) as usize].into(),
            r => format!("{}-{}", TENS[(n / 10) as usize], ONES[r as usize]),
        },
    }
}

fn under_1000(n: u64) -> String {
    match n {
        0..=99 => under_100(n),
        _ => match n % 100 {
            0 => format!("{} hundred", ONES[(n / 100) as usize]),
            r => format!("{} hundred and {}", ONES[(n / 100) as usize], under_100(r)),
        },
    }
}

pub fn cardinal(n: u64) -> String {
    if n == 0 {
        return "zero".into();
    }
    let mut parts: Vec<(String, u64)> = Vec::new();
    let mut rem = n;
    for (scale, name) in SCALES {
        let g = rem / scale;
        if g > 0 {
            parts.push((format!("{} {name}", under_1000(g)), g * scale));
        }
        rem %= scale;
    }
    if rem > 0 {
        parts.push((under_1000(rem), rem));
    }
    let mut out = String::new();
    for (i, (text, value)) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(if *value < 100 { " and " } else { ", " });
        }
        out.push_str(text);
    }
    out
}

pub fn ordinal(n: u64) -> String {
    let c = cardinal(n);
    let cut = c.rfind([' ', '-']).map_or(0, |i| i + 1);
    let (head, last) = c.split_at(cut);
    let tail = match last {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{tail}")
}

pub fn year(n: u64) -> String {
    let (high, low) = (n / 100, n % 100);
    if high == 0 || (high % 10 == 0 && low < 10) || high >= 100 {
        return cardinal(n);
    }
    let tail = match low {
        0 => "hundred".into(),
        1..=9 => format!("oh-{}", cardinal(low)),
        _ => cardinal(low),
    };
    format!("{} {tail}", cardinal(high))
}

pub fn float_words(s: &str) -> Option<String> {
    let v: f64 = s.parse().ok()?;
    let repr = format!("{v}");
    let (int_part, frac) = match repr.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (repr, "0".to_string()),
    };
    let int_n: u64 = int_part.parse().ok()?;
    let digits: Vec<&str> = frac
        .chars()
        .map(|c| ONES[c.to_digit(10).unwrap_or(0) as usize])
        .collect();
    Some(format!("{} point {}", cardinal(int_n), digits.join(" ")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn num2words_parity() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/num2words_ref.json"
        );
        let data = std::fs::read_to_string(path).expect("fixture");
        let v: HashMap<String, HashMap<String, String>> =
            serde_json::from_str(&data).expect("json");
        for (n, expected) in &v["cardinal"] {
            assert_eq!(
                &super::cardinal(n.parse().unwrap()),
                expected,
                "cardinal {n}"
            );
        }
        for (n, expected) in &v["ordinal"] {
            assert_eq!(&super::ordinal(n.parse().unwrap()), expected, "ordinal {n}");
        }
        for (n, expected) in &v["year"] {
            assert_eq!(&super::year(n.parse().unwrap()), expected, "year {n}");
        }
        for (n, expected) in &v["float"] {
            assert_eq!(
                super::float_words(n).as_deref(),
                Some(expected.as_str()),
                "float {n}"
            );
        }
    }
}
