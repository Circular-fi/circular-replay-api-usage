//! Transaction signatures of a slot via `GET /v2/replay/slot/{slot}/entries`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example slot_entries -- --slot 465589364
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient};
use clap::Parser;
use std::path::PathBuf;

const DEFAULT_URL: &str = "https://pro.circular.fi";
const DEFAULT_SLOT: u64 = 465_589_364;

#[derive(Parser)]
#[command(about = "Call GET /v2/replay/slot/{slot}/entries")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = DEFAULT_URL)]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value_t = DEFAULT_SLOT)]
    slot: u64,

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

    let value = client
        .slot_entries(args.slot)
        .await
        .context("GET /v2/replay/slot/entries failed")?;

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

    eprintln!("--- GET /v2/replay/slot/{{slot}}/entries ---");
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
