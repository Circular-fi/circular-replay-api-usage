//! Fetch a replay snapshot from `POST /v1/replay`, optionally with bonus accounts.
//!
//! Requires a reachable gateway and `CIRCULAR_REPLAY_API_KEY`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example replay_from_api -- --signature '<base58>'
//!
//! # up to 100 extra accounts captured just before the tx (repeat or comma-separated)
//! cargo run --example replay_from_api -- --signature '<base58>' \
//!     --bonus-account <pubkey> --bonus-account <pubkey>
//!
//! # program bytecode mode (default inline so the saved fixture is self-contained)
//! cargo run --example replay_from_api -- --signature '<base58>' --programs ref
//!
//! cargo run --example replay_from_api -- --signature '<base58>' -o fixtures/my-replay.json
//! ```

use anyhow::{Context, Result};
use circular_replay::{
    print_json_response, print_replay_summary, CircularReplayClient, CircularReplayRequest,
    ProgramsMode,
};
use clap::Parser;
use std::path::PathBuf;
use std::str::FromStr;

const DEFAULT_SIGNATURE: &str =
    "4HqcGERuLJKvnKdaCPW5zjbhLf87tfXErJLhTxvRmneYnqQd1JVM2XtWsUPb5dcKPBWRMPDmW3R1NB1ymhkZPCF6";

#[derive(Parser)]
#[command(about = "Call POST /v1/replay (with optional bonus accounts)")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = "http://localhost:8080")]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    /// Base58 transaction signature to replay.
    #[arg(long, default_value = DEFAULT_SIGNATURE)]
    signature: String,

    /// Optional hint: the slot containing the transaction.
    #[arg(long)]
    slot: Option<u64>,

    /// Up to 100 extra base58 pubkeys (repeat `--bonus-account` or comma-separated).
    #[arg(long = "bonus-account", value_delimiter = ',')]
    bonus_accounts: Vec<String>,

    /// Program bytecode mode: ref | inline | none.
    #[arg(long, default_value = "inline", value_parser = ProgramsMode::from_str)]
    programs: ProgramsMode,

    #[arg(long = "no-alts", action = clap::ArgAction::SetFalse)]
    alts: bool,

    #[arg(long = "no-spl", action = clap::ArgAction::SetFalse)]
    spl: bool,

    /// Print the full replay JSON to stdout (can be very large).
    #[arg(long)]
    print_response: bool,

    #[arg(long, short)]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;

    let status = client.status().await.context("GET /status")?;
    eprintln!(
        "gateway status: min_query_slot={} max_indexed_slot={}",
        status.get("min_query_slot").map(|v| v.to_string()).unwrap_or_default(),
        status.get("max_indexed_slot").map(|v| v.to_string()).unwrap_or_default(),
    );

    let mut req = CircularReplayRequest::new(&args.signature)
        .with_bonus_accounts(args.bonus_accounts.clone())
        .with_programs(args.programs);
    req.alt_details = args.alts;
    req.spl_details = args.spl;
    if let Some(slot) = args.slot {
        req = req.with_slot(slot);
    }

    eprintln!("--- POST /v1/replay ---");
    eprintln!("base:           {}", client.base_url());
    eprintln!("bonus_accounts: {}", args.bonus_accounts.len());
    eprintln!("programs:       {}", args.programs.as_str());

    let snapshot = client
        .replay_with_accounts(&req)
        .await
        .context("Circular POST /v1/replay failed")?;

    print_replay_summary("POST /v1/replay", &snapshot);
    if let Some(bonus) = snapshot.get("bonus_accounts").and_then(|v| v.as_object()) {
        eprintln!("bonus_accounts returned: {}", bonus.len());
    }

    if args.print_response {
        print_json_response(&snapshot)?;
    }

    if let Some(out) = args.output {
        let pretty = serde_json::to_string_pretty(&snapshot)?;
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out, pretty)?;
        eprintln!("wrote {}", out.display());
    }

    Ok(())
}
