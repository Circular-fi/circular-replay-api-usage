//! Fetch `GET /v1/replay/{signature}` and save the full replay snapshot JSON under
//! `fixtures/` (pre/post, programs, execution, etc.).
//!
//! By default writes `fixtures/replay-circular-<first 16 chars of signature>.json`.
//! Use that file with `compare_rpc -- -f fixtures/replay-circular-….json`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example fetch_replay
//! cargo run --example fetch_replay -- --signature '<base58>'
//! cargo run --example fetch_replay -- -o fixtures/my-replay.json
//! cargo run --example fetch_replay -- --no-write   # fetch only, no file
//! ```

use anyhow::Context;
use circular_replay::{CircularReplayClient, CircularReplayQuery, fixture_path, print_replay_summary, save_json_pretty};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Circular GET /v1/replay → save fixture")]
struct Args {
    #[arg(
        long,
        env = "CIRCULAR_REPLAY_URL",
        default_value = "http://localhost:8080"
    )]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    api_key: String,

    #[arg(
        long,
        default_value = "4HqcGERuLJKvnKdaCPW5zjbhLf87tfXErJLhTxvRmneYnqQd1JVM2XtWsUPb5dcKPBWRMPDmW3R1NB1ymhkZPCF6"
    )]
    signature: String,

    #[arg(long, default_value_t = true)]
    alts: bool,

    #[arg(long, default_value_t = true)]
    spl: bool,

    #[arg(long, short)]
    output: Option<PathBuf>,

    #[arg(long)]
    no_write: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();

    let client = CircularReplayClient::new(&args.circular_url, &args.api_key)?;
    let query = CircularReplayQuery {
        alts: args.alts,
        spl: args.spl,
    };

    eprintln!("--- GET /v1/replay/{{signature}} ---");
    eprintln!("base: {}", client.base_url());

    let snapshot = client
        .replay(&args.signature, &query)
        .await
        .context("Circular replay fetch failed")?;

    print_replay_summary("GET /v1/replay/{signature}", &snapshot);

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
