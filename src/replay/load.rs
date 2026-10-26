use anyhow::{anyhow, Context, Result};
use crate::replay::snapshot::ReplaySnapshot;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_snapshot_file(path: impl AsRef<Path>) -> Result<ReplaySnapshot> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read replay file: {} (run `cargo run --example fetch_replay` first; \
             default name is fixtures/replay-circular-<first 16 chars of signature>.json)",
            path.display()
        )
    })?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse replay JSON: {}", path.display()))
}

/// Find `replay-*.json` under `txs_dir` whose `signature` field matches.
pub fn find_snapshot_by_signature(txs_dir: &Path, signature: &str) -> Result<Option<PathBuf>> {
    if !txs_dir.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(txs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        if v.get("signature").and_then(|s| s.as_str()) == Some(signature) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub fn resolve_snapshot_path(
    file: Option<PathBuf>,
    signature: &str,
    txs_dir: &Path,
) -> Result<PathBuf> {
    if let Some(p) = file {
        return Ok(p);
    }
    find_snapshot_by_signature(txs_dir, signature)?
        .ok_or_else(|| {
            anyhow!(
                "no local snapshot for signature {signature} under {}",
                txs_dir.display()
            )
        })
}
