use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_path};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use std::time::Instant;

const TOOL_NAME: &str = "move";

#[mcp_tool(
    name = "move", 
    description = "Moves or renames files/directories within the project directory only. Preserves file tracking state for read/write operations. Supports overwrite option (default: false). Creates destination parent directories if needed."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct MoveTool {
    /// Source path (relative to project root)
    pub source: String,
    /// Destination path (relative to project root)
    pub destination: String,
    /// Whether to overwrite existing files (default: false)
    #[serde(default)]
    pub overwrite: bool,
    /// Whether to preserve file metadata (timestamps, permissions) (default: true)
    #[serde(default = "default_true")]
    pub preserve_metadata: bool,
}

fn default_true() -> bool {
    true
}

#[async_trait]
impl StatefulTool for MoveTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let current_dir = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Process source path
        let source_path = Path::new(&self.source);
        let absolute_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            current_dir.join(source_path)
        };
        
        let canonical_source = absolute_source.canonicalize()
            .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.source)))?;
        
        if !canonical_source.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.source,
                "Source path is outside the project directory"
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
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve destination path '{}': {}", self.destination, e))))?
        } else {
            // Ensure parent exists and is within project
            if let Some(parent) = absolute_dest.parent() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve destination parent directory: {}", e))))?;
                if !canonical_parent.starts_with(&current_dir) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        TOOL_NAME,
                        &self.destination,
                        "Destination path would be outside the project directory"
                    )));
                }
            }
            absolute_dest
        };
        
        if !canonical_dest.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.destination,
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
        
        // Get metadata before move
        let metadata = fs::metadata(&canonical_source).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read source metadata: {}", e))))?;
        
        let is_dir = metadata.is_dir();
        let total_size = if is_dir {
            // For directories, we'll just indicate it's a directory
            0u64
        } else {
            metadata.len()
        };
        
        // Perform the move
        fs::rename(&canonical_source, &canonical_dest)
            .await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to move file: {}", e))))?;
        
        // Restore metadata if requested
        if self.preserve_metadata {
            // Set file times (modified and accessed)
            if let Ok(modified) = metadata.modified() {
                let _ = filetime::set_file_mtime(&canonical_dest, 
                    filetime::FileTime::from_system_time(modified));
            }
            if let Ok(accessed) = metadata.accessed() {
                let _ = filetime::set_file_atime(&canonical_dest, 
                    filetime::FileTime::from_system_time(accessed));
            }
            
            // Set permissions on Unix-like systems
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&canonical_dest, 
                    std::fs::Permissions::from_mode(metadata.permissions().mode())).await;
            }
        }
        
        // Update tracking in context
        // Remove source from read/written files
        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        
        let written_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut written_files_clone = (*written_files).clone();
        
        // If source was tracked, track destination instead
        if read_files_clone.remove(&canonical_source) {
            read_files_clone.insert(canonical_dest.clone());
            context.set_custom_state(read_files_clone).await;
        }
        
        if written_files_clone.remove(&canonical_source) {
            written_files_clone.insert(canonical_dest.clone());
            context.set_custom_state(written_files_clone).await;
        }
        
        let _duration = start_time.elapsed();
        let file_type = if is_dir { "directory" } else { "file" };
        
        // Format paths relative to project root
        let source_relative = canonical_source.strip_prefix(&current_dir)
            .unwrap_or(&canonical_source);
        let dest_relative = canonical_dest.strip_prefix(&current_dir)
            .unwrap_or(&canonical_dest);
        
        // Build metrics string
        let mut metrics_parts = Vec::new();
        if !is_dir && total_size > 0 {
            metrics_parts.push(format_size(total_size));
        }
        if self.preserve_metadata {
            metrics_parts.push("metadata preserved".to_string());
        }
        
        let metrics = if !metrics_parts.is_empty() {
            format!(" ({})", metrics_parts.join(", "))
        } else {
            String::new()
        };
        
        let message = format!(
            "Moved {} {} to {}{}",
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

impl MoveTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}