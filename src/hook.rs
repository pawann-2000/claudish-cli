//! Claude Code integration: the SubagentStop hook and settings.json install.
//!
//! Why SubagentStop: subagent reports reach the parent as a task notification,
//! not a tool result, so PostToolUse `updatedToolOutput` never sees them.
//! Blocking the stop with the plain-English text as the `reason` makes the
//! subagent re-emit that text as its final message; the parent only ever
//! reads the short version. The second stop (stop_hook_active=true) is let through.
use crate::{gain, R};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(clap::Subcommand)]
pub enum Agent {
    /// Claude Code SubagentStop hook (reads JSON from stdin)
    Claude,
}

const CMD: &str = "claudish hook claude";
// ponytail: fixed thresholds; revisit if `claudish gain` shows too many skips or tiny savings
const MIN_CHARS: usize = 240; // below this a model load costs more than it saves
const KEEP_BELOW_PCT: usize = 85; // only replace when the translation is <85% of the original
const REASON: &str = "claudish: your final report was compressed to plain English. \
Reply with exactly the following text and nothing else:";

pub fn run(agent: Agent) -> R<()> {
    let Agent::Claude = agent;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    let v: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
    if let Some(out) = decide(&v, crate::to_english)? {
        println!("{out}");
    }
    Ok(())
}

/// Pure decision: returns the hook JSON to print, or None to let the stop through.
fn decide(v: &Value, translate: impl Fn(&str) -> R<String>) -> R<Option<String>> {
    let msg = v["last_assistant_message"].as_str().unwrap_or("");
    if v["hook_event_name"] != "SubagentStop"
        || v["stop_hook_active"] == true
        || msg.len() < MIN_CHARS
    {
        return Ok(None);
    }
    let out = translate(msg)?;
    if out.is_empty() || out.len() * 100 >= msg.len() * KEEP_BELOW_PCT {
        return Ok(None);
    }
    gain::record("hook", msg.len(), out.len());
    Ok(Some(
        json!({"decision": "block", "reason": format!("{REASON}\n\n{out}")}).to_string(),
    ))
}

pub fn init(global: bool, uninstall: bool) -> R<()> {
    let path = if global {
        PathBuf::from(std::env::var("HOME")?).join(".claude/settings.json")
    } else {
        PathBuf::from(".claude/settings.json")
    };
    patch_settings(&path, uninstall)?;
    println!(
        "{}: hook {}",
        path.display(),
        if uninstall { "removed" } else { "installed" }
    );
    if !uninstall {
        eprintln!("claudish: warming up models (first run downloads them)...");
        crate::to_english("Warm-up: here's where I'd hold the line, the gate is the test suite.")?;
        println!("Restart Claude Code to activate. Check savings with `claudish gain`.");
    }
    Ok(())
}

fn patch_settings(path: &Path, uninstall: bool) -> R<()> {
    let mut root: Value = match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s)?,
        Err(_) => json!({}),
    };
    let hooks = root
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert(json!({}))
        .as_object_mut()
        .ok_or("settings.json `hooks` is not an object")?;
    let arr = hooks
        .entry("SubagentStop")
        .or_insert(json!([]))
        .as_array_mut()
        .ok_or("settings.json `hooks.SubagentStop` is not an array")?;
    let ours = |e: &Value| {
        e["hooks"].as_array().is_some_and(|h| {
            h.iter().any(|x| {
                x["command"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("claudish hook"))
            })
        })
    };
    arr.retain(|e| !ours(e)); // idempotent: never two entries
    if !uninstall {
        arr.push(json!({"hooks": [{"type": "command", "command": CMD, "timeout": 120}]}));
    }
    if arr.is_empty() {
        hooks.remove("SubagentStop");
    }
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.bak"))?;
    } else if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&root)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: &str = "The honest shape is asymmetric: the data is correct; the format is hard to read. Correctness landed; legibility did not. The load-bearing constraint is the gate, not the suggestion. The verdict here is that the surface is stale and the path is owner-gated. That distinction matters; that is the boundary.";

    fn ev(active: bool, msg: &str) -> Value {
        json!({"hook_event_name": "SubagentStop", "stop_hook_active": active, "last_assistant_message": msg})
    }

    #[test]
    fn decide_blocks_only_when_it_should() {
        let short = |m: &str| Ok(m[..m.len() / 2].to_string());
        let out = decide(&ev(false, LONG), short).unwrap().unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["decision"], "block");
        assert!(v["reason"].as_str().unwrap().starts_with(REASON));
        // second stop, short report, other events, no gain -> pass through
        assert!(decide(&ev(true, LONG), short).unwrap().is_none());
        assert!(decide(&ev(false, "Done."), short).unwrap().is_none());
        assert!(decide(
            &json!({"hook_event_name": "Stop", "last_assistant_message": LONG}),
            short
        )
        .unwrap()
        .is_none());
        assert!(decide(&ev(false, LONG), |m| Ok(m.to_string()))
            .unwrap()
            .is_none());
    }

    #[test]
    fn init_is_idempotent_and_reversible() {
        let dir = std::env::temp_dir().join(format!("claudish-test-{}", std::process::id()));
        let path = dir.join("settings.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"permissions":{"allow":["Bash"]},"hooks":{"SubagentStop":[{"hooks":[{"type":"command","command":"other"}]}]}}"#).unwrap();
        patch_settings(&path, false).unwrap();
        patch_settings(&path, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let arr = v["hooks"]["SubagentStop"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "one foreign entry + exactly one of ours");
        assert_eq!(arr[1]["hooks"][0]["command"], CMD);
        assert_eq!(v["permissions"]["allow"][0], "Bash");
        assert!(path.with_extension("json.bak").exists());
        patch_settings(&path, true).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["hooks"]["SubagentStop"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
