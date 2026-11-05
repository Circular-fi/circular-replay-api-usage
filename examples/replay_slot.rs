//! Account states at the start of a slot via `GET /v2/replay/slot/{slot}`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example replay_slot -- --slot 465589364 \
//!     --account Harmonic8SoAfZixJqUxxdSeyWzr6q7XFjuiXS5Z2zPg
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient, ReplaySlotQuery};
use clap::Parser;
use std::path::PathBuf;

const DEFAULT_URL: &str = "https://pro.circular.fi";
const DEFAULT_SLOT: u64 = 465_589_364;

#[derive(Parser)]
#[command(about = "Call GET /v2/replay/slot/{slot}")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = DEFAULT_URL)]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value_t = DEFAULT_SLOT)]
    slot: u64,

    #[arg(
        long = "account",
        value_delimiter = ',',
        default_value = "Harmonic8SoAfZixJqUxxdSeyWzr6q7XFjuiXS5Z2zPg"
    )]
    accounts: Vec<String>,

    #[arg(long, default_value_t = true)]
    alts: bool,

    #[arg(long, default_value_t = true)]
    spl: bool,

    #[arg(long, default_value_t = true)]
    program_details: bool,

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

    let query = ReplaySlotQuery {
        alts: args.alts,
        spl: args.spl,
        program_details: args.program_details,
        accounts: args.accounts.clone(),
    };

    let value = client
        .replay_slot(args.slot, &query)
        .await
        .context("GET /v2/replay/slot failed")?;

    let returned_slot = value.get("slot").map(|v| v.to_string()).unwrap_or_default();
    let account_states = value
        .get("accounts")
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0);

    eprintln!("--- GET /v2/replay/slot/{{slot}} ---");
    eprintln!("slot:     {returned_slot}");
    eprintln!("queried:  {} account(s)", args.accounts.len());
    eprintln!("response: {account_states} account state(s) in JSON");

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
