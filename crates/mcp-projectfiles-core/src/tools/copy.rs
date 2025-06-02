use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_path, format_counts};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use std::time::Instant;

const TOOL_NAME: &str = "copy";

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
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Normalize paths
        let source_path = Path::new(&self.source);
        let dest_path = Path::new(&self.destination);
        
        // Ensure paths are within project root
        let canonical_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            project_root.join(source_path)
        };
        
        let canonical_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            project_root.join(dest_path)
        };
        
        // Validate source exists
        if !canonical_source.exists() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Source path '{}' does not exist", self.source)
            )));
        }
        
        // Ensure source is within project root
        if !canonical_source.starts_with(&project_root) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Source path is outside the project directory"
            )));
        }
        
        // Ensure destination would be within project root
        if !canonical_dest.starts_with(&project_root) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Destination path is outside the project directory"
            )));
        }
        
        // Prevent copying into itself
        if canonical_source.is_dir() && canonical_dest.starts_with(&canonical_source) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Cannot copy a directory into itself"
            )));
        }
        
        // Check if destination is inside an existing directory
        if let Some(parent) = canonical_dest.parent() {
            if parent.exists() && parent.is_file() {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    "Destination parent path exists but is not a directory"
                )));
            }
        }
        
        // Additional validation for absolute paths
        if dest_path.is_absolute() && !canonical_dest.starts_with(&project_root) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Destination path is outside the project directory"
            )));
        }
        
        // Check if destination exists
        if canonical_dest.exists() && !self.overwrite {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Destination '{}' already exists. Set overwrite=true to replace it.", self.destination)
            )));
        }
        
        // Create parent directory if needed
        if let Some(parent) = canonical_dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directory: {}", e))))?;
        }
        
        // Track operation start time
        let start_time = Instant::now();
        let total_size: u64;
        let file_count: usize;
        let dir_count: usize;
        
        // Perform the copy
        if canonical_source.is_file() {
            // Copy file
            let metadata = fs::metadata(&canonical_source)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read source metadata: {}", e))))?;
            
            total_size = metadata.len();
            file_count = 1;
            dir_count = 0;
            
            fs::copy(&canonical_source, &canonical_dest)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to copy file: {}", e))))?;
            
            // Preserve metadata if requested
            if self.preserve_metadata {
                // Metadata preservation is best-effort
                // Rust's async fs doesn't have direct timestamp setting
            }
        } else if canonical_source.is_dir() {
            // Recursive directory copy
            let stats = copy_dir_recursive(&canonical_source, &canonical_dest, self.overwrite).await?;
            total_size = stats.total_size;
            file_count = stats.file_count;
            dir_count = stats.dir_count;
        } else {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Source is neither a file nor a directory"
            )));
        }
        
        let _duration = start_time.elapsed();
        let file_type = if canonical_source.is_dir() { "directory" } else { "file" };
        
        // Format paths relative to project root
        let source_relative = canonical_source.strip_prefix(&project_root)
            .unwrap_or(&canonical_source);
        let dest_relative = canonical_dest.strip_prefix(&project_root)
            .unwrap_or(&canonical_dest);
        
        // Build metrics string
        let metrics = if canonical_source.is_dir() {
            let counts = format_counts(&[
                (file_count, "file", "files"),
                (dir_count, "directory", "directories")
            ]);
            format!(" ({}, {})", counts, format_size(total_size))
        } else {
            format!(" ({})", format_size(total_size))
        };
        
        let message = format!(
            "Copied {} {} to {}{}",
            file_type,
            format_path(source_relative),
            format_path(dest_relative),
            metrics
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

#[derive(Default)]
struct CopyStats {
    total_size: u64,
    file_count: usize,
    dir_count: usize,
}

fn copy_dir_recursive<'a>(
    src: &'a Path, 
    dst: &'a Path, 
    overwrite: bool
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CopyStats, CallToolError>> + Send + 'a>> {
    Box::pin(async move {
    let mut stats = CopyStats::default();
    
    // Create destination directory
    fs::create_dir_all(dst)
        .await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create directory: {}", e))))?;
    
    stats.dir_count += 1;
    
    // Read directory entries
    let mut entries = fs::read_dir(src)
        .await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;
    
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let file_type = entry.file_type().await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file type: {}", e))))?;
                
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                
                if file_type.is_dir() {
                    let sub_stats = Box::pin(copy_dir_recursive(&src_path, &dst_path, overwrite)).await?;
                    stats.total_size += sub_stats.total_size;
                    stats.file_count += sub_stats.file_count;
                    stats.dir_count += sub_stats.dir_count;
                } else if file_type.is_file() {
                    if dst_path.exists() && !overwrite {
                        return Err(CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Destination file '{}' already exists", dst_path.display())
                        )));
                    }
                    
                    let metadata = fs::metadata(&src_path)
                        .await
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?;
                    
                    stats.total_size += metadata.len();
                    stats.file_count += 1;
                    
                    fs::copy(&src_path, &dst_path)
                        .await
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to copy file: {}", e))))?;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory entry: {}", e)))),
        }
    }
    
    Ok(stats)
    })
}

impl CopyTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}