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

#[mcp_tool(name = "file_read", description = "Reads the contents of a text file within the project directory")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FileReadTool {
    /// Path to the file to read (relative to project root)
    pub path: String,
}

#[async_trait]
impl StatefulTool for FileReadTool {
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
        
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path '{}': {}", self.path, e)))?;
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "File not found: {}",
                self.path
            )));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::unknown_tool(format!(
                "Path is not a file: {}",
                self.path
            )));
        }

        let content = fs::read_to_string(&canonical_path)
            .await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read file: {}", e)))?;

        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        read_files_clone.insert(canonical_path.clone());
        context.set_custom_state(read_files_clone).await;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                content, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl FileReadTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}

#[mcp_tool(name = "file_write", description = "Writes content to a text file within the project directory. File must have been previously read.")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FileWriteTool {
    /// Path to the file to write (relative to project root)
    pub path: String,
    /// Content to write to the file
    pub content: String,
}

#[async_trait]
impl StatefulTool for FileWriteTool {
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

impl FileWriteTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}

#[mcp_tool(
    name = "file_list",
    description = "Lists files and directories in a given path"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FileListTool {
    /// Path to list files from
    pub path: String,
}

impl FileListTool {
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