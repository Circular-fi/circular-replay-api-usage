//! Queryable slot window via `GET /status` (public route, no API key).
//!
//! ```bash
//! cargo run --example status
//! cargo run --example status -- --circular-url http://localhost:8080
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Call GET /status")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = "http://localhost:8080")]
    circular_url: String,

    /// Not required for `/status`; only used to construct the client.
    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY", default_value = "")]
    api_key: String,

    /// Print only a short summary on stderr (no JSON on stdout).
    #[arg(long, short = 'q')]
    quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;

    let status = client.status().await.context("GET /status")?;

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
