//! Replay a transaction on [LiteSVM](https://docs.rs/litesvm) from a gateway snapshot.
//!
//! - sigverify OFF (real signatures are not available)
//! - blockhash_check OFF; when `execution.last_blockhash` is present it is used as the message blockhash
//! - `Clock`, `SlotHashes`, `LastRestartSlot`, `EpochSchedule`, and `Rent` come from the snapshot
//!   (`execution`, `epoch_schedule`, `rent`) — no RPC during setup
//! - feature gates: LiteSVM's mainnet set plus `EXTRA_MAINNET_FEATURES`, set before loading programs

use crate::replay::snapshot::{
    EpochScheduleSnapshot, ExecutionSnapshot, LoadStats, RentSnapshot, ReplaySnapshot,
    load_pre_into_svm,
};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use litesvm::LiteSVM;
use solana_sdk::{
    clock::Clock,
    epoch_schedule::EpochSchedule,
    hash::Hash,
    pubkey::Pubkey,
    rent::Rent,
    signature::Signature,
    slot_hashes::SlotHashes,
    sysvar::last_restart_slot::LastRestartSlot,
    transaction::VersionedTransaction,
};
use std::str::FromStr;

const EXTRA_MAINNET_FEATURES: &[Pubkey] = &[];

fn replay_feature_set() -> agave_feature_set::FeatureSet {
    let mut feature_set = LiteSVM::mainnet_feature_set();
    for id in EXTRA_MAINNET_FEATURES {
        feature_set.activate(id, 0);
    }
    feature_set
}

pub struct SvmReplayOutcome {
    pub success: bool,
    pub error: Option<String>,
    pub logs: Vec<String>,
    pub compute_units: Option<u64>,
}

/// SVM configured for replay (no signature or blockhash checks).
pub fn new_replay_svm() -> LiteSVM {
    LiteSVM::default()
        .with_feature_set(replay_feature_set())
        .with_precompiles()
        .with_builtins()
        .with_sysvars()
        .with_default_programs()
        .with_sigverify(false)
        .with_blockhash_check(false)
}

fn epoch_schedule_from_snapshot(schedule: &EpochScheduleSnapshot) -> EpochSchedule {
    EpochSchedule {
        slots_per_epoch: schedule.slots_per_epoch,
        leader_schedule_slot_offset: schedule.leader_schedule_slot_offset,
        warmup: schedule.warmup,
        first_normal_epoch: schedule.first_normal_epoch,
        first_normal_slot: schedule.first_normal_slot,
    }
}

#[allow(deprecated)]
fn rent_from_snapshot(rent: &RentSnapshot) -> Rent {
    Rent {
        lamports_per_byte: rent.lamports_per_byte,
        exemption_threshold: rent.exemption_threshold.to_le_bytes(),
        burn_percent: rent.burn_percent,
    }
}

fn decode_slot_hashes(encoded: &str) -> Result<SlotHashes> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("slot_hashes is not valid base64")?;
    wincode_v1::deserialize(&bytes).context("slot_hashes sysvar did not decode")
}

fn apply_execution_context(
    svm: &mut LiteSVM,
    snapshot: &ReplaySnapshot,
    exec: &ExecutionSnapshot,
    epoch_schedule: &EpochSchedule,
) -> Result<()> {
    let epoch = snapshot
        .epoch
        .unwrap_or_else(|| epoch_schedule.get_epoch(snapshot.slot));
    let slot_hashes = decode_slot_hashes(&exec.slot_hashes)
        .with_context(|| format!("slot {} execution.slot_hashes", snapshot.slot))?;
    svm.set_sysvar(&Clock {
        slot: snapshot.slot,
        epoch_start_timestamp: exec.epoch_start_timestamp,
        epoch,
        leader_schedule_epoch: exec.leader_schedule_epoch,
        unix_timestamp: exec.unix_timestamp,
    });
    svm.set_sysvar(epoch_schedule);
    svm.set_sysvar(&slot_hashes);
    svm.set_sysvar(&LastRestartSlot {
        last_restart_slot: exec.last_restart_slot,
    });
    Ok(())
}

fn ensure_snapshot_context(snapshot: &ReplaySnapshot) -> Result<(&ExecutionSnapshot, EpochSchedule, Rent)> {
    let exec = snapshot
        .execution
        .as_ref()
        .ok_or_else(|| anyhow!("snapshot missing `execution`; re-fetch from a current replayer"))?;
    let schedule = snapshot
        .epoch_schedule
        .as_ref()
        .ok_or_else(|| anyhow!("snapshot missing `epoch_schedule`"))?;
    let rent = snapshot
        .rent
        .as_ref()
        .ok_or_else(|| anyhow!("snapshot missing `rent`"))?;
    Ok((
        exec,
        epoch_schedule_from_snapshot(schedule),
        rent_from_snapshot(rent),
    ))
}

/// Prepare the SVM from snapshot fields only (no RPC). Requires `execution`, `epoch_schedule`, and `rent`.
pub fn setup_svm_from_snapshot(
    svm: &mut LiteSVM,
    snapshot: &ReplaySnapshot,
) -> Result<LoadStats> {
    let (exec, epoch_schedule, rent) = ensure_snapshot_context(snapshot)?;
    apply_execution_context(svm, snapshot, exec, &epoch_schedule)?;
    svm.set_sysvar(&rent);
    load_pre_into_svm(svm, snapshot)
}

/// Sets the message blockhash for local execution. When the snapshot carries
/// `execution.last_blockhash` (parent blockhash seen by txs in that slot),
/// use it so durable-nonce advances match chain state.
pub fn prepare_tx_for_svm(
    tx: &mut VersionedTransaction,
    svm: &LiteSVM,
    snapshot: &ReplaySnapshot,
) {
    let blockhash = snapshot
        .execution
        .as_ref()
        .and_then(|exec| exec.last_blockhash.as_deref())
        .and_then(|s| Hash::from_str(s).ok())
        .unwrap_or_else(|| svm.latest_blockhash());
    tx.message.set_recent_blockhash(blockhash);
    let n_sigs = tx.signatures.len();
    tx.signatures = vec![Signature::default(); n_sigs];
}

pub fn run_on_svm(svm: &mut LiteSVM, tx: VersionedTransaction) -> SvmReplayOutcome {
    match svm.send_transaction(tx) {
        Ok(meta) => SvmReplayOutcome {
            success: true,
            error: None,
            logs: meta.logs,
            compute_units: Some(meta.compute_units_consumed),
        },
        Err(failed) => SvmReplayOutcome {
            success: false,
            error: Some(format!("{:?}", failed.err)),
            logs: failed.meta.logs,
            compute_units: Some(failed.meta.compute_units_consumed),
        },
    }
}

pub fn parse_signature(s: &str) -> Result<Signature> {
    Signature::from_str(s).with_context(|| format!("invalid signature: {s}"))
}
