use std::path::{Path, PathBuf, Component};
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use super::utils::get_project_root_validated;

#[mcp_tool(
    name = "exists",
    description = "Checks if a file or directory exists within the project directory. Returns existence status and type (file/directory/none)."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ExistsTool {
    /// Path to check (relative to project root)
    pub path: String,
}

impl ExistsTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let project_root = get_project_root_validated()?;
        
        // Use validate_path but don't require the path to exist
        let absolute_path = crate::config::normalize_path(&self.path)
            .map_err(|e| CallToolError::unknown_tool(e))?;
        
        // Note: We don't use canonicalize() here because it fails if the path doesn't exist
        // Instead, we normalize the path and check if it's within the project
        let normalized_path = normalize_path(&absolute_path);
        
        if !normalized_path.starts_with(&project_root) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
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
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to serialize result: {}", e)))?,
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

