//! Well-known mainnet program IDs for LiteSVM `.so` overrides in `programs/`.
//!
//! Filenames must be `{pubkey}.so`. Refresh with:
//! `solana program dump <ID> programs/<ID>.so --url mainnet-beta`

/// SPL Token (classic).
pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

/// SPL Token-2022.
pub const SPL_TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// SPL Memo (BPF Loader v2 on mainnet — load with `add_program_with_loader` when owner is v2).
pub const SPL_MEMO_PROGRAM: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

/// Typical ELF sizes after `solana program dump` (sanity checks / docs).
pub const SPL_TOKEN_ELF_BYTES: u64 = 108_600;
pub const SPL_TOKEN_2022_ELF_BYTES: u64 = 1_382_016;
pub const SPL_MEMO_ELF_BYTES: u64 = 74_800;

/// Default set shipped under `programs/` for bit-exact CPI / rent on common swaps.
pub const DEFAULT_OVERRIDE_PROGRAMS: &[&str] = &[
    SPL_TOKEN_PROGRAM,
    SPL_TOKEN_2022_PROGRAM,
    SPL_MEMO_PROGRAM,
];
