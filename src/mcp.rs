use async_trait::async_trait;
use rust_mcp_sdk::{
    McpServer,
    macros,
    mcp_server::ServerHandler,
    schema::*,
};
use crate::{inferrer, scanner, generator, validator};
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
}

#[macros::mcp_tool(
    name = "validate_agents_md",
    description = "Validates an existing AGENTS.md file against the standard."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct ValidateTool {
    pub content: String,
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
                
                let provider = args.provider.unwrap_or_else(|| "gemini".into());
                let ctx = scanner::scan_local(&args.path).map_err(cte)?;
                let manifest = inferrer::infer_capabilities(&ctx, &provider, None)
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
