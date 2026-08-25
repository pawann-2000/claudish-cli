//! Token-savings ledger. Same estimate rtk uses: tokens ≈ bytes / 4.
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn path() -> std::path::PathBuf {
    // ponytail: $HOME/.local/share like rtk; no platform-dirs crate
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join(".local/share/claudish/history.jsonl")
}

pub fn record(kind: &str, in_bytes: usize, out_bytes: usize) {
    let p = path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let _ = writeln!(
            f,
            r#"{{"ts":{ts},"kind":"{kind}","in":{in_bytes},"out":{out_bytes}}}"#
        );
    }
}

pub fn report() {
    let text = std::fs::read_to_string(path()).unwrap_or_default();
    let (mut n, mut inb, mut outb) = (0u64, 0u64, 0u64);
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        n += 1;
        inb += v["in"].as_u64().unwrap_or(0);
        outb += v["out"].as_u64().unwrap_or(0);
    }
    if n == 0 {
        println!("No translations recorded yet.");
        return;
    }
    let saved = inb.saturating_sub(outb);
    let pct = if inb > 0 { saved * 100 / inb } else { 0 };
    println!("claudish gain");
    println!("  translations  {n}");
    println!("  input         ~{} tokens", inb / 4);
    println!("  output        ~{} tokens", outb / 4);
    println!("  saved         ~{} tokens ({pct}%)", saved / 4);
    println!("  ledger        {}", path().display());
}
