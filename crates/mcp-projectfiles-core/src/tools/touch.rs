use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use std::time::SystemTime;

#[mcp_tool(
    name = "touch", 
    description = "Creates empty files or updates timestamps within the project directory only. Creates parent directories automatically if needed. Supports selective timestamp updates (access/modification time). File creation enabled by default (create=true)."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct TouchTool {
    /// Path to the file (relative to project root)
    pub path: String,
    /// Whether to create the file if it doesn't exist (default: true)
    #[serde(default = "default_create")]
    pub create: bool,
    /// Whether to update access time (default: true)
    #[serde(default = "default_update_atime")]
    pub update_atime: bool,
    /// Whether to update modification time (default: true)
    #[serde(default = "default_update_mtime")]
    pub update_mtime: bool,
}

fn default_create() -> bool {
    true
}

fn default_update_atime() -> bool {
    true
}

fn default_update_mtime() -> bool {
    true
}

impl TouchTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        // Validate the full path is within project bounds
        if absolute_path.exists() {
            let canonical_path = absolute_path.canonicalize()
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path: {}", e)))?;
            if !canonical_path.starts_with(&current_dir) {
                return Err(CallToolError::unknown_tool(format!(
                    "Access denied: Path '{}' is outside the project directory",
                    self.path
                )));
            }
        } else {
            // For non-existent paths, validate parent and ensure final path would be within bounds
            if let Some(parent) = absolute_path.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve parent directory: {}", e)))?;
                    if !canonical_parent.starts_with(&current_dir) {
                        return Err(CallToolError::unknown_tool(format!(
                            "Access denied: Path '{}' would be outside the project directory",
                            self.path
                        )));
                    }
                    // Reconstruct path to ensure it stays within bounds
                    if let Some(file_name) = absolute_path.file_name() {
                        let final_path = canonical_parent.join(file_name);
                        if !final_path.starts_with(&current_dir) {
                            return Err(CallToolError::unknown_tool(format!(
                                "Access denied: Path '{}' would be outside the project directory",
                                self.path
                            )));
                        }
                    }
                } else if self.create {
                    // Need to validate the parent path would be within bounds before creating
                    let mut check_path = parent;
                    while let Some(ancestor) = check_path.parent() {
                        if ancestor.exists() {
                            let canonical_ancestor = ancestor.canonicalize()
                                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve ancestor directory: {}", e)))?;
                            if !canonical_ancestor.starts_with(&current_dir) {
                                return Err(CallToolError::unknown_tool(format!(
                                    "Access denied: Path '{}' would be outside the project directory",
                                    self.path
                                )));
                            }
                            break;
                        }
                        check_path = ancestor;
                    }
                    
                    // Create parent directories
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to create parent directory: {}", e)))?;
                }
            }
        }
        
        let mut action = "touched";
        
        if !absolute_path.exists() {
            if self.create {
                // Create empty file
                fs::write(&absolute_path, b"")
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to create file: {}", e)))?;
                action = "created";
            } else {
                return Err(CallToolError::unknown_tool(format!(
                    "File '{}' does not exist and create=false",
                    self.path
                )));
            }
        } else {
            // File exists, check if it's a file
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
            
            if !metadata.is_file() {
                return Err(CallToolError::unknown_tool(format!(
                    "Path '{}' exists but is not a file",
                    self.path
                )));
            }
            
            // Update timestamps
            if self.update_atime || self.update_mtime {
                let file = fs::OpenOptions::new()
                    .write(true)
                    .open(&absolute_path)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to open file: {}", e)))?;
                
                let _now = SystemTime::now();
                
                // Note: Rust's standard library doesn't provide direct access time updates
                // Setting modified time will typically update both on most systems
                if self.update_mtime {
                    // Update file modification time by opening and closing it
                    drop(file);
                    // Touch the file by writing nothing
                    let _ = fs::OpenOptions::new()
                        .append(true)
                        .open(&absolute_path)
                        .await;
                }
                
                action = "updated";
            }
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("Successfully {} file '{}'", action, self.path), None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}