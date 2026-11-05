//! Fetch `GET /v2/replay/tx/{signature}` and save under `fixtures/replay-circular-*.json`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example fetch_replay
//! cargo run --example fetch_replay -- --signature '<base58>'
//! ```

use anyhow::Context;
use circular_replay::{
    fixture_path, print_replay_summary, save_json_pretty, CircularReplayClient, CircularReplayQuery,
};
use clap::Parser;
use std::path::PathBuf;

const DEFAULT_URL: &str = "https://pro.circular.fi";
const DEFAULT_SIGNATURE: &str =
    "4UAJr3GYEgZGZkxoaxcJBjrKJjn5NEFqnh1aVKddgx1vrbjLLn5swKrmMFqPTe9zvmSThAfCAitkzN22aQxvGicH";

#[derive(Parser)]
#[command(about = "GET /v2/replay/tx → save fixture")]
struct Args {
    #[arg(long, env = "CIRCULAR_REPLAY_URL", default_value = DEFAULT_URL)]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(long, default_value = DEFAULT_SIGNATURE)]
    signature: String,

    #[arg(long, default_value_t = true)]
    alts: bool,

    #[arg(long, default_value_t = true)]
    spl: bool,

    #[arg(long, default_value_t = true)]
    program_details: bool,

    #[arg(long, short)]
    output: Option<PathBuf>,

    #[arg(long)]
    no_write: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;
    let query = CircularReplayQuery {
        alts: args.alts,
        spl: args.spl,
        program_details: args.program_details,
        bonus_accounts: Vec::new(),
    };

    eprintln!("--- GET /v2/replay/tx/{{signature}} ---");
    eprintln!("base: {}", client.base_url());

    let snapshot = client
        .replay(&args.signature, &query)
        .await
        .context("replay fetch failed")?;

    print_replay_summary("GET /v2/replay/tx/{signature}", &snapshot);

    if !args.no_write {
        let out = args
            .output
            .clone()
            .unwrap_or_else(|| fixture_path("fixtures", &args.signature));
        save_json_pretty(&out, &snapshot)?;
        eprintln!("wrote snapshot: {}", out.display());
    }

    eprintln!("fetch_replay OK");
    Ok(())
}
