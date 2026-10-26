//! Transaction signatures of a slot, grouped by entry, via `POST /v1/slot-entries`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example slot_entries -- --slot 461775917
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient, SlotEntriesRequest};
use clap::Parser;
use std::path::PathBuf;

const DEFAULT_SLOT: u64 = 461_773_004;

#[derive(Parser)]
#[command(about = "Call POST /v1/slot-entries")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = "http://localhost:8080")]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value_t = DEFAULT_SLOT)]
    slot: u64,

    /// Only print a short summary (no JSON body on stdout).
    #[arg(long, short = 'q')]
    quiet: bool,

    #[arg(long, short)]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;

    let req = SlotEntriesRequest { slot: args.slot };
    let value = client
        .slot_entries(&req)
        .await
        .context("Circular POST /v1/slot-entries failed")?;

    let returned_slot = value.get("slot").map(|v| v.to_string()).unwrap_or_default();
    let entries = value.get("entries").and_then(|v| v.as_array());
    let entry_count = entries.map(|a| a.len()).unwrap_or(0);
    let tx_count: usize = entries
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("transactions").and_then(|t| t.as_array()))
                .map(|t| t.len())
                .sum()
        })
        .unwrap_or(0);

    eprintln!("--- POST /v1/slot-entries ---");
    eprintln!("slot:    {returned_slot}");
    eprintln!("entries: {entry_count} entry group(s), {tx_count} transaction(s)");

    if !args.quiet {
        print_json_response(&value)?;
    }

    if let Some(out) = args.output {
        let pretty = serde_json::to_string_pretty(&value)?;
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out, pretty)?;
        eprintln!("wrote {}", out.display());
    }

    Ok(())
}
