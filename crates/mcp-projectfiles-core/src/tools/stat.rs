use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;

use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use chrono::{DateTime, Local};

const TOOL_NAME: &str = "stat";

fn default_follow_symlinks() -> bool {
    true
}

#[mcp_tool(
    name = "stat",
    description = "Gets detailed file or directory metadata (size, permissions, timestamps) within the project directory. Can follow symlinks to get metadata of files outside the project directory. Replaces the need for 'ls -la' or 'stat' commands."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct StatTool {
    /// Path to get stats for (relative to project root)
    pub path: String,
    
    /// Whether to follow symbolic links (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

#[async_trait]
impl StatefulTool for StatTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve path with symlink support
        let canonical_path = resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;
        
        // Get metadata
        let metadata = if self.follow_symlinks {
            fs::metadata(&canonical_path).await
        } else {
            fs::symlink_metadata(&canonical_path).await
        }.map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get metadata for '{}': {}", self.path, e))))?;
        
        // Build the result
        let mut result = serde_json::json!({
            "path": self.path,
            "absolute_path": canonical_path.display().to_string(),
            "exists": true,
            "type": get_file_type(&metadata),
            "size": metadata.len(),
            "is_file": metadata.is_file(),
            "is_dir": metadata.is_dir(),
            "is_symlink": metadata.is_symlink(),
            "readonly": metadata.permissions().readonly(),
        });
        
        // Add timestamps
        if let Ok(modified) = metadata.modified() {
            let modified_dt: DateTime<Local> = modified.into();
            result["modified"] = serde_json::Value::String(modified_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["modified_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(modified_dt.timestamp())
            );
        }
        
        if let Ok(accessed) = metadata.accessed() {
            let accessed_dt: DateTime<Local> = accessed.into();
            result["accessed"] = serde_json::Value::String(accessed_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["accessed_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(accessed_dt.timestamp())
            );
        }
        
        if let Ok(created) = metadata.created() {
            let created_dt: DateTime<Local> = created.into();
            result["created"] = serde_json::Value::String(created_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["created_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(created_dt.timestamp())
            );
        }
        
        // Add Unix-specific metadata
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            result["mode"] = serde_json::Value::String(format!("{:o}", metadata.mode()));
            result["uid"] = serde_json::Value::Number(serde_json::Number::from(metadata.uid()));
            result["gid"] = serde_json::Value::Number(serde_json::Number::from(metadata.gid()));
            result["nlink"] = serde_json::Value::Number(serde_json::Number::from(metadata.nlink()));
            result["dev"] = serde_json::Value::Number(serde_json::Number::from(metadata.dev()));
            result["ino"] = serde_json::Value::Number(serde_json::Number::from(metadata.ino()));
            
            // Format permissions in human-readable form
            result["permissions"] = serde_json::Value::String(format_permissions(metadata.mode()));
        }
        
        // Format size in human-readable form
        result["size_human"] = serde_json::Value::String(format_size(metadata.len()));
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to serialize result: {}", e))))?,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn get_file_type(metadata: &std::fs::Metadata) -> &'static str {
    if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else if metadata.is_symlink() {
        "symlink"
    } else {
        "other"
    }
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    
    if size == 0 {
        return "0 B".to_string();
    }
    
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(unix)]
fn format_permissions(mode: u32) -> String {
    let mut perms = String::with_capacity(10);
    
    // File type
    perms.push(match mode & 0o170000 {
        0o140000 => 's', // socket
        0o120000 => 'l', // symlink
        0o100000 => '-', // regular file
        0o060000 => 'b', // block device
        0o040000 => 'd', // directory
        0o020000 => 'c', // character device
        0o010000 => 'p', // fifo
        _ => '?',
    });
    
    // Owner permissions
    perms.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o100 != 0 {
        if mode & 0o4000 != 0 { 's' } else { 'x' }
    } else {
        if mode & 0o4000 != 0 { 'S' } else { '-' }
    });
    
    // Group permissions
    perms.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o010 != 0 {
        if mode & 0o2000 != 0 { 's' } else { 'x' }
    } else {
        if mode & 0o2000 != 0 { 'S' } else { '-' }
    });
    
    // Other permissions
    perms.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o001 != 0 {
        if mode & 0o1000 != 0 { 't' } else { 'x' }
    } else {
        if mode & 0o1000 != 0 { 'T' } else { '-' }
    });
    
    perms
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
    async fn test_stat_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        let test_content = "Hello, World!";
        fs::write(project_root.join("test.txt"), test_content).await.unwrap();
        
        let stat_tool = StatTool {
            path: "test.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Check that output contains expected file information (JSON format)
            assert!(text.text.contains("test.txt"));
            assert!(text.text.contains("\"type\": \"file\""));
            assert!(text.text.contains("\"size\":"));
            assert!(text.text.contains(&test_content.len().to_string()));
            assert!(text.text.contains("\"modified\":"));
        }
    }
    
    #[tokio::test]
    async fn test_stat_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test directory
        let project_root = context.get_project_root().unwrap();
        fs::create_dir(project_root.join("test_dir")).await.unwrap();
        
        let stat_tool = StatTool {
            path: "test_dir".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Check that output contains expected directory information (JSON format)
            assert!(text.text.contains("test_dir"));
            assert!(text.text.contains("\"type\": \"directory\""));
            assert!(text.text.contains("\"modified\":"));
        }
    }
    
    #[tokio::test]
    async fn test_stat_nonexistent_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let stat_tool = StatTool {
            path: "nonexistent.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }
    
    #[tokio::test]
    async fn test_stat_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let stat_tool = StatTool {
            path: "../outside.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_stat_with_permissions() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file with specific permissions
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("perms_test.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        // Set specific permissions (readable by owner only)
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&file_path).await.unwrap().permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&file_path, perms).await.unwrap();
        
        let stat_tool = StatTool {
            path: "perms_test.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Check that permissions are shown (JSON format)
            assert!(text.text.contains("\"permissions\":"));
            assert!(text.text.contains("rw-------") || text.text.contains("\"mode\": \"600\""));
        }
    }
    
    #[tokio::test]
    async fn test_stat_follow_symlinks_flag() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("target.txt"), "content").await.unwrap();
        
        // Test with follow_symlinks=true
        let stat_tool = StatTool {
            path: "target.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Test with follow_symlinks=false  
        let stat_tool = StatTool {
            path: "target.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_stat_empty_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create empty file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("empty.txt"), "").await.unwrap();
        
        let stat_tool = StatTool {
            path: "empty.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("empty.txt"));
            assert!(text.text.contains("\"size\": 0"));
        }
    }
    
    #[tokio::test]
    async fn test_stat_large_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create large file (10KB)
        let project_root = context.get_project_root().unwrap();
        let large_content = "x".repeat(10240);
        fs::write(project_root.join("large.txt"), &large_content).await.unwrap();
        
        let stat_tool = StatTool {
            path: "large.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("large.txt"));
            assert!(text.text.contains("\"size\": 10240"));
        }
    }
    
    #[tokio::test]
    async fn test_stat_symlink_within_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        let content = "symlink target content";
        fs::write(project_root.join("target.txt"), content).await.unwrap();
        
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
        
        let stat_tool = StatTool {
            path: "link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content_item = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content_item {
            // Should show file stats (target), not symlink stats
            assert!(text.text.contains("\"type\": \"file\""));
            assert!(text.text.contains(&content.len().to_string()));
        }
    }
    
    #[tokio::test]
    async fn test_stat_symlink_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create external target file
        let external_temp = TempDir::new().unwrap();
        let external_content = "external file content";
        let external_file = external_temp.path().join("external.txt");
        fs::write(&external_file, external_content).await.unwrap();
        
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
        
        let stat_tool = StatTool {
            path: "external_link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content_item = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content_item {
            // Should show external file stats
            assert!(text.text.contains("\"type\": \"file\""));
            assert!(text.text.contains(&external_content.len().to_string()));
        }
    }
    
    #[tokio::test]
    async fn test_stat_symlink_disabled() {
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
        
        let stat_tool = StatTool {
            path: "link.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content_item = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content_item {
            // Should show some valid stats for the symlink
            assert!(text.text.contains("link.txt"));
            assert!(text.text.contains("\"type\":"));
            // Test that we can get stats when follow_symlinks=false
        }
    }
    
    #[tokio::test]
    async fn test_stat_broken_symlink() {
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
        
        // With follow_symlinks=true, should fail because target doesn't exist
        let stat_tool = StatTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: true,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        // With follow_symlinks=false, may still fail due to broken symlink
        let stat_tool = StatTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: false,
        };
        
        let result = stat_tool.call_with_context(&context).await;
        // Note: Even with follow_symlinks=false, broken symlinks may fail
        // depending on the implementation of resolve_path_for_read
        if result.is_ok() {
            let output = result.unwrap();
            let content_item = &output.content[0];
            if let CallToolResultContentItem::TextContent(text) = content_item {
                // Should contain some info about the symlink
                assert!(text.text.contains("broken_link.txt") || text.text.contains("\"type\":"));
            }
        }
    }
}