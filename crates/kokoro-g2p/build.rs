use flate2::{Compression, write::GzEncoder};
use std::{env, fs, io::Write, path::Path};

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    for name in [
        "us_gold.json",
        "us_silver.json",
        "zh_pinyin.json",
        "ja_words.txt",
    ] {
        let src = format!("data/{name}");
        println!("cargo:rerun-if-changed={src}");
        let raw = fs::read(&src).unwrap();
        let mut enc = GzEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&raw).unwrap();
        fs::write(
            Path::new(&out).join(format!("{name}.gz")),
            enc.finish().unwrap(),
        )
        .unwrap();
    }
}
