//! Replayable slot window via `GET /v2/replay/status`.
//!
//! ```bash
//! cargo run --example status
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient};
use clap::Parser;

const DEFAULT_URL: &str = "https://pro.circular.fi";

#[derive(Parser)]
#[command(about = "Call GET /v2/replay/status")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = DEFAULT_URL)]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, short = 'q')]
    quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;

    let status = client.status().await.context("GET /v2/replay/status")?;

    if args.quiet {
        eprintln!(
            "status={} min_query_slot={} max_indexed_slot={}",
            status.get("status").and_then(|v| v.as_str()).unwrap_or("?"),
            status
                .get("min_query_slot")
                .map(|v| v.to_string())
                .unwrap_or_default(),
            status
                .get("max_indexed_slot")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        );
    } else {
        print_json_response(&status)?;
    }

    Ok(())
}
