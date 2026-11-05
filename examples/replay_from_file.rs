//! Load a replay snapshot from a local JSON file (offline, no gateway call).
//!
//! ```bash
//! cargo run --example replay_from_file
//! cargo run --example replay_from_file -- --file fixtures/replay-circular-4UAJr3GYEgZGZkxo.json
//! ```

use anyhow::Result;
use circular_replay::load_snapshot_file;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Inspect a local Circular replay JSON snapshot")]
struct Args {
    /// Path to `replay-circular-*.json` (Pro `/v2/replay/tx` snapshot shape).
    #[arg(long, short, default_value = "fixtures/replay-circular-4UAJr3GYEgZGZkxo.json")]
    file: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let snapshot = load_snapshot_file(&args.file)?;

    println!("--- {} ---", args.file.display());
    println!("signature:  {}", snapshot.signature);
    println!("slot:       {}", snapshot.slot);
    if let Some(epoch) = snapshot.epoch {
        println!("epoch:      {epoch}");
    }
    println!("tx_success: {}", snapshot.tx_success);
    println!(
        "accounts:   pre={} post={} bonus={}",
        snapshot.pre.len(),
        snapshot.post.len(),
        snapshot.bonus_accounts.len(),
    );
    println!(
        "programs:   pre={} post={}",
        snapshot.programs_pre.len(),
        snapshot.programs_post.len(),
    );
    println!(
        "alts:       pre={} post={}",
        snapshot.alts_pre.len(),
        snapshot.alts_post.len(),
    );
    println!(
        "execution:  {}",
        if snapshot.execution.is_some() { "present" } else { "absent" }
    );

    Ok(())
}
