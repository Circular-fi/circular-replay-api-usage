use anyhow::{anyhow, Result};
use solana_sdk::transaction::VersionedTransaction;

/// First byte of a v1 transaction on the wire (`MESSAGE_VERSION_PREFIX | 1`).
pub const V1_TX_WIRE_PREFIX: u8 = 0x81;

/// Decode RPC `getTransaction` base64 bytes into a [`VersionedTransaction`].
pub fn decode_versioned_transaction(bytes: &[u8]) -> Result<VersionedTransaction> {
    if bytes.first() == Some(&V1_TX_WIRE_PREFIX) {
        wincode_v1::deserialize(bytes).map_err(|e| anyhow!("v1 transaction decode: {e}"))
    } else {
        bincode::deserialize(bytes).map_err(|e| anyhow!("legacy/v0 transaction decode: {e}"))
    }
}

pub fn message_version_label(tx: &VersionedTransaction) -> &'static str {
    use solana_sdk::message::VersionedMessage;
    match &tx.message {
        VersionedMessage::Legacy(_) => "legacy",
        VersionedMessage::V0(_) => "v0",
        VersionedMessage::V1(_) => "v1",
    }
}
