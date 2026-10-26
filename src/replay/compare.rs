use crate::replay::litesvm::SvmReplayOutcome;
use litesvm::LiteSVM;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use solana_transaction_status_client_types::{
    option_serializer::OptionSerializer, UiTransactionStatusMeta,
};
use std::collections::BTreeSet;
use std::str::FromStr;

#[derive(Debug, Default)]
pub struct CompareReport {
    pub checks: Vec<CheckResult>,
}

#[derive(Debug)]
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

impl CompareReport {
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    pub fn push(&mut self, name: &'static str, passed: bool, detail: impl Into<String>) {
        self.checks.push(CheckResult {
            name,
            passed,
            detail: detail.into(),
        });
    }

    pub fn print_summary(&self) {
        println!("\n═══ Replay compare summary ═══");
        for c in &self.checks {
            let mark = if c.passed { "PASS" } else { "FAIL" };
            println!("  [{mark}] {} — {}", c.name, c.detail);
        }
        if self.all_passed() {
            println!("\n✅ All checks passed.");
        } else {
            println!("\n❌ Some checks failed.");
        }
    }
}

pub fn compare_rpc_vs_svm(
    report: &mut CompareReport,
    rpc_meta: &UiTransactionStatusMeta,
    svm: &SvmReplayOutcome,
    snapshot_tx_success: bool,
) {
    let rpc_ok = rpc_meta.err.is_none();
    report.push(
        "rpc_success",
        rpc_ok,
        format!("on-chain err={:?}", rpc_meta.err),
    );

    report.push(
        "svm_success",
        svm.success,
        svm.error
            .clone()
            .unwrap_or_else(|| "ok".into()),
    );

    report.push(
        "success_rpc_vs_svm",
        rpc_ok == svm.success,
        format!("rpc_ok={rpc_ok}, svm_ok={}", svm.success),
    );

    report.push(
        "success_snapshot_tx_success",
        snapshot_tx_success == rpc_ok,
        format!("snapshot={snapshot_tx_success}, rpc_ok={rpc_ok}"),
    );

    let rpc_logs: Vec<String> = match &rpc_meta.log_messages {
        OptionSerializer::Some(v) => v.clone(),
        _ => Vec::new(),
    };
    let logs_match = rpc_logs == svm.logs;
    report.push(
        "logs_rpc_vs_svm",
        logs_match,
        if logs_match {
            format!("{} log lines match", rpc_logs.len())
        } else {
            format!(
                "rpc={} lines, svm={} lines (see diff below)",
                rpc_logs.len(),
                svm.logs.len()
            )
        },
    );

    if !logs_match {
        print_log_diff(&rpc_logs, &svm.logs);
    }

    let rpc_cu = match &rpc_meta.compute_units_consumed {
        OptionSerializer::Some(cu) => Some(*cu),
        _ => None,
    };
    let cu_match = rpc_cu == svm.compute_units;
    report.push(
        "compute_units_rpc_vs_svm",
        cu_match,
        format!("rpc={rpc_cu:?}, svm={:?}", svm.compute_units),
    );

    let inner_rpc = match &rpc_meta.inner_instructions {
        OptionSerializer::Some(v) => v.len(),
        _ => 0,
    };
    report.push(
        "inner_instructions_rpc_count",
        true,
        format!("{inner_rpc} top-level inner instruction groups (informational)"),
    );
}

/// Compare on-chain accounts/programs touched by the tx with what is loaded in LiteSVM
/// after the snapshot is applied.
///
/// Call AFTER `setup_svm_from_snapshot` and BEFORE `run_on_svm` (which consumes the tx).
/// A missing account or unloaded program is the most common cause of an on-SVM
/// `Custom(...)` error that did not occur on-chain.
pub fn compare_loaded_accounts(
    svm: &LiteSVM,
    tx: &VersionedTransaction,
    rpc_meta: &UiTransactionStatusMeta,
) {
    // --- 1. Build the set of accounts required on-chain ---
    // Order: static keys, then writable ALT addresses, then readonly ALT addresses.
    let mut required: Vec<(Pubkey, &'static str)> = Vec::new();
    let mut seen: BTreeSet<Pubkey> = BTreeSet::new();

    let push = |pk: Pubkey, kind: &'static str, v: &mut Vec<(Pubkey, &'static str)>, s: &mut BTreeSet<Pubkey>| {
        if s.insert(pk) {
            v.push((pk, kind));
        }
    };

    for k in tx.message.static_account_keys() {
        push(*k, "static", &mut required, &mut seen);
    }
    if let OptionSerializer::Some(loaded) = &rpc_meta.loaded_addresses {
        for s in &loaded.writable {
            if let Ok(pk) = Pubkey::from_str(s) {
                push(pk, "alt-w", &mut required, &mut seen);
            }
        }
        for s in &loaded.readonly {
            if let Ok(pk) = Pubkey::from_str(s) {
                push(pk, "alt-r", &mut required, &mut seen);
            }
        }
    }

    println!("\n═══ Required accounts (on-chain) vs loaded (LiteSVM) ═══");
    let mut missing: Vec<Pubkey> = Vec::new();
    let mut empty: Vec<Pubkey> = Vec::new();
    for (pk, kind) in &required {
        match svm.get_account(pk) {
            Some(acc) => {
                if acc.lamports == 0 && acc.data.is_empty() {
                    println!("  [EMPTY] {pk} ({kind})  owner={} lamports=0 data=0B", acc.owner);
                    empty.push(*pk);
                } else {
                    println!(
                        "  [OK  ] {pk} ({kind})  owner={} data={}B exec={} lamports={}",
                        acc.owner,
                        acc.data.len(),
                        acc.executable,
                        acc.lamports
                    );
                }
            }
            None => {
                println!("  [MISS] {pk} ({kind})  ← not loaded in SVM");
                missing.push(*pk);
            }
        }
    }
    println!(
        "\n  → {} required | {} missing | {} empty",
        required.len(),
        missing.len(),
        empty.len()
    );

    // --- 2. Invoked programs (parse RPC logs "Program <id> invoke") ---
    let rpc_logs: Vec<String> = match &rpc_meta.log_messages {
        OptionSerializer::Some(v) => v.clone(),
        _ => Vec::new(),
    };
    let mut programs: BTreeSet<Pubkey> = BTreeSet::new();
    for line in &rpc_logs {
        if let Some(rest) = line.strip_prefix("Program ") {
            if rest.contains(" invoke [") {
                let id = rest.split(" invoke [").next().unwrap_or("").trim();
                if let Ok(pk) = Pubkey::from_str(id) {
                    programs.insert(pk);
                }
            }
        }
    }

    if !programs.is_empty() {
        println!("\n═══ Invoked programs (RPC logs) vs loaded (LiteSVM) ═══");
        let mut prog_missing = 0usize;
        for pk in &programs {
            match svm.get_account(pk) {
                Some(acc) if acc.executable => {
                    println!("  [OK     ] {pk}  owner={} data={}B", acc.owner, acc.data.len());
                }
                Some(acc) => {
                    println!(
                        "  [NONEXEC] {pk}  present but not executable (owner={} data={}B)",
                        acc.owner,
                        acc.data.len()
                    );
                    prog_missing += 1;
                }
                None => {
                    println!("  [MISS   ] {pk}  ← program not loaded");
                    prog_missing += 1;
                }
            }
        }
        println!(
            "\n  → {} invoked programs | {} missing/non-executable",
            programs.len(),
            prog_missing
        );
    }
}

fn print_log_diff(rpc: &[String], svm: &[String]) {
    // Separate blocks (no interleaving) so each side is easy to read / copy.
    let first_divergence = rpc
        .iter()
        .zip(svm.iter())
        .position(|(r, s)| r != s)
        .or_else(|| {
            if rpc.len() != svm.len() {
                Some(rpc.len().min(svm.len()))
            } else {
                None
            }
        });

    println!("\n─── RPC logs ({} lines) ───", rpc.len());
    for (i, line) in rpc.iter().enumerate() {
        let marker = if Some(i) == first_divergence { ">>" } else { "  " };
        println!("{marker}[{i:>3}] {line}");
    }

    println!("\n─── LiteSVM logs ({} lines) ───", svm.len());
    for (i, line) in svm.iter().enumerate() {
        let marker = if Some(i) == first_divergence { ">>" } else { "  " };
        println!("{marker}[{i:>3}] {line}");
    }

    if let Some(idx) = first_divergence {
        println!("\n⚠️  First divergence at log index {idx}:");
        println!("    RPC: {}", rpc.get(idx).map(String::as_str).unwrap_or("<none>"));
        println!("    SVM: {}", svm.get(idx).map(String::as_str).unwrap_or("<none>"));
    }
}
