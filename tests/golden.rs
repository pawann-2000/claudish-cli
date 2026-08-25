//! Regression: `claudish to-english` on every tests/golden/*.in must equal the matching
//! *.out. Local inference is greedy with a fixed seed, so output is byte-stable; a diff
//! means the program, base model, or CLI pipeline changed. Needs the cached models:
//!   cargo test --release -- --ignored
use std::process::{Command, Stdio};
use std::io::Write;

#[test]
#[ignore = "runs the local model (~620 MB download on first use)"]
fn to_english_matches_golden() {
    let home = std::env::var("HOME").unwrap();
    let cache = std::env::var("PAW_CACHE_DIR").unwrap_or(format!("{home}/.cache/programasweights"));
    let tmp = std::env::temp_dir().join("claudish-golden-home"); // keep the gain ledger out of ~
    std::fs::create_dir_all(&tmp).unwrap();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "in") {
            continue;
        }
        let want = std::fs::read_to_string(p.with_extension("out")).unwrap();
        let mut c = Command::new(env!("CARGO_BIN_EXE_claudish"))
            .arg("to-english")
            .env("HOME", &tmp)
            .env("PAW_CACHE_DIR", &cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(&std::fs::read(&p).unwrap()).unwrap();
        let out = c.wait_with_output().unwrap();
        assert!(out.status.success(), "{}: exit {:?}", p.display(), out.status);
        let got = String::from_utf8(out.stdout).unwrap();
        assert_eq!(got.trim(), want.trim(), "{}", p.display());
        n += 1;
    }
    assert!(n > 0, "no golden cases in {dir}");
}
