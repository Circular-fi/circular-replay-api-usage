use anyhow::{anyhow, Context, Result};
use base64::Engine;
use litesvm::LiteSVM;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use solana_account::Account;
use solana_address_lookup_table_interface::state::{
    LookupTableMeta, ProgramState, LOOKUP_TABLE_META_SIZE,
};
use solana_loader_v3_interface::state::UpgradeableLoaderState;
use solana_sdk::{bpf_loader, native_loader, pubkey::Pubkey, sysvar};
use solana_sdk_ids::{address_lookup_table, bpf_loader_upgradeable};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Deserialize)]
pub struct ReplaySnapshot {
    pub signature: String,
    pub slot: u64,
    /// Epoch of `slot` as reported by the replayer. `None` on snapshots
    /// captured before the field existed.
    #[serde(default)]
    pub epoch: Option<u64>,
    pub tx_success: bool,
    #[serde(default)]
    pub pre: HashMap<String, AccountSnapshotData>,
    #[serde(default)]
    pub post: HashMap<String, AccountSnapshotData>,
    #[serde(default)]
    pub programs_pre: HashMap<String, ProgramSnapshot>,
    #[serde(default)]
    pub programs_post: HashMap<String, ProgramSnapshot>,
    /// Extra accounts required for execution but not modified by the transaction
    /// (e.g. mints referenced by ATAs).
    #[serde(default)]
    pub bonus_accounts: HashMap<String, AccountSnapshotData>,
    /// Address lookup tables referenced by the transaction (V0).
    #[serde(default)]
    pub alts_pre: HashMap<String, AltSnapshot>,
    #[serde(default)]
    pub alts_post: HashMap<String, AltSnapshot>,
    /// Clock, `LastRestartSlot` and `SlotHashes` seen while the slot executed.
    /// Absent on snapshots taken before the replayer recorded them.
    #[serde(default)]
    pub execution: Option<ExecutionSnapshot>,
    /// Genesis epoch schedule from the checkpoint bank.
    #[serde(default)]
    pub epoch_schedule: Option<EpochScheduleSnapshot>,
    /// Cluster rent parameters from the checkpoint bank.
    #[serde(default)]
    pub rent: Option<RentSnapshot>,
}

/// Mirrors `ExecutionContext` on the replayer response.
#[derive(Debug, Deserialize)]
pub struct ExecutionSnapshot {
    pub unix_timestamp: i64,
    pub epoch_start_timestamp: i64,
    pub leader_schedule_epoch: u64,
    pub last_restart_slot: u64,
    /// Base64 of the `SlotHashes` sysvar account data.
    pub slot_hashes: String,
    /// Base58 blockhash transactions in this slot run with (parent's last blockhash).
    /// Absent on snapshots captured before the replayer recorded it.
    #[serde(default)]
    pub last_blockhash: Option<String>,
    /// Fee rate stored alongside the nonce on advance.
    #[serde(default)]
    pub lamports_per_signature: Option<u64>,
}

/// Mirrors `EpochScheduleSnapshot` on the replayer response.
#[derive(Debug, Deserialize)]
pub struct EpochScheduleSnapshot {
    pub slots_per_epoch: u64,
    pub leader_schedule_slot_offset: u64,
    pub warmup: bool,
    pub first_normal_epoch: u64,
    pub first_normal_slot: u64,
}

/// Mirrors `RentSnapshot` on the replayer response.
#[derive(Debug, Deserialize)]
pub struct RentSnapshot {
    pub lamports_per_byte: u64,
    pub exemption_threshold: f64,
    pub burn_percent: u8,
}

#[derive(Debug, Deserialize)]
pub struct AltSnapshot {
    pub lamports: u64,
    #[serde(default)]
    pub status: String,
    /// Slot deactivation was initiated (`null` when still active).
    #[serde(default)]
    pub deactivation_slot: Option<u64>,
    #[serde(default)]
    pub authority: Option<String>,
    #[serde(default)]
    pub last_extended_slot: u64,
    #[serde(default)]
    pub last_extended_slot_start_index: u64,
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AccountSnapshotData {
    pub lamports: u64,
    #[serde(default)]
    pub data: String,
    pub owner: String,
    pub executable: bool,
    #[serde(deserialize_with = "deserialize_rent_epoch")]
    pub rent_epoch: u64,
    #[serde(default)]
    pub spl: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ProgramSnapshot {
    pub programdata_address: String,
    pub state: AccountSnapshotData,
}

#[derive(Debug, Default, Clone)]
pub struct LoadStats {
    pub total_pre_accounts: usize,
    pub total_bonus_accounts: usize,
    pub total_alts: usize,
    pub regular_accounts: usize,
    pub native_programs_skipped: usize,
    pub builtin_programs_skipped: usize,
    pub sysvars_skipped: usize,
    pub bpf_v2_programs_loaded: usize,
    pub upgradeable_programs_loaded: usize,
    pub upgradeable_programs_missing_data: usize,
    pub alts_loaded: usize,
    pub program_overrides_loaded: usize,
}

fn deserialize_rent_epoch<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                return Ok(u);
            }
            if let Some(f) = n.as_f64() {
                return Ok(if f >= u64::MAX as f64 {
                    u64::MAX
                } else if f < 0.0 {
                    0
                } else {
                    f as u64
                });
            }
            Ok(u64::MAX)
        }
        _ => Ok(0),
    }
}

fn decode_b64(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return Vec::new();
    }
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap_or_default()
}

fn to_account(snap: &AccountSnapshotData) -> Result<Account> {
    let owner = Pubkey::from_str(&snap.owner)
        .with_context(|| format!("invalid owner pubkey: {}", snap.owner))?;
    Ok(Account {
        lamports: snap.lamports,
        data: decode_b64(&snap.data),
        owner,
        executable: snap.executable,
        rent_epoch: snap.rent_epoch,
    })
}

/// Rebuild ALT account binary `data` from an `AltSnapshot`.
///
/// Layout (see `solana_sdk::address_lookup_table::state`):
/// - `LOOKUP_TABLE_META_SIZE` (56) bytes: `ProgramState::LookupTable(meta)` (bincode)
/// - then `addresses.len() * 32` bytes: concatenated pubkeys
fn build_alt_account_data(snap: &AltSnapshot) -> Result<Vec<u8>> {
    let authority = match snap.authority.as_deref() {
        Some(s) if !s.is_empty() => Some(
            Pubkey::from_str(s).with_context(|| format!("invalid ALT authority: {s}"))?,
        ),
        _ => None,
    };

    let deactivation_slot = snap.deactivation_slot.unwrap_or_else(|| {
        // Legacy snapshots without `deactivation_slot`: approximate from status.
        match snap.status.as_str() {
            "deactivated" => 0,
            "deactivating" => 0,
            _ => u64::MAX,
        }
    });

    let addresses: Vec<Pubkey> = snap
        .addresses
        .iter()
        .map(|s| Pubkey::from_str(s).with_context(|| format!("invalid ALT address: {s}")))
        .collect::<Result<_>>()?;

    let meta = LookupTableMeta {
        deactivation_slot,
        last_extended_slot: snap.last_extended_slot,
        last_extended_slot_start_index: snap.last_extended_slot_start_index as u8,
        authority,
        _padding: 0u16,
    };

    let mut data = vec![0u8; LOOKUP_TABLE_META_SIZE + addresses.len() * 32];
    bincode::serialize_into(
        &mut data[..LOOKUP_TABLE_META_SIZE],
        &ProgramState::LookupTable(meta),
    )
    .map_err(|e| anyhow!("failed to serialize ALT meta: {e:?}"))?;

    let mut offset = LOOKUP_TABLE_META_SIZE;
    for addr in &addresses {
        data[offset..offset + 32].copy_from_slice(addr.as_ref());
        offset += 32;
    }
    Ok(data)
}

/// Load all accounts and programs (with ProgramData) from the snapshot file's `pre`
/// section into `svm`.
///
/// - Native accounts (owner = NativeLoader) are skipped (already provided by LiteSVM).
/// - BPF Loader v2 programs register ELF from `data` via `add_program`.
/// - Upgradeable programs load ProgramData from `programs_pre` via `set_account`, then
///   register ELF after the 45-byte header via `add_program`.
pub fn load_pre_state<P: AsRef<Path>>(svm: &mut LiteSVM, replay_path: P) -> Result<LoadStats> {
    let path_ref = replay_path.as_ref();
    let content = std::fs::read_to_string(path_ref)
        .with_context(|| format!("failed to read replay file: {}", path_ref.display()))?;
    let snapshot: ReplaySnapshot = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse replay JSON: {}", path_ref.display()))?;
    load_pre_into_svm(svm, &snapshot)
}

/// Same as [`load_pre_state`] but takes an in-memory [`ReplaySnapshot`].
pub fn load_pre_into_svm(svm: &mut LiteSVM, snapshot: &ReplaySnapshot) -> Result<LoadStats> {
    let mut stats = LoadStats {
        total_pre_accounts: snapshot.pre.len(),
        total_bonus_accounts: snapshot.bonus_accounts.len(),
        total_alts: snapshot.alts_pre.len(),
        ..Default::default()
    };

    let bpf_upgradeable_id = bpf_loader_upgradeable::id();
    let bpf_loader_v2_id = bpf_loader::id();
    let native_loader_id = native_loader::id();
    // Sysvar program owns all sysvars; LiteSVM manages them with validated binary layouts.
    let sysvar_owner_id = sysvar::id();

    // Phase 1: load upgradeable ProgramData before Program accounts that reference them
    // (otherwise LiteSVM `set_account` on the Program fails with `MissingAccount`).
    for prog_entry in snapshot.programs_pre.values() {
        let programdata_pubkey = Pubkey::from_str(&prog_entry.programdata_address)
            .with_context(|| {
                format!(
                    "invalid programdata pubkey: {}",
                    prog_entry.programdata_address
                )
            })?;
        let pd_account = to_account(&prog_entry.state)?;
        svm.set_account(programdata_pubkey, pd_account)
            .map_err(|e| anyhow!("set_account failed for programdata {programdata_pubkey}: {e:?}"))?;
    }

    // Phase 1b: load ALTs; data is rebuilt from `AltSnapshot` and owned by the ALT program.
    let alt_program_id = address_lookup_table::id();
    for (pubkey_str, alt) in &snapshot.alts_pre {
        let pubkey = Pubkey::from_str(pubkey_str)
            .with_context(|| format!("invalid ALT pubkey: {pubkey_str}"))?;
        let data = build_alt_account_data(alt)?;
        let account = Account {
            lamports: alt.lamports,
            data,
            owner: alt_program_id,
            executable: false,
            rent_epoch: u64::MAX,
        };
        svm.set_account(pubkey, account)
            .map_err(|e| anyhow!("set_account failed for ALT {pubkey}: {e:?}"))?;
        stats.alts_loaded += 1;
    }

    // Phase 2: `pre` then `bonus_accounts` (same eligibility rules).
    let chained = snapshot
        .pre
        .iter()
        .chain(snapshot.bonus_accounts.iter());

    for (pubkey_str, snap) in chained {
        let pubkey =
            Pubkey::from_str(pubkey_str).with_context(|| format!("invalid pubkey: {pubkey_str}"))?;
        let account = to_account(snap)?;

        if account.owner == native_loader_id {
            stats.native_programs_skipped += 1;
            continue;
        }

        // Sysvars: LiteSVM manages them internally; `set_account` would fail (e.g. `InvalidSysvarData(Rent)`).
        if account.owner == sysvar_owner_id {
            stats.sysvars_skipped += 1;
            continue;
        }

        // LiteSVM preloaded builtins (SPL Token, ATA, Token-2022, etc.): snapshot has
        // `executable=true` but empty `data`, which would fail `set_account`; skip them.
        if account.executable && account.data.is_empty() {
            stats.builtin_programs_skipped += 1;
            continue;
        }

        svm.set_account(pubkey, account.clone())
            .map_err(|e| anyhow!("set_account failed for {pubkey}: {e:?}"))?;

        if !account.executable {
            stats.regular_accounts += 1;
            continue;
        }

        if account.owner == bpf_loader_v2_id {
            svm.add_program(pubkey, &account.data)
                .map_err(|e| anyhow!("add_program failed for {pubkey}: {e:?}"))?;
            stats.bpf_v2_programs_loaded += 1;
            continue;
        }

        if account.owner == bpf_upgradeable_id {
            let Some(prog_entry) = snapshot.programs_pre.get(pubkey_str) else {
                stats.upgradeable_programs_missing_data += 1;
                continue;
            };
            let pd_account = to_account(&prog_entry.state)?;

            let header_size = UpgradeableLoaderState::size_of_programdata_metadata();
            if pd_account.data.len() <= header_size {
                stats.upgradeable_programs_missing_data += 1;
                continue;
            }
            let elf = &pd_account.data[header_size..];
            svm.add_program(pubkey, elf)
                .map_err(|e| anyhow!("add_program failed for {pubkey}: {e:?}"))?;
            stats.upgradeable_programs_loaded += 1;
        }
    }

    // Phase 3: override LiteSVM bundled program ELFs with mainnet `.so` files (see
    // `load_program_overrides`). Applied last so it wins over bundled and snapshot stubs.
    stats.program_overrides_loaded = load_program_overrides(svm, snapshot);

    Ok(stats)
}

/// Replace LiteSVM bundled programs with mainnet `.so` files under `REPLAY_PROGRAMS_DIR`
/// (default: crate `programs/`).
///
/// Each `*.so` is loaded under the pubkey derived from its filename (stem must be base58).
/// Token, Token-2022, Memo, etc. rarely upgrade; refresh with:
/// `solana program dump <ID> <ID>.so --url mainnet-beta`.
///
/// If the snapshot marks a program as BPF Loader v2, reload with that loader (account
/// bytes are the ELF; CPI charges `len / 250`). Bundled LiteSVM Memo is much smaller
/// than mainnet (~290 CU difference). Other overrides use the upgradeable loader via
/// `add_program`.
///
/// Returns 0 if the directory is missing (no error).
pub fn load_program_overrides(svm: &mut LiteSVM, snapshot: &ReplaySnapshot) -> usize {
    let dir = std::env::var("REPLAY_PROGRAMS_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs")
    });
    let dir = dir.as_path();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    let mut loaded = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("so") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(program_id) = Pubkey::from_str(stem) else {
            eprintln!("⚠️  skipped program override (filename is not a pubkey): {}", path.display());
            continue;
        };
        let loader_v2 = snapshot.pre.get(stem).is_some_and(|acc| {
            Pubkey::from_str(&acc.owner).ok() == Some(bpf_loader::id())
        });
        match std::fs::read(&path) {
            Ok(bytes) => {
                let result = if loader_v2 {
                    svm.add_program_with_loader(program_id, &bytes, bpf_loader::id())
                } else {
                    svm.add_program(program_id, &bytes)
                };
                match result {
                Ok(()) => {
                    println!("🔧 program override: {program_id} ({} bytes)", bytes.len());
                    loaded += 1;
                }
                Err(e) => eprintln!("⚠️  override {program_id} failed: {e:?}"),
                }
            }
            Err(e) => eprintln!("⚠️  failed to read {}: {e}", path.display()),
        }
    }
    loaded
}
