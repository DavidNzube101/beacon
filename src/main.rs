mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;

use clap::{Parser, Subcommand};
use axum::{
    routing::post,
    routing::get,
    Router,
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(name = "beacon")]
#[command(about = "🔦 Make any repo agent-ready. Instantly.")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate an AGENTS.md for a local repo or GitHub URL
    Generate {
        /// Path to local repo or GitHub URL
        target: String,
        /// Output file path
        #[arg(short, long, default_value = "AGENTS.md")]
        output: String,
    },
    /// Validate an existing AGENTS.md file
    Validate {
        /// Path to AGENTS.md
        file: String,
        /// Also test if declared endpoints are reachable
        #[arg(long)]
        check_endpoints: bool,
    },
    /// Start the Beacon web API server
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

// ── API request/response types ─────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateRequest {
    repo_url: String,
    #[serde(default = "default_output")]
    output: String,
}

fn default_output() -> String {
    "AGENTS.md".to_string()
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

// ── Handlers ───────────────────────────────────────────────────────────────

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
        name: "beacon",
    })
}

async fn handle_generate(
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, String)> {
    // Scan
    let ctx = scanner::scan_local(&req.repo_url)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Infer
    let manifest = inferrer::infer_capabilities(&ctx)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Generate markdown
    let mut agents_md_content = String::new();
    let tmp_path = format!("/tmp/beacon_{}.md", &ctx.name);
    generator::generate_agents_md(&manifest, &tmp_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    agents_md_content = std::fs::read_to_string(&tmp_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = std::fs::remove_file(&tmp_path);

    Ok(Json(GenerateResponse {
        success: true,
        capabilities: manifest.capabilities.len(),
        endpoints: manifest.endpoints.len(),
        repo_name: manifest.name.clone(),
        agents_md: agents_md_content,
    }))
}

async fn handle_validate(
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, (StatusCode, String)> {
    let result = validator::validate_content(&req.content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ValidateResponse {
        valid: result.valid,
        errors: result.errors,
        warnings: result.warnings,
    }))
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { target, output } => {
            println!("🔦 Beacon — scanning {}...", target);

            let ctx = scanner::scan_local(&target)?;
            println!("📦 Repo: {} ({} source files)", ctx.name, ctx.source_files.len());

            let manifest = inferrer::infer_capabilities(&ctx).await?;

            generator::generate_agents_md(&manifest, &output)?;

            println!("\n✅ Done! AGENTS.md written to: {}", output);
            println!("   Capabilities: {}", manifest.capabilities.len());
            println!("   Endpoints:    {}", manifest.endpoints.len());
        }

        Commands::Validate { file, check_endpoints } => {
            println!("🔦 Beacon — validating {}...", file);

            let content = std::fs::read_to_string(&file)
                .map_err(|_| anyhow::anyhow!("File not found: {}", file))?;

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
                for e in &result.errors { println!("   • {}", e); }
            }
            if !result.warnings.is_empty() {
                println!("\n⚠️  Warnings:");
                for w in &result.warnings { println!("   • {}", w); }
            }
            if !result.endpoint_results.is_empty() {
                println!("\n🌐 Endpoint Results:");
                for ep in &result.endpoint_results {
                    let status = ep.status_code
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "—".to_string());
                    println!("   {} {} ({})",
                        if ep.reachable { "✅" } else { "❌" },
                        ep.endpoint, status);
                }
            }
        }

        Commands::Serve { port } => {
            let app = Router::new()
                .route("/health", get(health))
                .route("/generate", post(handle_generate))
                .route("/validate", post(handle_validate));

            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            println!("🔦 Beacon API");
            println!("   http://0.0.0.0:{}", port);
            println!("   POST /generate  — generate AGENTS.md from a repo path");
            println!("   POST /validate  — validate an AGENTS.md file");
            println!("   GET  /health    — health check");

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}