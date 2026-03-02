#![allow(dead_code)]

mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;
mod verifier;

mod tests;
mod db;

use anyhow::Context;
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc, time::SystemTime};

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
    },
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

#[derive(Deserialize)]
struct GenerateRequest {
    #[serde(flatten)]
    repo_context: models::RepoContext,
    provider: Option<String>,
}

#[derive(Serialize)]
struct GenerateResponse {
    success: bool,
    agents_md: String,
    capabilities: usize,
    endpoints: usize,
    repo_name: String,
}

#[derive(Deserialize)]
struct ValidateRequest {
    content: String,
    provider: Option<String>,
}

#[derive(Serialize)]
struct ValidateResponse {
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
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

async fn rate_limit_middleware(
    State(state): State<AppState>,
    addr: Option<ConnectInfo<SocketAddr>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.uri().path() != "/generate" && request.uri().path() != "/validate" {
        return Ok(next.run(request).await);
    }
    
    let ip = match addr {
        Some(ConnectInfo(a)) => a.ip().to_string(),
        None => {
            // fallback for things like tests/proxies
            request.headers()
                .get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown")
                .to_string()
        }
    };
    
    let key = format!("ratelimit:{}", ip);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        
        
    let results: Vec<redis::Value> = redis::pipe()
        .atomic()
        .zrembyscore(&key, 0, (now - RATE_LIMIT_WINDOW_SECONDS) as f64)
        .zadd(&key, now, now)
        .zcard(&key)
        .expire(&key, RATE_LIMIT_WINDOW_SECONDS as i64)
        .query_async(&mut conn)
        .await
        .map_err(|e| {
            tracing::error!("Redis pipeline error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;



        
    let count: usize = if results.len() >= 3 {
        match &results[2] {
            redis::Value::Int(c) => *c as usize,
            _ => 0,
        }
    } else {
        0
    };

    if count > RATE_LIMIT_MAX_REQUESTS {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    
    Ok(next.run(request).await)
}

async fn handle_generate(
    headers: HeaderMap,
    Json(req): Json<GenerateRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let provider = req.provider.clone().unwrap_or_else(|| "gemini".to_string());
    let mut actual_provider = provider.clone();
    let mut rid_final = None;

    if provider == "beacon-ai-cloud" {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            rid_final = Some(rid.to_string());
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err((StatusCode::CONFLICT, "Transaction hash already used".to_string()));
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
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Verification failed: {}", e)))?;

            if !verified {
                return Err((StatusCode::PAYMENT_REQUIRED, "Payment not verified".to_string()));
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
            actual_provider = "gemini".to_string();
        } else {
            let rid = db::create_run(&req.repo_context.name)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Ok((
                StatusCode::PAYMENT_REQUIRED,
                [
                    ("x-payment-amount", amount),
                    ("x-payment-currency", "USDC".to_string()),
                    ("x-payment-chain-base", "base".to_string()),
                    ("x-payment-address-base", w_base),
                    ("x-payment-chain-solana", "solana".to_string()),
                    ("x-payment-address-solana", w_sol),
                    ("x-payment-run-id", rid),
                ],
                "Payment required",
            )
                .into_response());
        }
    }

    let manifest = inferrer::infer_capabilities(&req.repo_context, &actual_provider, None)
        .await
        .map_err(|e| {
            tracing::error!("Inference failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, json!({"success": false, "error": e.to_string()}).to_string())
        })?;

    let tmp_path = format!("/tmp/beacon_{}.md", &req.repo_context.name);
    generator::generate_agents_md(&manifest, &tmp_path)
        .map_err(|e| {
            tracing::error!("File generation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, json!({"success": false, "error": e.to_string()}).to_string())
        })?;
    let content = std::fs::read_to_string(&tmp_path)
        .map_err(|e| {
            tracing::error!("Read generated file failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, json!({"success": false, "error": e.to_string()}).to_string())
        })?;
    let _ = std::fs::remove_file(&tmp_path);

    if provider == "beacon-ai-cloud" {
        if let Some(rid) = rid_final {
            db::mark_run_complete(&rid, &content).await.ok();
        }
    }

    Ok(Json(GenerateResponse {
        success: true,
        capabilities: manifest.capabilities.len(),
        endpoints: manifest.endpoints.len(),
        repo_name: manifest.name.clone(),
        agents_md: content,
    })
    .into_response())
}

async fn handle_validate(
    headers: HeaderMap,
    Json(req): Json<ValidateRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let provider = req.provider.clone().unwrap_or_else(|| "none".to_string());

    if provider == "beacon-ai-cloud" {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err((StatusCode::CONFLICT, json!({"success": false, "error": "Transaction hash already used"}).to_string()));
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
                .map_err(|e| (StatusCode::BAD_REQUEST, json!({"success": false, "error": format!("Verification failed: {}", e)}).to_string()))?;

            if !verified {
                return Err((StatusCode::PAYMENT_REQUIRED, json!({"success": false, "error": "Payment not verified"}).to_string()));
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
        } else {
            let rid = db::create_run("validate-only")
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, json!({"success": false, "error": e.to_string()}).to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Ok((
                StatusCode::PAYMENT_REQUIRED,
                [
                    ("x-payment-amount", amount),
                    ("x-payment-currency", "USDC".to_string()),
                    ("x-payment-chain-base", "base".to_string()),
                    ("x-payment-address-base", w_base),
                    ("x-payment-chain-solana", "solana".to_string()),
                    ("x-payment-address-solana", w_sol),
                    ("x-payment-run-id", rid),
                ],
                json!({"success": false, "error": "Payment required"}).to_string(),
            )
                .into_response());
        }
    }

    let result = validator::validate_content(&req.content)
        .map_err(|e| {
            tracing::error!("Validation failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, json!({"success": false, "error": e.to_string()}).to_string())
        })?;

    Ok(Json(ValidateResponse {
        valid: result.valid,
        errors: result.errors,
        warnings: result.warnings,
    })
    .into_response())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        } => {
            println!("{} Beacon — validating {}...", random_emoji(), file);
            let content =
                std::fs::read_to_string(&file).map_err(|_| anyhow::anyhow!("File not found: {}", file))?;
            let mut result = validator::validate_content(&content)?;
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
            
            let app = Router::new()
                .route("/health", get(health))
                .route("/validate", post(handle_validate))
                .route("/generate", post(handle_generate))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit_middleware,
                ))
                .with_state(state);

            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            println!("{} Beacon API", random_emoji());
            println!("   http://0.0.0.0:{}", port);
            println!("   POST /generate  — generate AGENTS.md from a repo path");
            println!("   POST /validate  — validate an AGENTS.md file");
            println!("   GET  /health    — health check");
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        }
    }
    Ok(())
}
