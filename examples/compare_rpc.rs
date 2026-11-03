//! Circular `GET /v1/replay`, re-execute in LiteSVM, compare with `getTransaction`.
//!
//! ```bash
//! export CIRCULAR_REPLAY_API_KEY=your_key
//! cargo run --example compare_rpc -- --signature '<base58>'
//! cargo run --example compare_rpc -- -f fixtures/replay-circular-4HqcGERuLJKvnKda.json
//! ```

use anyhow::{anyhow, Context, Result};
use circular_replay::{
    compare_loaded_accounts, compare_rpc_vs_svm, decode_tx_from_rpc_response, fetch_confirmed_tx,
    load_snapshot_file, message_version_label, new_replay_svm, parse_signature, prepare_tx_for_svm,
    run_on_svm, setup_svm_from_snapshot, CircularReplayClient, CircularReplayQuery, CompareReport,
    ReplaySnapshot,
};
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use std::path::PathBuf;

const DEFAULT_SIGNATURE: &str =
    "4HqcGERuLJKvnKdaCPW5zjbhLf87tfXErJLhTxvRmneYnqQd1JVM2XtWsUPb5dcKPBWRMPDmW3R1NB1ymhkZPCF6";

#[derive(Parser, Debug)]
#[command(
    name = "compare_rpc",
    about = "Circular replay snapshot → LiteSVM vs getTransaction RPC"
)]
struct Args {
    #[arg(long, default_value = DEFAULT_SIGNATURE)]
    signature: String,

    #[arg(long, short = 'f')]
    file: Option<PathBuf>,

    #[arg(
        long,
        env = "CIRCULAR_REPLAY_URL",
        default_value = "http://localhost:8080"
    )]
    circular_url: String,

    #[arg(long, env = "CIRCULAR_REPLAY_API_KEY")]
    circular_api_key: Option<String>,

    #[arg(long, env = "RPC_URL", default_value = "https://api.mainnet-beta.solana.com")]
    rpc_url: String,

    #[arg(long, default_value_t = true)]
    alts: bool,

    #[arg(long, default_value_t = true)]
    spl: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let (snapshot, source_label) = load_snapshot(&args).await?;

    println!("📄 source    : {source_label}");
    println!("📄 signature : {}", snapshot.signature);
    println!("📄 slot      : {}", snapshot.slot);
    if let Some(epoch) = snapshot.epoch {
        println!("📄 epoch     : {epoch}");
    }
    println!("📄 tx_success: {}", snapshot.tx_success);

    let rpc = RpcClient::new_with_commitment(args.rpc_url.clone(), CommitmentConfig::confirmed());

    let mut svm = new_replay_svm();
    setup_svm_from_snapshot(&mut svm, &snapshot)?;
    println!("📦 LiteSVM pre-state loaded");

    let sig = parse_signature(&snapshot.signature)?;
    let tx_resp = fetch_confirmed_tx(&rpc, &sig)?;
    let rpc_meta = tx_resp
        .transaction
        .meta
        .as_ref()
        .ok_or_else(|| anyhow!("getTransaction returned no meta for {}", snapshot.signature))?;

    let mut versioned_tx = decode_tx_from_rpc_response(&tx_resp)?;
    println!(
        "🌐 RPC tx: version={}, signers={}, ixs={}",
        message_version_label(&versioned_tx),
        versioned_tx.signatures.len(),
        versioned_tx.message.instructions().len()
    );

    compare_loaded_accounts(&svm, &versioned_tx, rpc_meta);

    prepare_tx_for_svm(&mut versioned_tx, &svm, &snapshot);
    let svm_out = run_on_svm(&mut svm, versioned_tx);

    let mut report = CompareReport::default();
    compare_rpc_vs_svm(&mut report, rpc_meta, &svm_out, snapshot.tx_success);
    report.print_summary();

    if !report.all_passed() {
        std::process::exit(1);
    }
    Ok(())
}

async fn load_snapshot(args: &Args) -> Result<(ReplaySnapshot, String)> {
    if let Some(path) = &args.file {
        let snap = load_snapshot_file(path)?;
        return Ok((snap, format!("file:{}", path.display())));
    }

    let api_key = args
        .circular_api_key
        .as_deref()
        .ok_or_else(|| anyhow!("CIRCULAR_REPLAY_API_KEY is required to fetch from Circular API"))?;

    let client = CircularReplayClient::new(&args.circular_url, api_key)?;
    let query = CircularReplayQuery {
        alts: args.alts,
        spl: args.spl,
    };

    let value = client
        .replay(&args.signature, &query)
        .await
        .context("Circular replay fetch failed")?;

    let snap: ReplaySnapshot = serde_json::from_value(value)
        .context("Circular replay response is not a valid ReplaySnapshot")?;

    Ok((
        snap,
        format!(
            "circular:{}/v1/replay/{{sig}} (alts={}, spl={})",
            args.circular_url.trim_end_matches('/'),
            args.alts,
            args.spl,
        ),
    ))
}
