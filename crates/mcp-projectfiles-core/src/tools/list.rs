use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[mcp_tool(
    name = "list",
    description = "Lists files and directories in a given path"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ListTool {
    /// Path to list files from
    pub path: String,
}

impl ListTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let path = Path::new(&self.path);

        if !path.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "Path not found: {}",
                self.path
            )));
        }

        if !path.is_dir() {
            return Err(CallToolError::unknown_tool(format!(
                "Path is not a directory: {}",
                self.path
            )));
        }

        let mut entries = fs::read_dir(path)
            .await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read directory: {}", e)))?;

        let mut files = Vec::new();
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    return Err(CallToolError::unknown_tool(format!(
                        "Failed to read directory entry: {}",
                        e
                    )));
                }
            };

            let file_name = entry.file_name().to_string_lossy().to_string();
            let metadata = match entry.metadata().await {
                Ok(metadata) => metadata,
                Err(e) => {
                    return Err(CallToolError::unknown_tool(format!(
                        "Failed to read metadata: {}",
                        e
                    )));
                }
            };

            let file_type = if metadata.is_dir() {
                "directory"
            } else {
                "file"
            };
            files.push(format!("{} ({})", file_name, file_type));
        }

        files.sort();
        let listing = files.join("\n");

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                listing, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}