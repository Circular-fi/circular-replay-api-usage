//! Account states at the start of a slot via `POST /v1/replay-slot`.
//!
//! For the state at the *end* of slot N, ask for N+1.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example replay_slot -- --slot 461775917 \
//!     --account Harmonic8SoAfZixJqUxxdSeyWzr6q7XFjuiXS5Z2zPg
//! ```

use anyhow::{Context, Result};
use circular_replay::{print_json_response, CircularReplayClient, ProgramsMode, ReplaySlotRequest};
use clap::Parser;
use std::path::PathBuf;
use std::str::FromStr;

const DEFAULT_SLOT: u64 = 461_775_917;

#[derive(Parser)]
#[command(about = "Call POST /v1/replay-slot")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = "http://localhost:8080")]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value_t = DEFAULT_SLOT)]
    slot: u64,

    /// Base58 pubkeys, 1 to 100 (repeat `--account` or comma-separated).
    #[arg(
        long = "account",
        value_delimiter = ',',
        default_value = "Harmonic8SoAfZixJqUxxdSeyWzr6q7XFjuiXS5Z2zPg"
    )]
    accounts: Vec<String>,

    /// Program bytecode mode: ref | inline | none.
    #[arg(long, default_value = "inline", value_parser = ProgramsMode::from_str)]
    programs: ProgramsMode,

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

    let req = ReplaySlotRequest::new(args.slot, args.accounts.clone()).with_programs(args.programs);
    let value = client
        .replay_slot(&req)
        .await
        .context("Circular POST /v1/replay-slot failed")?;

    let returned_slot = value.get("slot").map(|v| v.to_string()).unwrap_or_default();
    let account_states = value
        .get("accounts")
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0);

    eprintln!("--- POST /v1/replay-slot ---");
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
