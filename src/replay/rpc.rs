use anyhow::{anyhow, Context, Result};
use base64::Engine;
use crate::replay::tx::decode_versioned_transaction;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcTransactionConfig};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{signature::Signature, transaction::VersionedTransaction};
use solana_transaction_status_client_types::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiTransactionEncoding,
};

pub fn fetch_confirmed_tx(
    rpc: &RpcClient,
    signature: &Signature,
) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
    let cfg = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Base64),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(1),
    };
    rpc.get_transaction_with_config(signature, cfg)
        .with_context(|| format!("getTransaction failed for {signature}"))
}

pub fn decode_tx_from_rpc_response(
    resp: &EncodedConfirmedTransactionWithStatusMeta,
) -> Result<VersionedTransaction> {
    match &resp.transaction.transaction {
        EncodedTransaction::Binary(data, _enc) => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
            decode_versioned_transaction(&bytes)
        }
        other => Err(anyhow!("unexpected RPC transaction encoding: {other:?}")),
    }
}
