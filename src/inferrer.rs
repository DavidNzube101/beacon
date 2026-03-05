use anyhow::{Result, Context};
use reqwest::Client;
use serde_json::{json, Value};
use crate::models::{RepoContext, AgentsManifest};
use crate::errors::BeaconError;
use once_cell::sync::Lazy;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .use_rustls_tls()
        .build()
        .expect("Failed to create reqwest client")
});




const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
const CLAUDE_URL: &str =
    "https://api.anthropic.com/v1/messages";
const OPENAI_URL: &str =
    "https://api.openai.com/v1/chat/completions";
const DEEPSEEK_URL: &str =
    "https://api.deepseek.com/chat/completions";

pub async fn infer_capabilities(
    ctx: &RepoContext,
    provider: &str,
    api_key: Option<&str>,
) -> Result<AgentsManifest> {
    let prompt = build_prompt(ctx);

    println!("   🤖 Provider: {}", provider);

    let result = match provider {
        "gemini" => {
            let key = resolve_key(api_key, "GEMINI_API_KEY", "gemini")?;
            call_gemini(&prompt, &key).await?
        }
        "claude" => {
            let key = resolve_key(api_key, "CLAUDE_API_KEY", "claude")?;
            call_claude(&prompt, &key).await?
        }
        "openai" => {
            let key = resolve_key(api_key, "OPENAI_API_KEY", "openai")?;
            call_openai(&prompt, &key).await?
        }
        "deepseek" => {
            let key = resolve_key(api_key, "DEEPSEEK_API_KEY", "deepseek")?;
            call_deepseek(&prompt, &key).await?
        }
        "beacon-ai-cloud" => {
            call_beacon_cloud(ctx, &prompt).await?
        }
        other => anyhow::bail!(
            "Unknown provider '{}'. Valid options: gemini, claude, openai, deepseek, beacon-ai-cloud",
            other
        ),
    };

    println!("   ✓ Inferred {} capabilities", result.capabilities.len());
    println!("   ✓ Inferred {} endpoints", result.endpoints.len());

    Ok(result)
}


async fn call_gemini(prompt: &str, api_key: &str) -> Result<AgentsManifest> {
    let response = CLIENT
        .post(format!("{}?key={}", GEMINI_URL, api_key))
        .json(&json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": {
                "temperature": 0.2,
                "responseMimeType": "application/json"
            }
        }))
        .send()
        .await
        .context("Failed to reach Gemini API")?;

    check_status(&response, "Gemini")?;

    let raw: Value = response.json().await?;
    let text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .context("Unexpected Gemini response shape")?;

    parse_manifest(text)
}




async fn call_claude(prompt: &str, api_key: &str) -> Result<AgentsManifest> {
    let response = CLIENT
        .post(CLAUDE_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 4096,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        }))
        .send()
        .await
        .context("Failed to reach Claude API")?;

    check_status(&response, "Claude")?;

    let raw: Value = response.json().await?;
    let text = raw["content"][0]["text"]
        .as_str()
        .context("Unexpected Claude response shape")?;

    parse_manifest(text)
}


async fn call_openai(prompt: &str, api_key: &str) -> Result<AgentsManifest> {
    let response = CLIENT
        .post(OPENAI_URL)
        .bearer_auth(api_key)
        .json(&json!({
            "model": "gpt-4o",
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                {
                    "role": "system",
                    "content": "You are an expert at analyzing software repositories. Always respond with valid JSON only."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        }))
        .send()
        .await
        .context("Failed to reach OpenAI API")?;

    check_status(&response, "OpenAI")?;

    let raw: Value = response.json().await?;
    let text = raw["choices"][0]["message"]["content"]
        .as_str()
        .context("Unexpected OpenAI response shape")?;

    parse_manifest(text)
}

async fn call_deepseek(prompt: &str, api_key: &str) -> Result<AgentsManifest> {
    let response = CLIENT
        .post(DEEPSEEK_URL)
        .bearer_auth(api_key)
        .json(&json!({
            "model": "deepseek-chat",
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                {
                    "role": "system",
                    "content": "You are an expert at analyzing software repositories. Always respond with valid JSON only."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        }))
        .send()
        .await
        .context("Failed to reach DeepSeek API")?;

    check_status(&response, "DeepSeek")?;

    let raw: Value = response.json().await?;
    let text = raw["choices"][0]["message"]["content"]
        .as_str()
        .context("Unexpected DeepSeek response shape")?;

    parse_manifest(text)
}



const BEACON_CLOUD_URL: &str = "https://api.beaconcloud.org";

async fn call_beacon_cloud(ctx: &RepoContext, _prompt: &str) -> Result<AgentsManifest> {
    let beacon_url = std::env::var("BEACON_CLOUD_URL")
        .unwrap_or_else(|_| BEACON_CLOUD_URL.to_string());
    let generate_url = format!("{}/generate", beacon_url);

    println!("   ⚡️ Contacting Beacon Cloud...");

    let payload = json!({
        "provider": "beacon-ai-cloud",
        "name": ctx.name,
        "readme": ctx.readme,
        "source_files": ctx.source_files,
        "openapi_spec": ctx.openapi_spec,
        "package_manifest": ctx.package_manifest,
        "existing_agents_md": ctx.existing_agents_md
    });

    let initial_res = CLIENT
        .post(&generate_url)
        .json(&payload)
        .send()
        .await
        .context("Failed to connect to Beacon Cloud API")?;

    if initial_res.status() == reqwest::StatusCode::PAYMENT_REQUIRED {
        let headers = initial_res.headers();
        let amount = headers.get("x-payment-amount").and_then(|v| v.to_str().ok()).unwrap_or("N/A").to_string();
        let run_id = headers.get("x-payment-run-id").and_then(|v| v.to_str().ok()).context("Missing run ID from server")?.to_string();
        let base_addr = headers.get("x-payment-address-base").and_then(|v| v.to_str().ok()).unwrap_or("N/A").to_string();
        let sol_addr = headers.get("x-payment-address-solana").and_then(|v| v.to_str().ok()).unwrap_or("N/A").to_string();

        println!("   💰 Payment required to proceed.");
        println!("\n--------------------------------------------------");
        println!("Please send {} USDC to one of these addresses:", amount);
        println!("  - Base:   {}", base_addr);
        println!("  - Solana: {}", sol_addr);
        println!("--------------------------------------------------\n");

        let mut chain = String::new();
        println!("Which chain did you pay on? (base/solana)");
        std::io::stdin().read_line(&mut chain).context("Failed to read chain")?;
        let chain = chain.trim().to_lowercase();
        
        let mut txn_hash = String::new();
        println!("Please paste the transaction hash:");
        std::io::stdin().read_line(&mut txn_hash).context("Failed to read transaction hash")?;
        let txn_hash = txn_hash.trim();

        println!("   🔍 Verifying payment...");

        let final_res = CLIENT
            .post(&generate_url)
            .header("x-payment-run-id", run_id)
            .header("x-payment-chain", &chain)
            .header("x-payment-txn-hash", txn_hash)
            .json(&payload)
            .send()
            .await
            .context("Failed to send final request to Beacon Cloud")?;

        if !final_res.status().is_success() {
            let status = final_res.status().as_u16();
            let raw: Value = final_res.json().await.unwrap_or(json!({"error": "Unknown error"}));
            let message = raw["error"].as_str().or(raw["error"]["message"].as_str()).unwrap_or("Unknown error").to_string();
            return Err(BeaconError::CloudError { status, message }.into());
        }

        let raw: Value = final_res.json().await.context("Failed to parse final response from Beacon Cloud")?;
        let manifest_val = raw["manifest"].clone();
        let manifest: AgentsManifest = serde_json::from_value(manifest_val).context("Failed to deserialize AgentsManifest from server response")?;
        return Ok(manifest);
    }

    if !initial_res.status().is_success() {
        let status = initial_res.status().as_u16();
        let raw: Value = initial_res.json().await.unwrap_or(json!({"error": "Unknown error"}));
        let message = raw["error"].as_str().or(raw["error"]["message"].as_str()).unwrap_or("Unknown error").to_string();
        return Err(BeaconError::CloudError { status, message }.into());
    }

    let raw: Value = initial_res.json().await?;
    let manifest_val = raw["manifest"].clone();
    let manifest: AgentsManifest = serde_json::from_value(manifest_val).context("Failed to deserialize AgentsManifest from server response")?;
    Ok(manifest)
}

/// resolving API key here, flow would be cli flag > env > error
fn resolve_key(cli_key: Option<&str>, env_var: &str, provider: &str) -> Result<String> {
    if let Some(key) = cli_key {
        return Ok(key.to_string());
    }
    std::env::var(env_var).map_err(|_| anyhow::anyhow!(
        "No API key for {}. Pass --api-key or set {} in your environment.",
        provider, env_var
    ))
}

fn check_status(response: &reqwest::Response, provider: &str) -> Result<()> {
    if !response.status().is_success() {
        anyhow::bail!("{} API returned status {}", provider, response.status());
    }
    Ok(())
}

fn parse_manifest(text: &str) -> Result<AgentsManifest> {
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    serde_json::from_str(clean)
        .context("Failed to parse LLM output as AgentsManifest")
}

fn build_prompt(ctx: &RepoContext) -> String {
    let mut parts: Vec<String> = vec![
        "You are an expert at analyzing software repositories and extracting agent-usable capabilities.".into(),
        "Analyze the following repository context and return a JSON object describing its capabilities for AI agents.".into(),
        "GUIDANCE: Look beyond just utility scripts. Identify server-side capabilities, REST API endpoints (e.g., NestJS/Express/FastAPI decorators like @Get, @Post, @app.get), and background services (notifications, chat systems, indexers).".into(),
        "CRITICAL: Return ONLY valid JSON. No markdown, no explanation, no preamble.".into(),
        "".into(),
        "The JSON must match this exact schema:".into(),
        r#"{
  "name": "string",
  "description": "string — what this project does, written for an AI agent",
  "version": "string or null",
  "capabilities": [
    {
      "name": "string (snake_case)",
      "description": "string — what an agent can do with this",
      "input_schema": null,
      "output_schema": null,
      "examples": ["string"]
    }
  ],
  "endpoints": [
    {
      "path": "string",
      "method": "GET|POST|PUT|DELETE",
      "description": "string",
      "parameters": [
        { "name": "string", "type": "string", "required": true, "description": "string" }
      ]
    }
  ],
  "authentication": { "type": "bearer|api_key|none", "description": "string or null" },
  "rate_limits": null,
  "contact": null
}"#.into(),
        "".into(),
        "--- REPOSITORY CONTEXT ---".into(),
        format!("Project name: {}", ctx.name),
    ];

    if let Some(readme) = &ctx.readme {
        parts.push(format!("\n## README\n{}", truncate(readme, 3000)));
    }
    if let Some(manifest) = &ctx.package_manifest {
        parts.push(format!("\n## Package Manifest\n{}", truncate(manifest, 1000)));
    }
    if let Some(openapi) = &ctx.openapi_spec {
        parts.push(format!("\n## OpenAPI Spec\n{}", truncate(openapi, 3000)));
    }
    if !ctx.source_files.is_empty() {
        parts.push("\n## Source Files".into());
        for file in ctx.source_files.iter().take(10) {
            parts.push(format!("\n### {}\n{}", file.path, truncate(&file.content, 1500)));
        }
    }

    parts.push("\n--- END CONTEXT ---".into());
    parts.push("Return the JSON object:".into());
    parts.join("\n")
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}
