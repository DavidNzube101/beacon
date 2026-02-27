mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;

use clap::{Parser, Subcommand};

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
        /// Path to local repo or GitHub URL (e.g. https://github.com/user/repo)
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
            println!("🚀 Beacon API on http://0.0.0.0:{port}");
            // axum server
        }
    }

    Ok(())
}
