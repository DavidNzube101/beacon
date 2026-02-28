use anyhow::{Result, Context};
use ethers::prelude::*;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

const BASE_USDC: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const SOLANA_USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

pub async fn verify_payment(
    chain: &str,
    txn_hash: &str,
    expected_amount: f64,
    expected_address: &str,
) -> Result<bool> {
    match chain {
        "base" => verify_base(txn_hash, expected_amount, expected_address).await,
        "solana" => verify_solana(txn_hash, expected_amount, expected_address).await,
        _ => anyhow::bail!("Unsupported chain: {}", chain),
    }
}

async fn verify_base(
    txn_hash: &str,
    expected_amount: f64,
    expected_address: &str,
) -> Result<bool> {
    let rpc_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| "https://mainnet.base.org".to_string());
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let hash = H256::from_str(txn_hash).context("Invalid Base txn hash")?;

    let receipt = provider.get_transaction_receipt(hash).await?
        .context("Base transaction receipt not found")?;

    if receipt.status != Some(1.into()) {
        return Ok(false);
    }

    let usdc_addr = Address::from_str(BASE_USDC)?;
    let receiver_addr = Address::from_str(expected_address)?;
    let transfer_topic = H256::from_str("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")?;

    for log in receipt.logs {
        if log.address == usdc_addr && log.topics.len() == 3 && log.topics[0] == transfer_topic {
            let to = Address::from_slice(&log.topics[2][12..]);
            if to == receiver_addr {
                let value = U256::from_big_endian(&log.data);
                let amount_f64 = value.as_u128() as f64 / 1_000_000.0;
                if (amount_f64 - expected_amount).abs() < 0.001 {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

async fn verify_solana(
    txn_hash: &str,
    expected_amount: f64,
    expected_address: &str,
) -> Result<bool> {
    let rpc_url = std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let client = RpcClient::new(rpc_url);
    let signature = solana_sdk::signature::Signature::from_str(txn_hash).context("Invalid Solana signature")?;

    let tx = client.get_transaction(&signature, solana_client::rpc_config::RpcTransactionConfig {
        encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    })?;

    let meta = tx.transaction.meta.context("Transaction meta not found")?;
    let recipient_pubkey = Pubkey::from_str(expected_address)?;
    let pre_balances = meta.pre_token_balances.context("Pre-balances missing")?;
    let post_balances = meta.post_token_balances.context("Post-balances missing")?;

    let pre_val = pre_balances.iter()
        .find(|b| b.mint == SOLANA_USDC && b.owner == Some(recipient_pubkey.to_string()))
        .and_then(|b| b.ui_token_amount.ui_amount)
        .unwrap_or(0.0);

    let post_val = post_balances.iter()
        .find(|b| b.mint == SOLANA_USDC && b.owner == Some(recipient_pubkey.to_string()))
        .and_then(|b| b.ui_token_amount.ui_amount)
        .unwrap_or(0.0);

    if (post_val - pre_val - expected_amount).abs() < 0.001 {
        return Ok(true);
    }

    Ok(false)
}
