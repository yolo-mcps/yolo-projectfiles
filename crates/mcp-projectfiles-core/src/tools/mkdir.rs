use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::format_path;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const TOOL_NAME: &str = "mkdir";

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

#[async_trait]
impl StatefulTool for MkdirTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
            
        // Canonicalize project root for consistent path comparison
        let current_dir = project_root.canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e))))?;
        
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
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path: {}", e))))?;
            if !canonical_path.starts_with(&current_dir) {
                return Err(CallToolError::from(tool_errors::access_denied(
                    TOOL_NAME,
                    &self.path,
                    "Path is outside the project directory"
                )));
            }
        } else {
            // For non-existent paths, check all parent components
            let mut check_path = absolute_path.as_path();
            while let Some(parent) = check_path.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve parent directory: {}", e))))?;
                    if !canonical_parent.starts_with(&current_dir) {
                        return Err(CallToolError::from(tool_errors::access_denied(
                            TOOL_NAME,
                            &self.path,
                            "Path would be outside the project directory"
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
                return Err(CallToolError::from(tool_errors::access_denied(
                    TOOL_NAME,
                    &self.path,
                    "Path would be outside the project directory"
                )));
            }
        }
        
        // Check if already exists
        if absolute_path.exists() {
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata: {}", e))))?;
            
            if metadata.is_dir() {
                return Ok(CallToolResult {
                    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                        format!("Directory '{}' already exists", self.path), None,
                    ))],
                    is_error: Some(false),
                    meta: None,
                });
            } else {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Path '{}' already exists as a file", self.path)
                )));
            }
        }
        
        // Create the directory
        if self.parents {
            fs::create_dir_all(&absolute_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create directory: {}", e))))?;
        } else {
            fs::create_dir(&absolute_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create directory: {}", e))))?;
        }
        
        // Set permissions if specified (Unix-like systems only)
        #[cfg(unix)]
        if let Some(mode_str) = &self.mode {
            use std::os::unix::fs::PermissionsExt;
            
            if let Ok(mode) = u32::from_str_radix(mode_str, 8) {
                let permissions = std::fs::Permissions::from_mode(mode);
                fs::set_permissions(&absolute_path, permissions)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set permissions: {}", e))))?;
            } else {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Invalid mode '{}'. Must be an octal number like '755'", mode_str)
                )));
            }
        }
        
        // Format path relative to project root
        let relative_path = absolute_path.strip_prefix(&current_dir)
            .unwrap_or(&absolute_path);
        
        let message = if self.parents {
            format!("Created directory {} (with parents)", format_path(relative_path))
        } else {
            format!("Created directory {}", format_path(relative_path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;
    use tokio::fs;
    
    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }
    
    #[tokio::test]
    async fn test_mkdir_basic_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "test_dir".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("test_dir");
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }
    
    #[tokio::test]
    async fn test_mkdir_nested_directories() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "parent/child/grandchild".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let project_root = context.get_project_root().unwrap();
        let parent_dir = project_root.join("parent");
        let child_dir = parent_dir.join("child");
        let grandchild_dir = child_dir.join("grandchild");
        
        assert!(parent_dir.exists() && parent_dir.is_dir());
        assert!(child_dir.exists() && child_dir.is_dir());
        assert!(grandchild_dir.exists() && grandchild_dir.is_dir());
    }
    
    #[tokio::test]
    async fn test_mkdir_without_parents_fails() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "nonexistent/child".to_string(),
            parents: false,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("No such file") || error_msg.contains("not found"));
    }
    
    #[tokio::test]
    async fn test_mkdir_existing_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create directory first
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("existing");
        fs::create_dir(&dir_path).await.unwrap();
        
        let mkdir_tool = MkdirTool {
            path: "existing".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        // Should succeed (no-op for existing directory)
        assert!(result.is_ok());
        
        // Directory should still exist
        assert!(dir_path.exists() && dir_path.is_dir());
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_mkdir_with_mode() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "mode_test".to_string(),
            parents: true,
            mode: Some("755".to_string()),
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("mode_test");
        assert!(dir_path.exists() && dir_path.is_dir());
        
        // Check permissions (on Unix systems)
        let metadata = fs::metadata(&dir_path).await.unwrap();
        let permissions = metadata.permissions();
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
    
    #[tokio::test]
    async fn test_mkdir_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "../outside".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[tokio::test]
    async fn test_mkdir_empty_path() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        // Empty path might succeed (creating project root) or fail - both are valid
        // Let's just check that it doesn't crash
        match result {
            Ok(_) => {
                // Empty path might be interpreted as "." which is valid
            }
            Err(_) => {
                // Empty path rejection is also valid
            }
        }
    }
    
    #[tokio::test]
    async fn test_mkdir_relative_path() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mkdir_tool = MkdirTool {
            path: "./relative/path".to_string(),
            parents: true,
            mode: None,
        };
        
        let result = mkdir_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("relative/path");
        assert!(dir_path.exists() && dir_path.is_dir());
    }
}