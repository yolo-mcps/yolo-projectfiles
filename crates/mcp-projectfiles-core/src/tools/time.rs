use chrono::Utc;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};

#[mcp_tool(name = "current_time", description = "Gets the current date and time")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CurrentTimeTool {
    /// Timezone (e.g., 'UTC', 'local', or IANA timezone like 'America/New_York')
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "local".to_string()
}

impl CurrentTimeTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let now = Utc::now();
        let formatted_time = match self.timezone.as_str() {
            "UTC" | "utc" => now.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            "local" => {
                let local_now = chrono::Local::now();
                local_now.format("%Y-%m-%d %H:%M:%S %Z").to_string()
            }
            _ => now.format("%Y-%m-%d %H:%M:%S UTC").to_string(), // Default to UTC for unknown timezones
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                formatted_time,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[mcp_tool(name = "timestamp", description = "Gets the current Unix timestamp")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct TimestampTool {
    /// Unit for timestamp ('seconds' or 'milliseconds')
    #[serde(default = "default_unit")]
    pub unit: String,
}

fn default_unit() -> String {
    "seconds".to_string()
}

impl TimestampTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let now = Utc::now();
        let timestamp = match self.unit.as_str() {
            "milliseconds" => now.timestamp_millis().to_string(),
            _ => now.timestamp().to_string(), // Default to seconds
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                timestamp, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}