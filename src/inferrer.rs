use anyhow::{Result, Context};
use reqwest::Client;
use serde_json::{json, Value};
use crate::models::{RepoContext, AgentsManifest};

const GEMINI_API_URL: &str = 
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";

pub async fn infer_capabilities(ctx: &RepoContext) -> Result<AgentsManifest> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .context("GEMINI_API_KEY not set — add it to your .env file")?;

    let client = Client::new();
    let prompt = build_prompt(ctx);

    println!("   🤖 Calling Gemini 2.5 Flash...");

    let response = client
        .post(format!("{}?key={}", GEMINI_API_URL, api_key))
        .json(&json!({
            "contents": [{
                "parts": [{
                    "text": prompt
                }]
            }],
            "generationConfig": {
                "temperature": 0.2,
                "responseMimeType": "application/json"
            }
        }))
        .send()
        .await
        .context("Failed to reach Gemini API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Gemini API error {}: {}", status, body);
    }

    let raw: Value = response.json().await
        .context("Failed to parse Gemini response")?;

    // Extract the text content from Gemini's response envelope
    let text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .context("Unexpected Gemini response shape")?;

    // Strip markdown fences if Gemini adds them despite responseMimeType
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let manifest: AgentsManifest = serde_json::from_str(clean)
        .context("Failed to parse Gemini output as AgentsManifest")?;

    println!("   ✓ Inferred {} capabilities", manifest.capabilities.len());
    println!("   ✓ Inferred {} endpoints", manifest.endpoints.len());

    Ok(manifest)
}

fn build_prompt(ctx: &RepoContext) -> String {
    let mut parts: Vec<String> = vec![
        "You are an expert at analyzing software repositories and extracting agent-usable capabilities.".to_string(),
        "Analyze the following repository context and return a JSON object describing its capabilities for AI agents.".to_string(),
        "".to_string(),
        "CRITICAL: Return ONLY valid JSON. No markdown, no explanation, no preamble.".to_string(),
        "".to_string(),
        "The JSON must match this exact schema:".to_string(),
        r#"{
  "name": "string — repo/project name",
  "description": "string — what this project does, written for an AI agent",
  "version": "string or null",
  "capabilities": [
    {
      "name": "string — capability name (snake_case)",
      "description": "string — what an agent can do with this",
      "input_schema": null,
      "output_schema": null,
      "examples": ["string — example usage"]
    }
  ],
  "endpoints": [
    {
      "path": "string — e.g. /api/users",
      "method": "string — GET/POST/PUT/DELETE",
      "description": "string",
      "parameters": [
        {
          "name": "string",
          "type": "string",
          "required": true,
          "description": "string"
        }
      ]
    }
  ],
  "authentication": {
    "type": "bearer | api_key | none",
    "description": "string or null"
  },
  "rate_limits": null,
  "contact": null
}"#.to_string(),
        "".to_string(),
        "--- REPOSITORY CONTEXT ---".to_string(),
    ];

    parts.push(format!("Project name: {}", ctx.name));

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
        parts.push("\n## Source Files".to_string());
        for file in ctx.source_files.iter().take(10) {
            parts.push(format!(
                "\n### {}\n{}",
                file.path,
                truncate(&file.content, 1500)
            ));
        }
    }

    parts.push("\n--- END CONTEXT ---".to_string());
    parts.push("Now return the JSON object:".to_string());

    parts.join("\n")
}

fn truncate(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        s
    } else {
        &s[..max_chars]
    }
}