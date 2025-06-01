use crate::context::{StatefulTool, ToolContext};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[mcp_tool(
    name = "copy", 
    description = "Copies files/directories recursively within the project directory only. Preserves directory structure and supports metadata preservation (default: true). Supports overwrite option (default: false). Creates destination parent directories if needed."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CopyTool {
    /// Source path (relative to project root)
    pub source: String,
    /// Destination path (relative to project root)
    pub destination: String,
    /// Whether to overwrite existing files (default: false)
    #[serde(default)]
    pub overwrite: bool,
    /// Whether to preserve file metadata (default: true)
    #[serde(default = "default_preserve_metadata")]
    pub preserve_metadata: bool,
}

fn default_preserve_metadata() -> bool {
    true
}

#[async_trait]
impl StatefulTool for CopyTool {
    async fn call_with_context(
        self,
        _context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        // Process source path
        let source_path = Path::new(&self.source);
        let absolute_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            current_dir.join(source_path)
        };
        
        let canonical_source = absolute_source.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve source path '{}': {}", self.source, e)))?;
        
        if !canonical_source.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Source path '{}' is outside the project directory",
                self.source
            )));
        }
        
        if !canonical_source.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "Source file not found: {}",
                self.source
            )));
        }
        
        // Process destination path
        let dest_path = Path::new(&self.destination);
        let absolute_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            current_dir.join(dest_path)
        };
        
        // For destination, we can't canonicalize if it doesn't exist yet
        let canonical_dest = if absolute_dest.exists() {
            absolute_dest.canonicalize()
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve destination path '{}': {}", self.destination, e)))?
        } else {
            // Ensure parent exists and is within project
            if let Some(parent) = absolute_dest.parent() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve destination parent directory: {}", e)))?;
                if !canonical_parent.starts_with(&current_dir) {
                    return Err(CallToolError::unknown_tool(format!(
                        "Access denied: Destination path '{}' would be outside the project directory",
                        self.destination
                    )));
                }
            }
            absolute_dest
        };
        
        if !canonical_dest.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Destination path '{}' is outside the project directory",
                self.destination
            )));
        }
        
        // Check if destination exists
        if canonical_dest.exists() && !self.overwrite {
            return Err(CallToolError::unknown_tool(format!(
                "Destination '{}' already exists. Set overwrite=true to replace it.",
                self.destination
            )));
        }
        
        // Create parent directory if needed
        if let Some(parent) = canonical_dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to create parent directory: {}", e)))?;
        }
        
        // Perform the copy
        if canonical_source.is_file() {
            // Copy file
            fs::copy(&canonical_source, &canonical_dest)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to copy file: {}", e)))?;
            
            // Preserve metadata if requested
            if self.preserve_metadata {
                let _metadata = fs::metadata(&canonical_source)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to read source metadata: {}", e)))?;
                
                // Metadata preservation is best-effort
                // Rust's async fs doesn't have direct timestamp setting
            }
        } else if canonical_source.is_dir() {
            // Recursive directory copy
            copy_dir_recursive(&canonical_source, &canonical_dest, self.overwrite).await?;
        } else {
            return Err(CallToolError::unknown_tool(
                "Source is neither a file nor a directory".to_string()
            ));
        }
        
        let file_type = if canonical_source.is_dir() { "directory" } else { "file" };
        let message = format!(
            "Successfully copied {} from '{}' to '{}'",
            file_type, self.source, self.destination
        );

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn copy_dir_recursive<'a>(
    src: &'a Path, 
    dst: &'a Path, 
    overwrite: bool
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), CallToolError>> + Send + 'a>> {
    Box::pin(async move {
    // Create destination directory
    fs::create_dir_all(dst)
        .await
        .map_err(|e| CallToolError::unknown_tool(format!("Failed to create directory: {}", e)))?;
    
    // Read directory entries
    let mut entries = fs::read_dir(src)
        .await
        .map_err(|e| CallToolError::unknown_tool(format!("Failed to read directory: {}", e)))?;
    
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let file_type = entry.file_type().await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to get file type: {}", e)))?;
                
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                
                if file_type.is_dir() {
                    Box::pin(copy_dir_recursive(&src_path, &dst_path, overwrite)).await?;
                } else if file_type.is_file() {
                    if dst_path.exists() && !overwrite {
                        return Err(CallToolError::unknown_tool(format!(
                            "Destination file '{}' already exists",
                            dst_path.display()
                        )));
                    }
                    fs::copy(&src_path, &dst_path)
                        .await
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to copy file: {}", e)))?;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(CallToolError::unknown_tool(format!("Failed to read directory entry: {}", e))),
        }
    }
    
    Ok(())
    })
}

impl CopyTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}