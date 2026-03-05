use async_trait::async_trait;
use rust_mcp_sdk::{
    McpServer,
    macros,
    mcp_server::ServerHandler,
    schema::*,
};
use crate::{inferrer, scanner, generator, validator, db, verifier};
use std::sync::Arc;

fn cte(msg: impl std::fmt::Display) -> CallToolError {
    use std::io::{Error, ErrorKind};
    CallToolError(Box::new(Error::new(ErrorKind::Other, msg.to_string())))
}

#[macros::mcp_tool(
    name = "generate_agents_md",
    description = "Scans a local repository and generates an AGENTS.md manifest."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct GenerateTool {
    pub path: String,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub run_id: Option<String>,
    pub txn_hash: Option<String>,
    pub chain: Option<String>,
}

#[macros::mcp_tool(
    name = "validate_agents_md",
    description = "Validates an existing AGENTS.md file against the standard."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct ValidateTool {
    pub content: String,
    pub api_key: Option<String>,
    pub run_id: Option<String>,
    pub txn_hash: Option<String>,
    pub chain: Option<String>,
}

#[derive(Default)]
pub struct BeaconMcpHandler;

#[async_trait]
impl ServerHandler for BeaconMcpHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![GenerateTool::tool(), ValidateTool::tool()],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        match params.name.as_str() {
            "generate_agents_md" => {
                let args: GenerateTool = serde_json::from_value(serde_json::Value::Object(params.arguments.unwrap_or_default()))
                    .map_err(cte)?;
                
                let is_cloud = args.api_key.is_none();
                
                if is_cloud {
                    if let (Some(rid), Some(txn), Some(ch)) = (args.run_id, args.txn_hash, args.chain) {
                        let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string()).parse::<f64>().unwrap_or(0.09);
                        let wallet = if ch == "base" { std::env::var("BEACON_WALLET_BASE").unwrap_or_default() } else { std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default() };
                        
                        let verified = verifier::verify_payment(&ch, &txn, amount, &wallet).await.map_err(cte)?;
                        if !verified { return Ok(CallToolResult::text_content(vec!["Payment verification failed. Please try again.".into()])); }
                        
                        db::mark_run_paid(&rid, &txn, &ch).await.ok();
                    } else {
                        let ctx = scanner::scan_local(&args.path).map_err(cte)?;
                        let rid = db::create_run(&ctx.name).await.map_err(cte)?;
                        let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
                        let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
                        let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();
                        
                        let instructions = format!(
                            "💰 Beacon Cloud Payment Required\n\nRun ID: {}\nAmount: {} USDC\n\nAddresses:\n- Base: {}\n- Solana: {}\n\nAfter paying, please call this tool again with run_id, txn_hash, and chain.",
                            rid, amount, w_base, w_sol
                        );
                        return Ok(CallToolResult::text_content(vec![instructions.into()]));
                    }
                }

                let provider = args.provider.unwrap_or_else(|| "gemini".into());
                let ctx = scanner::scan_local(&args.path).map_err(cte)?;
                let manifest = inferrer::infer_capabilities(&ctx, &provider, args.api_key.as_deref())
                    .await
                    .map_err(cte)?;

                let tmp_path = format!("/tmp/mcp_beacon_{}.md", ctx.name);
                generator::generate_agents_md(&manifest, &tmp_path)
                    .map_err(cte)?;
                let content = std::fs::read_to_string(&tmp_path)
                    .map_err(cte)?;
                let _ = std::fs::remove_file(tmp_path);

                Ok(CallToolResult::text_content(vec![content.into()]))
            }
            "validate_agents_md" => {
                let args: ValidateTool = serde_json::from_value(serde_json::Value::Object(params.arguments.unwrap_or_default()))
                    .map_err(cte)?;
                
                let is_cloud = args.api_key.is_none();

                if is_cloud {
                    if let (Some(rid), Some(txn), Some(ch)) = (args.run_id, args.txn_hash, args.chain) {
                        let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string()).parse::<f64>().unwrap_or(0.09);
                        let wallet = if ch == "base" { std::env::var("BEACON_WALLET_BASE").unwrap_or_default() } else { std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default() };
                        let verified = verifier::verify_payment(&ch, &txn, amount, &wallet).await.map_err(cte)?;
                        if !verified { return Ok(CallToolResult::text_content(vec!["Payment verification failed.".into()])); }
                        db::mark_run_paid(&rid, &txn, &ch).await.ok();
                    } else {
                        let rid = db::create_run("validate-only").await.map_err(cte)?;
                        let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
                        let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
                        let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();
                        let instructions = format!(
                            "💰 Beacon Cloud Validation Payment Required\n\nRun ID: {}\nAmount: {} USDC\n\nAddresses:\n- Base: {}\n- Solana: {}\n\nAfter paying, call this tool again with run_id, txn_hash, and chain.",
                            rid, amount, w_base, w_sol
                        );
                        return Ok(CallToolResult::text_content(vec![instructions.into()]));
                    }
                }

                let result = validator::validate_content(&args.content)
                    .map_err(cte)?;

                let text = format!(
                    "Validation Result:\nValid: {}\nErrors: {}\nWarnings: {}",
                    result.valid,
                    result.errors.join(", "),
                    result.warnings.join(", ")
                );

                Ok(CallToolResult::text_content(vec![text.into()]))
            }
            _ => Err(CallToolError::unknown_tool(params.name)),
        }
    }
}
