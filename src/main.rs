//! claudish — Claudish <-> plain English, run locally, wired into Claude Code like rtk.
mod gain;
mod hook;

use clap::{Parser, Subcommand};
use paw_rs::prelude::*;
use std::io::{Read, Write};

pub type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const TO_ENGLISH: &str = "e469f61ccab2699fbd51";
pub const TO_CLAUDISH: &str = "ca9d5165b6c8e6615529";
// Base model both programs run on (same file the Python SDK downloads).
const BASE_GGUF: &str = "qwen3-0.6b-q6_k.gguf";
const BASE_URL: &str =
    "https://huggingface.co/programasweights/Qwen3-0.6B-GGUF-Q6_K/resolve/main/qwen3-0.6b-q6_k.gguf";

#[derive(Parser)]
#[command(name = "claudish", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Claudish -> plain English (TEXT or stdin)
    ToEnglish { text: Option<String> },
    /// Plain English -> Claudish (TEXT or stdin)
    ToClaudish { text: Option<String> },
    /// Process an agent hook event (JSON on stdin)
    Hook {
        #[command(subcommand)]
        agent: hook::Agent,
    },
    /// Install the Claude Code hook into .claude/settings.json (or ~/.claude with -g)
    Init {
        #[arg(short, long)]
        global: bool,
        #[arg(long)]
        uninstall: bool,
    },
    /// Show token savings
    Gain,
}

fn main() {
    let r = match Cli::parse().cmd {
        Cmd::ToEnglish { text } => cli_translate(TO_ENGLISH, text),
        Cmd::ToClaudish { text } => cli_translate(TO_CLAUDISH, text),
        Cmd::Hook { agent } => hook::run(agent),
        Cmd::Init { global, uninstall } => hook::init(global, uninstall),
        Cmd::Gain => {
            gain::report();
            Ok(())
        }
    };
    let code = match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("claudish: {e}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    // ponytail: skip atexit destructors. paw-llamacpp parks the model in a static pool and
    // ggml-metal asserts while tearing it down (exit 134), which would make Claude Code
    // discard the hook's stdout. The process is finished; nothing needs unwinding.
    unsafe { libc::_exit(code) }
}

fn cli_translate(program: &str, text: Option<String>) -> R<()> {
    let input = match text {
        Some(t) => t,
        None => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s
        }
    };
    let input = input.trim();
    if input.is_empty() {
        return Err("no input (pass TEXT or pipe to stdin)".into());
    }
    let out = Translator::load(program)?.run(input)?;
    let kind = if program == TO_ENGLISH { "to-english" } else { "to-claudish" };
    gain::record(kind, input.len(), out.len());
    println!("{out}");
    Ok(())
}

/// A loaded PAW program. Downloads program + base model on first use.
pub struct Translator {
    _rt: tokio::runtime::Runtime,
    f: Box<dyn PawFnTrait>,
}

impl Translator {
    pub fn load(program: &str) -> R<Self> {
        let cfg = config()?; // before the runtime spawns threads (set_var)
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(ensure_base_model(&cfg))?;
        let f = quiet(|| rt.block_on(PawFnBuilder::builder().config(cfg).id(program).load()))?;
        Ok(Self { _rt: rt, f })
    }

    pub fn run(&mut self, input: &str) -> R<String> {
        Ok(quiet(|| self.f.run(input))?.trim().to_string())
    }
}

/// Claudish -> plain English, whole text through the program like the live demo. Used by the hook.
pub fn to_english(text: &str) -> R<String> {
    Translator::load(TO_ENGLISH)?.run(text)
}

/// Run `f` with fd 2 pointed at /dev/null: ggml prints device banners straight to
/// stderr, bypassing tracing. `PAW_VERBOSE=1` keeps them.
fn quiet<T>(f: impl FnOnce() -> T) -> T {
    if std::env::var_os("PAW_VERBOSE").is_some() {
        return f();
    }
    unsafe {
        let saved = libc::dup(2);
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
        libc::dup2(null, 2);
        libc::close(null);
        let r = f();
        libc::dup2(saved, 2);
        libc::close(saved);
        r
    }
}

/// Same cache layout as the Python SDK (~/.cache/programasweights) so downloads are shared;
/// PAW_* env vars still override everything.
fn config() -> R<PawConfig> {
    // ponytail: env var rather than the builder — paw-rs's `.id()` step rebuilds its
    // config from env and silently drops a builder-set cache_dir.
    if std::env::var_os("PAW_CACHE_DIR").is_none() {
        let home = std::env::var("HOME")?;
        std::env::set_var(
            "PAW_CACHE_DIR",
            std::path::Path::new(&home).join(".cache/programasweights"),
        );
    }
    Ok(PawConfig::from_env())
}

async fn ensure_base_model(cfg: &PawConfig) -> R<()> {
    let path = cfg.base_models_dir().join(BASE_GGUF);
    if path.exists() {
        return Ok(());
    }
    eprintln!(
        "claudish: downloading base model once (~600 MB) -> {}",
        path.display()
    );
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("gguf.part");
    let mut resp = reqwest::get(BASE_URL).await?.error_for_status()?;
    let mut f = std::fs::File::create(&tmp)?;
    while let Some(chunk) = resp.chunk().await? {
        f.write_all(&chunk)?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
