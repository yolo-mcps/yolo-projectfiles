use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::env;

#[mcp_tool(
    name = "system_info",
    description = "Returns information about the system (OS, architecture, etc.)"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct SystemInfoTool {
    // No parameters needed for system info
}

impl SystemInfoTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let family = env::consts::FAMILY;

        let info = format!(
            "Operating System: {}\nArchitecture: {}\nFamily: {}",
            os, arch, family
        );

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                info, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[mcp_tool(
    name = "environment",
    description = "Gets the value of an environment variable"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EnvironmentTool {
    /// Name of the environment variable
    pub variable: String,
}

impl EnvironmentTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let value = match env::var(&self.variable) {
            Ok(val) => val,
            Err(_) => format!("Environment variable '{}' not found", self.variable),
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                value, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}