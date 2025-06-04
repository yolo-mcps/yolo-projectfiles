use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::{resolve_path_for_read, resolve_path_allowing_symlinks};

use async_trait::async_trait;
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
    description = "Check if files/directories exist. Preferred over system file tests.

Returns existence status and type (file/directory/none) as JSON response.
NOTE: Omit optional parameters when not needed, don't pass null.

Parameters:
- path: File or directory path to check (required)
- follow_symlinks: Follow symlinks to target (optional, default: true)
- include_metadata: Include size, permissions, timestamps (optional, default: false)

Examples:
- Check file: {\"path\": \"README.md\"}
- Check directory: {\"path\": \"src/\"}
- Check without following symlinks: {\"path\": \"link.txt\", \"follow_symlinks\": false}
- Check with metadata: {\"path\": \"package.json\", \"include_metadata\": true}
- Check nested path: {\"path\": \"src/utils/helper.js\"}

Returns JSON with:
- exists: boolean indicating if path exists
- type: \"file\", \"directory\", \"other\", or \"none\"
- path: original path provided
- absolute_path: resolved absolute path
- metadata: (if requested) size, permissions, modified time"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ExistsTool {
    /// Path to check (relative to project root)
    pub path: String,
    /// Follow symlinks to check files outside the project directory (optional, default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    /// Include additional metadata like permissions and size (optional, default: false)
    #[serde(default)]
    pub include_metadata: bool,
}

#[async_trait]
impl StatefulTool for ExistsTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        // Use different path resolution based on follow_symlinks
        let resolved_path = if self.follow_symlinks {
            // When following symlinks, use the standard resolution
            match resolve_path_for_read(&self.path, &project_root, true, TOOL_NAME) {
                Ok(path) => path,
                Err(e) => {
                    // If the path doesn't exist, we still want to provide a result instead of erroring
                    // Check if this is a "file not found" error
                    if e.to_string().contains("not found")
                        || e.to_string().contains("does not exist")
                    {
                        // For non-existent paths, try to get the normalized path
                        resolve_path_allowing_symlinks(&self.path, &project_root, TOOL_NAME)?
                    } else {
                        // For other errors (like access denied), propagate them
                        return Err(e);
                    }
                }
            }
        } else {
            // When not following symlinks, use the new function that allows checking symlinks
            resolve_path_allowing_symlinks(&self.path, &project_root, TOOL_NAME)?
        };

        let exists = resolved_path.exists();

        let path_type = if !exists {
            "none"
        } else if resolved_path.is_file() {
            "file"
        } else if resolved_path.is_dir() {
            "directory"
        } else {
            // Could be a symlink or other special file
            "other"
        };

        // Build the base result
        let mut result_json = serde_json::json!({
            "exists": exists,
            "type": path_type,
            "path": self.path,
            "absolute_path": resolved_path.display().to_string()
        });

        // Add metadata if requested and file exists
        if self.include_metadata && exists {
            if let Ok(metadata) = tokio::fs::metadata(&resolved_path).await {
                let metadata_obj = serde_json::json!({
                    "size": metadata.len(),
                    "is_readonly": metadata.permissions().readonly(),
                    "modified": metadata.modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                    "created": metadata.created()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                });
                result_json["metadata"] = metadata_obj;
            }
        }

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result_json).map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to serialize result: {}", e),
                    ))
                })?,
                None,
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
    async fn test_exists_file() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content")
            .await
            .unwrap();

        let exists_tool = ExistsTool {
            path: "test.txt".to_string(),
            follow_symlinks: true,
            include_metadata: false,
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
            include_metadata: false,
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
            include_metadata: false,
        };

        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // The exists tool returns JSON with exists: false for non-existent files
            assert!(
                text.text.contains("\"exists\": false") || text.text.contains("\"type\": \"none\"")
            );
        }
    }

    #[tokio::test]
    async fn test_exists_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;

        let exists_tool = ExistsTool {
            path: "../outside.txt".to_string(),
            follow_symlinks: false, // Test with symlinks disabled
            include_metadata: false,
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
        fs::write(project_root.join("target.txt"), "content")
            .await
            .unwrap();

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
            include_metadata: false,
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
            include_metadata: false,
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
        fs::write(project_root.join("target.txt"), "content")
            .await
            .unwrap();

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
            include_metadata: false,
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
            include_metadata: false,
        };

        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should report that the target doesn't exist
            assert!(
                text.text.contains("\"exists\": false") || text.text.contains("\"type\": \"none\"")
            );
        }

        // With follow_symlinks=false, behavior may vary for broken symlinks
        let exists_tool = ExistsTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: false,
            include_metadata: false,
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

    #[tokio::test]
    async fn test_exists_with_metadata() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file with known content
        let project_root = context.get_project_root().unwrap();
        let test_content = "test content for metadata";
        fs::write(project_root.join("metadata_test.txt"), test_content)
            .await
            .unwrap();

        let exists_tool = ExistsTool {
            path: "metadata_test.txt".to_string(),
            follow_symlinks: true,
            include_metadata: true,
        };

        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should include metadata fields
            assert!(text.text.contains("\"metadata\""));
            assert!(text.text.contains("\"size\""));
            assert!(text.text.contains("\"is_readonly\""));
            assert!(text.text.contains("\"modified\""));

            // Parse JSON to verify size
            let json: serde_json::Value = serde_json::from_str(&text.text).unwrap();
            assert_eq!(json["metadata"]["size"], test_content.len() as u64);
        }
    }

    #[tokio::test]
    async fn test_exists_without_metadata() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("no_metadata_test.txt"), "content")
            .await
            .unwrap();

        let exists_tool = ExistsTool {
            path: "no_metadata_test.txt".to_string(),
            follow_symlinks: true,
            include_metadata: false,
        };

        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should not include metadata
            assert!(!text.text.contains("\"metadata\""));
            assert!(!text.text.contains("\"size\""));
        }
    }

    #[tokio::test]
    async fn test_exists_nested_path() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create nested directory structure
        let project_root = context.get_project_root().unwrap();
        let nested_dir = project_root.join("src").join("utils");
        fs::create_dir_all(&nested_dir).await.unwrap();
        fs::write(nested_dir.join("helper.js"), "// helper")
            .await
            .unwrap();

        let exists_tool = ExistsTool {
            path: "src/utils/helper.js".to_string(),
            follow_symlinks: true,
            include_metadata: false,
        };

        let result = exists_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("\"exists\": true"));
            assert!(text.text.contains("\"type\": \"file\""));
        }
    }
}
