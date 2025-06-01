use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[mcp_tool(
    name = "mkdir", 
    description = "Creates directories within the project directory only. Creates parent directories by default (parents=true). Supports Unix permissions mode setting (e.g., '755'). Handles existing directories gracefully."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct MkdirTool {
    /// Path of the directory to create (relative to project root)
    pub path: String,
    /// Whether to create parent directories if they don't exist (default: true)
    #[serde(default = "default_create_parents")]
    pub parents: bool,
    /// File permissions mode in octal (e.g., "755"). Platform-specific.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

fn default_create_parents() -> bool {
    true
}

impl MkdirTool {
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
        // For existing paths, canonicalize them
        // For new paths, validate each component
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
            // For non-existent paths, check all parent components
            let mut check_path = absolute_path.as_path();
            while let Some(parent) = check_path.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve parent directory: {}", e)))?;
                    if !canonical_parent.starts_with(&current_dir) {
                        return Err(CallToolError::unknown_tool(format!(
                            "Access denied: Path '{}' would be outside the project directory",
                            self.path
                        )));
                    }
                    break;
                }
                check_path = parent;
            }
            
            // Also ensure the absolute path itself would be within bounds
            // This prevents creating directories with names like "../outside"
            let normalized = absolute_path.components()
                .fold(std::path::PathBuf::new(), |mut acc, comp| {
                    match comp {
                        std::path::Component::ParentDir => {
                            acc.pop();
                        }
                        std::path::Component::Normal(name) => {
                            acc.push(name);
                        }
                        std::path::Component::RootDir => {
                            acc.push("/");
                        }
                        _ => {}
                    }
                    acc
                });
            
            if !current_dir.join(&normalized).starts_with(&current_dir) {
                return Err(CallToolError::unknown_tool(format!(
                    "Access denied: Path '{}' would be outside the project directory",
                    self.path
                )));
            }
        }
        
        // Check if already exists
        if absolute_path.exists() {
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
            
            if metadata.is_dir() {
                return Ok(CallToolResult {
                    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                        format!("Directory '{}' already exists", self.path), None,
                    ))],
                    is_error: Some(false),
                    meta: None,
                });
            } else {
                return Err(CallToolError::unknown_tool(format!(
                    "Path '{}' already exists as a file",
                    self.path
                )));
            }
        }
        
        // Create the directory
        if self.parents {
            fs::create_dir_all(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to create directory: {}", e)))?;
        } else {
            fs::create_dir(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to create directory: {}", e)))?;
        }
        
        // Set permissions if specified (Unix-like systems only)
        #[cfg(unix)]
        if let Some(mode_str) = &self.mode {
            use std::os::unix::fs::PermissionsExt;
            
            if let Ok(mode) = u32::from_str_radix(mode_str, 8) {
                let permissions = std::fs::Permissions::from_mode(mode);
                fs::set_permissions(&absolute_path, permissions)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to set permissions: {}", e)))?;
            } else {
                return Err(CallToolError::unknown_tool(format!(
                    "Invalid mode '{}'. Must be an octal number like '755'",
                    mode_str
                )));
            }
        }
        
        let message = if self.parents {
            format!("Successfully created directory '{}' (with parents)", self.path)
        } else {
            format!("Successfully created directory '{}'", self.path)
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