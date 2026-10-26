use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub fn fixture_path(dir: impl AsRef<Path>, signature: &str) -> PathBuf {
    let prefix: String = signature.chars().take(16).collect();
    dir.as_ref().join(format!("replay-circular-{prefix}.json"))
}

pub fn save_json_pretty(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(value)?;
    fs::write(path, &pretty).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Pretty-print a JSON value to stdout (used by the endpoint examples).
pub fn print_json_response(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn print_replay_summary(label: &str, value: &Value) {
    let signature = value
        .get("signature")
        .and_then(|v| v.as_str())
        .unwrap_or("(missing)");
    let slot = value
        .get("slot")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "?".into());
    let tx_success = value
        .get("tx_success")
        .and_then(|v| v.as_bool())
        .map(|b| b.to_string())
        .unwrap_or_else(|| "?".into());
    let pre = value.get("pre").and_then(|v| v.as_object()).map(|m| m.len());
    let post = value.get("post").and_then(|v| v.as_object()).map(|m| m.len());

    println!("--- {label} ---");
    println!("signature:  {signature}");
    println!("slot:       {slot}");
    println!("tx_success: {tx_success}");
    if let (Some(pre_n), Some(post_n)) = (pre, post) {
        println!("accounts:   pre={pre_n} post={post_n}");
    }
    if value.get("execution").is_some() {
        println!("execution:  present");
    }
}
