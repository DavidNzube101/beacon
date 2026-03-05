#![allow(dead_code)]

mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;
mod verifier;
mod errors;
mod identity;
mod mcp;

mod tests;
mod db;

use anyhow::{Result as AnyResult, Context};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json,
};
use rust_mcp_sdk::{
    mcp_server::{hyper_server, HyperServerOptions, ToMcpServerHandler},
    schema::*,
};
use clap::{Parser, Subcommand};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};
use std::result::Result as StdResult;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct AppState {
    redis_client: Arc<redis::Client>,
}

const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
const RATE_LIMIT_MAX_REQUESTS: usize = 20;

fn random_emoji() -> &'static str {
    ["⬛", "⬜"].choose(&mut rand::thread_rng()).unwrap_or(&"⬛")
}

async fn check_rate_limit(state: &AppState, ip: &str) -> StdResult<(), StatusCode> {
    let key = format!("ratelimit:{}", ip);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut conn = state.redis_client
        .get_multiplexed_async_connection().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let results: StdResult<Vec<redis::Value>, _> = redis::pipe()
        .atomic()
        .zrembyscore(&key, 0, (now - RATE_LIMIT_WINDOW_SECONDS) as f64)
        .zadd(&key, now, now)
        .zcard(&key)
        .expire(&key, RATE_LIMIT_WINDOW_SECONDS as i64)
        .query_async(&mut conn)
        .await;

    let results = results.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let count: usize = if results.len() >= 3 {
        match &results[2] {
            redis::Value::Int(c) => *c as usize,
            _ => 0,
        }
    } else { 0 };

    if count > RATE_LIMIT_MAX_REQUESTS {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}

#[derive(Parser)]
#[command(name = "beacon")]
#[command(about = "⬛ Make any repo agent-ready. Instantly.")]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        target: String,
        #[arg(short, long, default_value = "AGENTS.md")]
        output: String,
        #[arg(long, default_value = "gemini")]
        provider: String,
        #[arg(long)]
        api_key: Option<String>,
    },
    Validate {
        file: String,
        #[arg(long)]
        check_endpoints: bool,
        #[arg(long)]
        provider: Option<String>,
    },
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    Register {
        #[arg(default_value = "./")]
        repo_path: String,
        #[arg(long, default_value = "base")]
        chain: String,
        #[arg(long)]
        agency: Option<String>,
    },
    Upgrade,
}

#[derive(Deserialize)]
struct GenerateRequest {
    #[serde(flatten)]
    repo_context: models::RepoContext,
    provider: Option<String>,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct GenerateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agents_md: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<models::AgentsManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ValidateRequest {
    content: String,
    provider: Option<String>,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct ValidateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint_results: Option<Vec<models::EndpointCheckResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    name: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
        name: "beacon",
    })
}

async fn handle_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GenerateRequest>,
) -> StdResult<impl IntoResponse, errors::BeaconError> {
    let ip = headers.get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if let Err(status) = check_rate_limit(&state, &ip).await {
        return Ok(status.into_response());
    }

    let provider = req.provider.clone().unwrap_or_else(|| "gemini".to_string());
    let mut actual_provider = provider.clone();
    let mut rid_final = None;

    let is_cloud_request = req.api_key.is_none();

    if is_cloud_request {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            rid_final = Some(rid.to_string());
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err(errors::BeaconError::TransactionAlreadyUsed);
            }

            let amount = std::env::var("PAYMENT_AMOUNT_USDC")
                .unwrap_or_else(|_| "0.09".to_string())
                .parse::<f64>()
                .unwrap_or(0.09);
            let wallet = if ch == "base" {
                std::env::var("BEACON_WALLET_BASE").unwrap_or_default()
            } else {
                std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default()
            };

            let verified = verifier::verify_payment(ch, txn, amount, &wallet)
                .await
                .map_err(|e| errors::BeaconError::InferenceError(format!("Verification failed: {}", e)))?;

            if !verified {
                return Err(errors::BeaconError::CloudError { 
                    status: StatusCode::PAYMENT_REQUIRED.as_u16(), 
                    message: "Payment not verified".to_string() 
                });
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
            actual_provider = "gemini".to_string();
        } else {
            let rid = db::create_run(&req.repo_context.name)
                .await
                .map_err(|e| errors::BeaconError::DatabaseError(e.to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Err(errors::BeaconError::PaymentRequired {
                run_id: rid,
                amount,
                base_addr: w_base,
                sol_addr: w_sol,
            });
        }
    }

    let manifest = inferrer::infer_capabilities(&req.repo_context, &actual_provider, req.api_key.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Inference failed: {}", e);
            errors::BeaconError::InferenceError(e.to_string())
        })?;

    let tmp_path = format!("/tmp/beacon_{}.md", &req.repo_context.name);
    generator::generate_agents_md(&manifest, &tmp_path)
        .map_err(|e| {
            tracing::error!("File generation failed: {}", e);
            errors::BeaconError::Unknown(format!("File generation failed: {}", e))
        })?;
    let content = std::fs::read_to_string(&tmp_path)
        .map_err(|e| {
            tracing::error!("Read generated file failed: {}", e);
            errors::BeaconError::IoError(e)
        })?;
    let _ = std::fs::remove_file(&tmp_path);

    if is_cloud_request {
        if let Some(rid) = rid_final {
            db::mark_run_complete(&rid, &content).await.ok();
        }
    }

    Ok(Json(GenerateResponse {
        success: true,
        agents_md: Some(content),
        manifest: Some(manifest),
        error: None,
    }).into_response())
}

async fn handle_validate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ValidateRequest>,
) -> StdResult<impl IntoResponse, errors::BeaconError> {
    let ip = headers.get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if let Err(status) = check_rate_limit(&state, &ip).await {
        return Ok(status.into_response());
    }

    let is_cloud_request = req.api_key.is_none();

    if is_cloud_request {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err(errors::BeaconError::TransactionAlreadyUsed);
            }

            let amount = std::env::var("PAYMENT_AMOUNT_USDC")
                .unwrap_or_else(|_| "0.09".to_string())
                .parse::<f64>()
                .unwrap_or(0.09);
            let wallet = if ch == "base" {
                std::env::var("BEACON_WALLET_BASE").unwrap_or_default()
            } else {
                std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default()
            };

            let verified = verifier::verify_payment(ch, txn, amount, &wallet)
                .await
                .map_err(|e| errors::BeaconError::ValidationError(format!("Verification failed: {}", e)))?;

            if !verified {
                return Err(errors::BeaconError::CloudError { 
                    status: StatusCode::PAYMENT_REQUIRED.as_u16(), 
                    message: "Payment not verified".to_string() 
                });
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
        } else {
            let rid = db::create_run("validate-only")
                .await
                .map_err(|e| errors::BeaconError::DatabaseError(e.to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Err(errors::BeaconError::PaymentRequired {
                run_id: rid,
                amount,
                base_addr: w_base,
                sol_addr: w_sol,
            });
        }
    }

    let result = validator::validate_content(&req.content)
        .map_err(|e| {
            tracing::error!("Validation failed: {}", e);
            errors::BeaconError::ValidationError(e.to_string())
        })?;

    Ok(Json(ValidateResponse {
        success: true,
        valid: Some(result.valid),
        errors: Some(result.errors),
        warnings: Some(result.warnings),
        endpoint_results: Some(result.endpoint_results),
        error: None,
    }).into_response())
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate {
            target,
            output,
            provider,
            api_key,
        } => {
            println!("{} Beacon — scanning {}...", random_emoji(), target);
            let ctx = scanner::scan_local(&target)?;
            println!("📦 Repo: {} ({} source files)", ctx.name, ctx.source_files.len());
            let manifest = inferrer::infer_capabilities(&ctx, &provider, api_key.as_deref()).await?;
            generator::generate_agents_md(&manifest, &output)?;
            println!("\n✅ Done! AGENTS.md written to: {}", output);
            println!("   Provider:     {}", provider);
            println!("   Capabilities: {}", manifest.capabilities.len());
            println!("   Endpoints:    {}", manifest.endpoints.len());
        }
        Commands::Validate {
            file,
            check_endpoints,
            provider,
        } => {
            println!("{} Beacon — validating {}...", random_emoji(), file);
            let content =
                std::fs::read_to_string(&file).map_err(|_| anyhow::anyhow!("File not found: {}", file))?;

            let mut result = if let Some(p) = provider {
                if p == "beacon-ai-cloud" {
                    validator::validate_cloud(&content).await?
                } else {
                    validator::validate_content(&content)?
                }
            } else {
                validator::validate_content(&content)?
            };

            if check_endpoints {
                println!("   🌐 Checking endpoint reachability...");
                result.endpoint_results = validator::check_endpoints(&content).await?;
            }
            println!("\n📋 Validation Report");
            println!("   Valid:    {}", if result.valid { "✅ Yes" } else { "❌ No" });
            println!("   Errors:   {}", result.errors.len());
            println!("   Warnings: {}", result.warnings.len());
            if !result.errors.is_empty() {
                println!("\n❌ Errors:");
                for e in &result.errors {
                    println!("   • {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("\n⚠️  Warnings:");
                for w in &result.warnings {
                    println!("   • {}", w);
                }
            }
            if !result.endpoint_results.is_empty() {
                println!("\n🌐 Endpoint Results:");
                for ep in &result.endpoint_results {
                    let status = ep.status_code.map(|s| s.to_string()).unwrap_or_else(|| "—".to_string());
                    println!(
                        "   {} {} ({})",
                        if ep.reachable { "✅" } else { "❌" },
                        ep.endpoint,
                        status
                    );
                }
            }
        }
        Commands::Serve { port } => {
            let redis_url = std::env::var("REDIS_URL").context("REDIS_URL must be set")?;
            let state = AppState {
                redis_client: Arc::new(redis::Client::open(redis_url)?),
            };
            
            let server_info = InitializeResult {
                server_info: Implementation {
                    name: "beacon-mcp".into(),
                    version: VERSION.into(),
                    title: Some("Beacon MCP Server".into()),
                    description: Some("Make any repo agent-ready. Instantly.".into()),
                    icons: vec![],
                    website_url: Some("https://beacon.davidnzube.xyz".into()),
                },
                capabilities: ServerCapabilities {
                    tools: Some(ServerCapabilitiesTools { list_changed: Some(false) }),
                    ..Default::default()
                },
                instructions: None,
                meta: None,
                protocol_version: "2025-11-25".into(),
            };

            let mcp_handler = mcp::BeaconMcpHandler::default();

            let server = hyper_server::create_server(
                server_info,
                mcp_handler.to_mcp_server_handler(),
                HyperServerOptions {
                    host: "0.0.0.0".to_string(),
                    port,
                    sse_support: true,
                    ..Default::default()
                },
            );

            let server = server
                .with_route("/health", get(health))
                .with_route("/validate", post(handle_validate).with_state(state.clone()))
                .with_route("/generate", post(handle_generate).with_state(state));

            println!("{} Beacon API & MCP Server", random_emoji());
            println!("   http://0.0.0.0:{}", port);
            println!("   POST /generate  — generate AGENTS.md from a repo path");
            println!("   POST /validate  — validate an AGENTS.md file");
            println!("   GET  /sse       — MCP Server (SSE)");
            println!("   GET  /health    — health check");

            server.start().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        Commands::Register { repo_path, chain, agency } => {
            println!("{} Registering on-chain agent identity...", random_emoji());
            let _chain = chain;
            identity::register_agent_identity(&repo_path, &_chain, agency.as_deref()).await?;
            println!("\n✅ Done! Agent identity registered.");
        }
        Commands::Upgrade => {
            println!("{} Upgrading Beacon CLI...", random_emoji());
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg("curl -fsSL https://raw.githubusercontent.com/DavidNzube101/beacon/master/install.sh | sh")
                .status()?;
            
            if !status.success() {
                anyhow::bail!("Upgrade failed with status: {}", status);
            }
        }
    }
    Ok(())
}
