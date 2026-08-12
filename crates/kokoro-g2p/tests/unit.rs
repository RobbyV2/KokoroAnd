use kokoro_g2p::{CustomDict, EnOptions, G2p, Lang, normalize};

fn g2p() -> G2p {
    G2p::new().expect("init")
}

#[test]
fn normalizer() {
    assert_eq!(
        normalize("The meeting starts at 3:00 pm."),
        "The meeting starts at three pm."
    );
    assert_eq!(normalize("Wake me at 6:30."), "Wake me at six thirty.");
    assert_eq!(normalize("It costs $5."), "It costs five dollars.");
    assert_eq!(
        normalize("He paid $4.75 for it."),
        "He paid four dollars and seventy-five cents for it."
    );
    assert_eq!(
        normalize("They raised $3.2 million."),
        "They raised three point two million dollars."
    );
    assert_eq!(
        normalize("audio at 24 kHz."),
        "audio at twenty-four kilohertz."
    );
    assert_eq!(
        normalize("It weighs 2.5 kg."),
        "It weighs two point five kilograms."
    );
    assert_eq!(
        normalize("Version 3.11.9 fixed it."),
        "Version three point eleven point nine fixed it."
    );
    assert_eq!(
        normalize("She was born in 1987."),
        "She was born in nineteen eighty-seven."
    );
    assert_eq!(normalize("pages 5-10"), "pages five to ten");
    assert_eq!(
        normalize("Dr. Smith and Mr. Jones"),
        "Doctor Smith and Mister Jones"
    );
    assert_eq!(
        normalize("Visit https://example.com/docs now."),
        "Visit https example dot com slash docs now."
    );
    assert_eq!(
        normalize("mail hello@example.org today"),
        "mail hello at example dot org today"
    );
}

#[test]
fn custom_dict_layered_over_gold() {
    let g = g2p();
    let dict = CustomDict::from_json(r#"{"hello": "hɛlOOO", "zyxwyn": "zˈɪkswɪn"}"#).expect("json");
    let out = g
        .phonemize("hello zyxwyn", Lang::EnUs, Some(&dict))
        .expect("run");
    assert!(out.contains("hɛlOOO"), "custom overrides gold: {out}");
    assert!(out.contains("zˈɪkswɪn"), "custom covers OOV: {out}");
    let roundtrip = CustomDict::from_json(&dict.to_json().expect("ser")).expect("de");
    assert_eq!(dict, roundtrip);
}

#[test]
fn inline_ipa_override() {
    let g = g2p();
    let out = g
        .phonemize("I like [kokoro](/kˈOkəɹO/) a lot.", Lang::EnUs, None)
        .expect("run");
    assert!(out.contains("kˈOkəɹO"), "{out}");
    let stressed = g
        .phonemize_en(
            "[important](-1) word",
            None,
            EnOptions {
                normalize: false,
                fallback: false,
            },
        )
        .expect("run");
    assert!(
        !stressed.contains('ˈ') || !stressed.starts_with("ɪmpˈ"),
        "{stressed}"
    );
}

#[test]
fn oov_fallback_fills() {
    let g = g2p();
    let out = g
        .phonemize("The frobnicator gronkulated.", Lang::EnUs, None)
        .expect("run");
    for w in out.split_whitespace() {
        assert!(
            w.chars().any(|c| !c.is_ascii_punctuation()),
            "no dropped words: {out}"
        );
    }
    assert!(out.contains('ˈ'), "fallback stresses: {out}");
}

#[test]
fn unsupported_language() {
    let g = g2p();
    assert!(g.phonemize("hola", Lang::Es, None).is_err());
    assert!(g.phonemize("   ", Lang::EnUs, None).is_err());
}

#[test]
fn basic_sentence() {
    let g = g2p();
    let out = g
        .phonemize("Hello, this is a TTS test.", Lang::EnUs, None)
        .expect("run");
    assert_eq!(out, "həlˈO, ðɪs ɪz ɐ tˌitˌiˈɛs tˈɛst.");
}
