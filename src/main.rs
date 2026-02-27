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
            println!("✅ Validating: {file}");
            println!("   Endpoint checks: {check_endpoints}");
            // validator pipeline
        }
        Commands::Serve { port } => {
            println!("🚀 Beacon API on http://0.0.0.0:{port}");
            // axum server
        }
    }

    Ok(())
}
