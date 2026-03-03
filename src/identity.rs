use anyhow::{Result, Context};
use ethers_core::types::{Address, Bytes};
use ethers_contract::abigen;
use ethers_providers::{Provider, Http, Middleware};
use ethers_signers::{LocalWallet, Signer};
use ethers_middleware::SignerMiddleware;
use std::sync::Arc;
use std::fs;
use std::path::Path;

abigen!(
    IERC7527Agency,
    r#"[
        function getWrapOracle(bytes data) external view returns (uint256 premium, uint256 fee)
        function wrap(address to, bytes data) external payable returns (uint256 tokenId)
    ]"#
);

const BASE_RPC: &str = "https://mainnet.base.org";

const CANONICAL_AGENCY: &str = "0xd8b934580fcE35a11B58C6D73aDeE468a2833fa8";

pub async fn register_agent_identity(repo_path: &str, _chain: &str, agency_address: Option<&str>) -> Result<()> {
    let agency_addr_str = agency_address.unwrap_or(CANONICAL_AGENCY);
    let agency_addr = agency_addr_str.parse::<Address>()?;
    
    let provider_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| BASE_RPC.to_string());
    let provider = Provider::<Http>::try_from(provider_url)?;
    
    let wallet = match std::env::var("AGENT_PRIVATE_KEY") {
        Ok(key) => key.parse::<LocalWallet>()?,
        Err(_) => {
            let new_wallet = LocalWallet::new(&mut rand::thread_rng());
            println!("   🔑 Generated new agent wallet: {:?}", new_wallet.address());
            println!("   ⚠️  SAVE THIS PRIVATE KEY: 0x{}", hex::encode(new_wallet.signer().to_bytes()));
            new_wallet
        }
    };

    let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.with_chain_id(8453u64)));
    let address = client.address();
    let agency = IERC7527Agency::new(agency_addr, client.clone());

    let repo_url = get_repo_url(repo_path).context("Could not find repository URL in .git/config")?;
    let data = Bytes::from(repo_url.as_bytes().to_vec());

    let (premium, fee) = agency.get_wrap_oracle(data.clone()).call().await?;
    let total = premium + fee;
    
    let balance = provider.get_balance(address, None).await?;
    if balance < total {
        anyhow::bail!("Insufficient balance on Base. Need {} wei, have {} wei", total, balance);
    }

    println!("   💸 Registering identity via ERC-7527 (Cost: {} wei)...", total);
    
    let tx = agency.wrap(address, data).value(total);
    let pending_tx = tx.send().await?;
    let receipt = pending_tx.await?.context("Transaction failed")?;
    
    println!("   ✅ Registration confirmed: {:?}", receipt.transaction_hash);

    update_agents_md(repo_path, &format!("{:?}", address))?;

    Ok(())
}

fn get_repo_url(repo_path: &str) -> Option<String> {
    let git_config_path = Path::new(repo_path).join(".git/config");
    if git_config_path.exists() {
        if let Ok(content) = fs::read_to_string(git_config_path) {
            for line in content.lines() {
                if line.trim().starts_with("url =") {
                    return Some(line.split('=').nth(1)?.trim().to_string());
                }
            }
        }
    }
    None
}

fn update_agents_md(repo_path: &str, address: &str) -> Result<()> {
    let path = Path::new(repo_path).join("AGENTS.md");
    if !path.exists() {
        anyhow::bail!("AGENTS.md not found. Run 'generate' first.");
    }

    let content = fs::read_to_string(&path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    
    let mut found = false;
    for line in &mut lines {
        if line.contains("Agent Identity:") {
            *line = format!("**Agent Identity:** `{}`", address);
            found = true;
            break;
        }
    }

    if !found {
        let mut insert_pos = 0;
        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("> ") {
                insert_pos = i + 1;
            }
        }
        lines.insert(insert_pos, "".to_string());
        lines.insert(insert_pos + 1, format!("**Agent Identity:** `{}`", address));
    }

    fs::write(path, lines.join("\n"))?;
    println!("   📝 Updated AGENTS.md with Agent Identity.");
    Ok(())
}
