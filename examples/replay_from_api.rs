//! `GET /v2/replay/tx/{signature}` with optional `bonus_accounts`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example replay_from_api -- --signature '<base58>'
//! cargo run --example replay_from_api -- --signature '<base58>' \
//!     --bonus-account <pubkey> --bonus-account <pubkey>
//! ```

use anyhow::{Context, Result};
use circular_replay::{
    print_json_response, print_replay_summary, CircularReplayClient, CircularReplayQuery,
};
use clap::Parser;
use std::path::PathBuf;

const DEFAULT_URL: &str = "https://pro.circular.fi";
const DEFAULT_SIGNATURE: &str =
    "4UAJr3GYEgZGZkxoaxcJBjrKJjn5NEFqnh1aVKddgx1vrbjLLn5swKrmMFqPTe9zvmSThAfCAitkzN22aQxvGicH";

#[derive(Parser)]
#[command(about = "GET /v2/replay/tx (optional bonus accounts)")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = DEFAULT_URL)]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value = DEFAULT_SIGNATURE)]
    signature: String,

    #[arg(long = "bonus-account", value_delimiter = ',')]
    bonus_accounts: Vec<String>,

    #[arg(long = "no-alts", action = clap::ArgAction::SetFalse)]
    alts: bool,

    #[arg(long = "no-spl", action = clap::ArgAction::SetFalse)]
    spl: bool,

    #[arg(long = "no-program-details", action = clap::ArgAction::SetFalse)]
    program_details: bool,

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

    let status = client.status().await.context("GET /v2/replay/status")?;
    eprintln!(
        "replay status: min_query_slot={} max_indexed_slot={}",
        status.get("min_query_slot").map(|v| v.to_string()).unwrap_or_default(),
        status.get("max_indexed_slot").map(|v| v.to_string()).unwrap_or_default(),
    );

    let query = CircularReplayQuery {
        alts: args.alts,
        spl: args.spl,
        program_details: args.program_details,
        bonus_accounts: args.bonus_accounts.clone(),
    };

    eprintln!("--- GET /v2/replay/tx/{{signature}} ---");
    eprintln!("base:            {}", client.base_url());
    eprintln!("bonus_accounts:  {}", args.bonus_accounts.len());
    eprintln!("program_details: {}", args.program_details);

    let snapshot = client
        .replay(&args.signature, &query)
        .await
        .context("GET /v2/replay/tx failed")?;

    print_replay_summary("GET /v2/replay/tx/{signature}", &snapshot);
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
