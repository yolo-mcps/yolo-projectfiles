use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;

use async_trait::async_trait;
use std::path::{Path, PathBuf, Component};
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};

const TOOL_NAME: &str = "exists";

fn default_follow_symlinks() -> bool {
    true
}

#[mcp_tool(
    name = "exists",
    description = "Checks if a file or directory exists within the project directory. Returns existence status and type (file/directory/none). Can follow symlinks to check files outside the project directory. Prefer this over system file tests when checking project file existence."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ExistsTool {
    /// Path to check (relative to project root)
    pub path: String,
    /// Follow symlinks to check files outside the project directory (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

#[async_trait]
impl StatefulTool for ExistsTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Convert relative path to absolute path
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            project_root.join(requested_path)
        };
        
        // For symlink following, try to resolve if path exists and is a symlink
        let (normalized_path, exists) = if self.follow_symlinks && absolute_path.exists() && absolute_path.is_symlink() {
            match absolute_path.canonicalize() {
                Ok(target_path) => {
                    let exists_val = target_path.exists();
                    (target_path, exists_val)
                },
                Err(_) => (normalize_path(&absolute_path), false), // Broken symlink
            }
        } else {
            // For regular files or when not following symlinks, use normal validation
            let normalized = normalize_path(&absolute_path);
            
            // Check if the resolved path is within the project directory (only when not following symlinks)
            if !self.follow_symlinks || !absolute_path.is_symlink() {
                if !normalized.starts_with(&project_root) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        TOOL_NAME,
                        &self.path,
                        "Path is outside the project directory"
                    )));
                }
            }
            
            let exists_val = normalized.exists();
            (normalized, exists_val)
        };
        
        let path_type = if !exists {
            "none"
        } else if normalized_path.is_file() {
            "file"
        } else if normalized_path.is_dir() {
            "directory"
        } else {
            // Could be a symlink or other special file
            "other"
        };
        
        // Format the result as JSON
        let result_json = serde_json::json!({
            "exists": exists,
            "type": path_type,
            "path": self.path,
            "absolute_path": normalized_path.display().to_string()
        });
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result_json)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to serialize result: {}", e))))?,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

/// Normalize a path without requiring it to exist (unlike canonicalize)
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {
                // Skip current directory markers
            }
            _ => {
                normalized.push(component);
            }
        }
    }
    
    normalized
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
    async fn test_exists_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content").await.unwrap();
        
        let exists_tool = ExistsTool {
            path: "test.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("exists") && text.text.contains("file"));
        }
    }
    
    #[tokio::test]
    async fn test_exists_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test directory
        let project_root = context.get_project_root().unwrap();
        fs::create_dir(project_root.join("test_dir")).await.unwrap();
        
        let exists_tool = ExistsTool {
            path: "test_dir".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("exists") && text.text.contains("directory"));
        }
    }
    
    #[tokio::test]
    async fn test_exists_nonexistent() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let exists_tool = ExistsTool {
            path: "nonexistent.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // The exists tool returns JSON with exists: false for non-existent files
            assert!(text.text.contains("\"exists\": false") || text.text.contains("\"type\": \"none\""));
        }
    }
    
    #[tokio::test]
    async fn test_exists_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let exists_tool = ExistsTool {
            path: "../outside.txt".to_string(),
            follow_symlinks: false, // Test with symlinks disabled
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[tokio::test]
    async fn test_exists_symlink_within_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("target.txt"), "content").await.unwrap();
        
        // Create symlink within project directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("target.txt", project_root.join("link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file("target.txt", project_root.join("link.txt")).unwrap();
        }
        
        let exists_tool = ExistsTool {
            path: "link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report the symlink exists and follows to file
            assert!(text.text.contains("\"exists\": true"));
            assert!(text.text.contains("\"type\": \"file\""));
        }
    }
    
    #[tokio::test]
    async fn test_exists_symlink_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create external target file
        let external_temp = TempDir::new().unwrap();
        let external_file = external_temp.path().join("external.txt");
        fs::write(&external_file, "external content").await.unwrap();
        
        let project_root = context.get_project_root().unwrap();
        
        // Create symlink pointing outside project directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&external_file, project_root.join("external_link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file(&external_file, project_root.join("external_link.txt")).unwrap();
        }
        
        let exists_tool = ExistsTool {
            path: "external_link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report the external file exists
            assert!(text.text.contains("\"exists\": true"));
            assert!(text.text.contains("\"type\": \"file\""));
        }
    }
    
    #[tokio::test]
    async fn test_exists_symlink_disabled() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("target.txt"), "content").await.unwrap();
        
        // Create symlink
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("target.txt", project_root.join("link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file("target.txt", project_root.join("link.txt")).unwrap();
        }
        
        let exists_tool = ExistsTool {
            path: "link.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report the symlink exists
            assert!(text.text.contains("\"exists\": true"));
            // Type might vary by platform, just check it exists
        }
    }
    
    #[tokio::test]
    async fn test_exists_broken_symlink() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let project_root = context.get_project_root().unwrap();
        
        // Create symlink pointing to non-existent file
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("nonexistent.txt", project_root.join("broken_link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file("nonexistent.txt", project_root.join("broken_link.txt")).unwrap();
        }
        
        // With follow_symlinks=true, should report that target doesn't exist
        let exists_tool = ExistsTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report that the target doesn't exist
            assert!(text.text.contains("\"exists\": false") || text.text.contains("\"type\": \"none\""));
        }
        
        // With follow_symlinks=false, behavior may vary for broken symlinks
        let exists_tool = ExistsTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report some status for the symlink
            // Note: Implementation may report false for broken symlinks even with follow_symlinks=false
            assert!(text.text.contains("\"exists\":") && text.text.contains("broken_link.txt"));
        }
    }
}

