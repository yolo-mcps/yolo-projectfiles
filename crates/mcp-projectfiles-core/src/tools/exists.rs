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

#[mcp_tool(
    name = "exists",
    description = "Checks if a file or directory exists within the project directory. Returns existence status and type (file/directory/none)."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ExistsTool {
    /// Path to check (relative to project root)
    pub path: String,
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
        
        // Note: We don't use canonicalize() here because it fails if the path doesn't exist
        // Instead, we normalize the path and check if it's within the project
        let normalized_path = normalize_path(&absolute_path);
        
        if !normalized_path.starts_with(&project_root) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory"
            )));
        }
        
        let exists = normalized_path.exists();
        
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
        };
        
        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
}

