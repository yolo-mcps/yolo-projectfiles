use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[mcp_tool(
    name = "chmod", 
    description = "Changes file/directory permissions within the project directory only. Unix-like systems only. Uses octal mode format (e.g., '755' for rwxr-xr-x, '644' for rw-r--r--). Supports recursive application for directories."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ChmodTool {
    /// Path to the file or directory (relative to project root)
    pub path: String,
    /// Permissions mode in octal format (e.g., "755", "644")
    pub mode: String,
    /// Whether to apply permissions recursively to directories (default: false)
    #[serde(default)]
    pub recursive: bool,
}

impl ChmodTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Check if we're on a Unix-like system
        #[cfg(not(unix))]
        {
            return Err(CallToolError::unknown_tool(
                "chmod is only available on Unix-like systems".to_string()
            ));
        }
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            
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
                    "Path not found: {}",
                    self.path
                )));
            }
            
            // Parse the mode
            let mode = u32::from_str_radix(&self.mode, 8)
                .map_err(|_| CallToolError::unknown_tool(format!(
                    "Invalid mode '{}'. Must be an octal number like '755' or '644'",
                    self.mode
                )))?;
            
            let metadata = fs::metadata(&canonical_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
            
            let mut changed_count = 0;
            
            if metadata.is_file() || (metadata.is_dir() && !self.recursive) {
                // Single file or non-recursive directory
                let permissions = std::fs::Permissions::from_mode(mode);
                fs::set_permissions(&canonical_path, permissions)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to set permissions: {}", e)))?;
                changed_count = 1;
            } else if metadata.is_dir() && self.recursive {
                // Recursive directory permissions
                changed_count = chmod_recursive(&canonical_path, mode).await?;
            }
            
            let message = if self.recursive && changed_count > 1 {
                format!(
                    "Successfully changed permissions to {} for '{}' ({} items)",
                    self.mode, self.path, changed_count
                )
            } else {
                format!(
                    "Successfully changed permissions to {} for '{}'",
                    self.mode, self.path
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
}

#[cfg(unix)]
fn chmod_recursive(path: &Path, mode: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, CallToolError>> + Send + '_>> {
    Box::pin(async move {
    use std::os::unix::fs::PermissionsExt;
    
    let mut count = 1;
    
    // Set permissions on the directory itself
    let permissions = std::fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
        .await
        .map_err(|e| CallToolError::unknown_tool(format!("Failed to set permissions: {}", e)))?;
    
    // Read directory entries
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| CallToolError::unknown_tool(format!("Failed to read directory: {}", e)))?;
    
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let entry_path = entry.path();
                let file_type = entry.file_type().await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to get file type: {}", e)))?;
                
                if file_type.is_dir() {
                    count += Box::pin(chmod_recursive(&entry_path, mode)).await?;
                } else {
                    let permissions = std::fs::Permissions::from_mode(mode);
                    fs::set_permissions(&entry_path, permissions)
                        .await
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to set permissions: {}", e)))?;
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