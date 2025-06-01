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

#[mcp_tool(
    name = "delete", 
    description = "Deletes files/directories within the project directory only. IMPORTANT: Requires explicit confirmation (confirm=true) for safety. Supports recursive directory deletion. Removes file from read tracking after deletion."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct DeleteTool {
    /// Path to delete (relative to project root)
    pub path: String,
    /// Whether to recursively delete directories (default: false)
    #[serde(default)]
    pub recursive: bool,
    /// Require confirmation by setting to true (default: false for safety)
    #[serde(default)]
    pub confirm: bool,
}

#[async_trait]
impl StatefulTool for DeleteTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        if !self.confirm {
            return Err(CallToolError::unknown_tool(
                "Deletion requires confirmation. Set confirm=true to proceed.".to_string()
            ));
        }
        
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
        
        // Don't allow deleting the project root
        if canonical_path == current_dir {
            return Err(CallToolError::unknown_tool(
                "Cannot delete the project root directory".to_string()
            ));
        }
        
        if !canonical_path.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "Path not found: {}",
                self.path
            )));
        }
        
        let metadata = fs::metadata(&canonical_path)
            .await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
        
        let deleted_count;
        let file_type;
        
        if metadata.is_file() {
            file_type = "file";
            fs::remove_file(&canonical_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to delete file: {}", e)))?;
            deleted_count = 1;
        } else if metadata.is_dir() {
            file_type = "directory";
            if self.recursive {
                deleted_count = count_entries(&canonical_path).await?;
                fs::remove_dir_all(&canonical_path)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to delete directory: {}", e)))?;
            } else {
                // Check if directory is empty
                let mut entries = fs::read_dir(&canonical_path)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to read directory: {}", e)))?;
                
                if entries.next_entry().await.map_err(|e| CallToolError::unknown_tool(format!("Failed to check directory: {}", e)))?.is_some() {
                    return Err(CallToolError::unknown_tool(
                        "Directory is not empty. Set recursive=true to delete non-empty directories.".to_string()
                    ));
                }
                
                fs::remove_dir(&canonical_path)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to delete empty directory: {}", e)))?;
                deleted_count = 1;
            }
        } else {
            return Err(CallToolError::unknown_tool(
                "Path is neither a file nor a directory".to_string()
            ));
        }
        
        // Remove from tracking
        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        read_files_clone.remove(&canonical_path);
        context.set_custom_state(read_files_clone).await;
        
        let written_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut written_files_clone = (*written_files).clone();
        written_files_clone.remove(&canonical_path);
        context.set_custom_state(written_files_clone).await;
        
        let message = if self.recursive && deleted_count > 1 {
            format!(
                "Successfully deleted {} '{}' ({} items removed)",
                file_type, self.path, deleted_count
            )
        } else {
            format!(
                "Successfully deleted {} '{}'",
                file_type, self.path
            )
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn count_entries(path: &Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, CallToolError>> + Send + '_>> {
    Box::pin(async move {
    let mut count = 1; // Count the directory itself
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| CallToolError::unknown_tool(format!("Failed to read directory: {}", e)))?;
    
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let file_type = entry.file_type().await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to get file type: {}", e)))?;
                
                if file_type.is_dir() {
                    count += Box::pin(count_entries(&entry.path())).await?;
                } else {
                    count += 1;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(CallToolError::unknown_tool(format!("Failed to read entry: {}", e))),
        }
    }
    
    Ok(count)
    })
}

impl DeleteTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}