use kokoro_g2p::{EnOptions, G2p, Lang};
use std::sync::LazyLock;

static G2P: LazyLock<G2p> = LazyLock::new(|| G2p::new().expect("init"));

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn parity(golden: &str, phonemize: impl Fn(&str) -> String) -> (f64, f64) {
    let path = format!("{}/tests/fixtures/{golden}", env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read_to_string(path).expect("golden corpus");
    let mut exact = 0usize;
    let mut total = 0usize;
    let mut dist_sum = 0usize;
    let mut len_sum = 0usize;
    for line in data.lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("jsonl");
        let text = v["text"].as_str().unwrap_or_default();
        let expected = v["phonemes"].as_str().unwrap_or_default();
        let got = phonemize(text);
        total += 1;
        let e: Vec<char> = expected.chars().collect();
        let o: Vec<char> = got.chars().collect();
        dist_sum += levenshtein(&e, &o);
        len_sum += e.len().max(o.len());
        if got == expected {
            exact += 1;
        } else if std::env::var("G2P_DEBUG").is_ok() {
            println!("TEXT {text}\n EXP {expected}\n GOT {got}\n");
        }
    }
    let exact_pct = 100.0 * exact as f64 / total as f64;
    let sim_pct = 100.0 * (1.0 - dist_sum as f64 / len_sum as f64);
    println!("{golden}: exact {exact}/{total} = {exact_pct:.2}%  similarity {sim_pct:.2}%");
    (exact_pct, sim_pct)
}

#[test]
fn golden_parity_en() {
    let opts = EnOptions {
        normalize: false,
        fallback: false,
    };
    let (exact, sim) = parity("en_golden.jsonl", |t| {
        G2P.phonemize_en(t, None, opts).unwrap_or_default()
    });
    assert!(
        exact >= 99.5,
        "exact {exact:.2}% below floor 99.51% (407/409)"
    );
    assert!(sim >= 99.8, "similarity {sim:.2}% below floor 99.81%");
}

#[test]
fn golden_parity_ja() {
    let (exact, sim) = parity("ja_golden.jsonl", |t| {
        G2P.phonemize(t, Lang::Ja, None).unwrap_or_default()
    });
    assert!(
        exact >= 86.5,
        "exact {exact:.2}% below recorded floor 86.55% (103/119)"
    );
    assert!(sim >= 99.2, "similarity {sim:.2}% below floor 99.21%");
}

#[test]
fn golden_parity_zh() {
    let (exact, sim) = parity("zh_golden.jsonl", |t| {
        G2P.phonemize(t, Lang::Zh, None).unwrap_or_default()
    });
    assert!(
        exact >= 100.0,
        "exact {exact:.2}% below recorded floor 100% (120/120)"
    );
    assert!(sim >= 100.0, "similarity {sim:.2}% below floor 100%");
}
