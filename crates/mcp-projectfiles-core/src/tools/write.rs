use crate::context::{StatefulTool, ToolContext};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

#[mcp_tool(name = "write", description = "Writes content to a text file within the project directory. File must have been previously read.")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct WriteTool {
    /// Path to the file to write (relative to project root)
    pub path: String,
    /// Content to write to the file
    pub content: String,
}

#[async_trait]
impl StatefulTool for WriteTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        let canonical_path = if absolute_path.exists() {
            absolute_path.canonicalize()
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path '{}': {}", self.path, e)))?
        } else {
            absolute_path
        };
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }

        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        
        if canonical_path.exists() && !read_files.contains(&canonical_path) {
            return Err(CallToolError::unknown_tool(format!(
                "Cannot write to '{}': File must be read first before writing",
                self.path
            )));
        }

        if let Some(parent) = canonical_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to create parent directories: {}", e)))?;
            }
        }

        fs::write(&canonical_path, &self.content)
            .await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to write file: {}", e)))?;

        let mut read_files_clone = (*read_files).clone();
        read_files_clone.insert(canonical_path.clone());
        context.set_custom_state(read_files_clone).await;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!(
                    "Successfully wrote {} bytes to {}",
                    self.content.len(),
                    self.path
                ),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl WriteTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}