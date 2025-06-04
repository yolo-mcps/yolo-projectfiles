use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_path, format_count};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use glob::{glob_with, MatchOptions};

const TOOL_NAME: &str = "chmod";

#[mcp_tool(
    name = "chmod", 
    description = "Changes file/directory permissions within the project directory only. Unix-like systems only. Uses octal mode format (e.g., '755' for rwxr-xr-x, '644' for rw-r--r--). Supports recursive application for directories. Prefer this over system 'chmod' command when modifying project file permissions."
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
    /// Pattern matching mode - treat path as a glob pattern for bulk operations (default: false)
    #[serde(default)]
    pub pattern: bool,
}

#[async_trait]
impl StatefulTool for ChmodTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Check if we're on a Unix-like system
        #[cfg(not(unix))]
        {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME,
                "chmod is only available on Unix-like systems"
            )));
        }
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            
            let project_root = context.get_project_root()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
                
            // Canonicalize project root for consistent path comparison
            let current_dir = project_root.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e))))?;
            
            if self.pattern {
                // Pattern matching mode - treat path as glob pattern
                let pattern_path = if Path::new(&self.path).is_absolute() {
                    self.path.clone()
                } else {
                    format!("{}/{}", current_dir.display(), self.path)
                };
                
                let options = MatchOptions {
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                    ..Default::default()
                };
                
                let paths: Vec<_> = glob_with(&pattern_path, options)
                    .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, &self.path, &e.to_string())))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to expand pattern: {}", e))))?;
                
                if paths.is_empty() {
                    return Err(CallToolError::from(tool_errors::file_not_found(
                        TOOL_NAME,
                        &format!("No files found matching pattern: {}", self.path)
                    )));
                }
                
                // Parse the mode
                let mode = u32::from_str_radix(&self.mode, 8)
                    .map_err(|_| CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Invalid mode '{}'. Must be an octal number like '755' or '644'", self.mode)
                    )))?;
                
                let mut changed_paths = Vec::new();
                let mut _total_changed = 0;
                
                for path in paths {
                    // Security check: ensure path is within project directory
                    let canonical_path = path.canonicalize()
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path '{}': {}", path.display(), e))))?;
                    
                    if !canonical_path.starts_with(&current_dir) {
                        continue; // Skip paths outside project directory
                    }
                    
                    // Apply chmod
                    let metadata = fs::metadata(&canonical_path).await
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata for '{}': {}", path.display(), e))))?;
                    
                    let changed_count = if metadata.is_file() || (metadata.is_dir() && !self.recursive) {
                        let permissions = std::fs::Permissions::from_mode(mode);
                        fs::set_permissions(&canonical_path, permissions).await
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set permissions for '{}': {}", path.display(), e))))?;
                        1
                    } else if metadata.is_dir() && self.recursive {
                        chmod_recursive(&canonical_path, mode).await?
                    } else {
                        0
                    };
                    
                    if changed_count > 0 {
                        changed_paths.push(path.display().to_string());
                        _total_changed += changed_count;
                    }
                }
                
                let summary = format!(
                    "Changed permissions to {} for {} matching pattern '{}':\n{}",
                    self.mode,
                    format_count(changed_paths.len(), "path", "paths"),
                    self.path,
                    changed_paths.iter()
                        .map(|p| format!("  {}", format_path(Path::new(p))))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                
                return Ok(CallToolResult {
                    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                        summary, None,
                    ))],
                    is_error: None,
                    meta: None,
                });
            }
            
            // Single path mode (existing logic)
            let requested_path = Path::new(&self.path);
            let absolute_path = if requested_path.is_absolute() {
                requested_path.to_path_buf()
            } else {
                current_dir.join(requested_path)
            };
            
            let canonical_path = absolute_path.canonicalize()
                .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.path)))?;
            
            if !canonical_path.starts_with(&current_dir) {
                return Err(CallToolError::from(tool_errors::access_denied(
                    TOOL_NAME,
                    &self.path,
                    "Path is outside the project directory"
                )));
            }
            
            if !canonical_path.exists() {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &self.path
                )));
            }
            
            // Parse the mode
            let mode = u32::from_str_radix(&self.mode, 8)
                .map_err(|_| CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Invalid mode '{}'. Must be an octal number like '755' or '644'", self.mode)
                )))?;
            
            let metadata = fs::metadata(&canonical_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata: {}", e))))?;
            
            let mut changed_count = 0;
            
            if metadata.is_file() || (metadata.is_dir() && !self.recursive) {
                // Single file or non-recursive directory
                let permissions = std::fs::Permissions::from_mode(mode);
                fs::set_permissions(&canonical_path, permissions)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set permissions: {}", e))))?;
                changed_count = 1;
            } else if metadata.is_dir() && self.recursive {
                // Recursive directory permissions
                changed_count = chmod_recursive(&canonical_path, mode).await?;
            }
            
            // Format path relative to project root
            let relative_path = canonical_path.strip_prefix(&current_dir)
                .unwrap_or(&canonical_path);
            
            let message = if self.recursive && changed_count > 1 {
                format!(
                    "Changed permissions to {} for {} ({})",
                    self.mode, 
                    format_path(relative_path), 
                    format_count(changed_count, "item", "items")
                )
            } else {
                format!(
                    "Changed permissions to {} for {}",
                    self.mode, 
                    format_path(relative_path)
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
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set permissions: {}", e))))?;
    
    // Read directory entries
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;
    
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let entry_path = entry.path();
                let file_type = entry.file_type().await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file type: {}", e))))?;
                
                if file_type.is_dir() {
                    count += Box::pin(chmod_recursive(&entry_path, mode)).await?;
                } else {
                    let permissions = std::fs::Permissions::from_mode(mode);
                    fs::set_permissions(&entry_path, permissions)
                        .await
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set permissions: {}", e))))?;
                    count += 1;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read entry: {}", e)))),
        }
    }
    
    Ok(count)
    })
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
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_basic_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("test.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        let chmod_tool = ChmodTool {
            path: "test.txt".to_string(),
            mode: "644".to_string(),
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check permissions were set
        let metadata = fs::metadata(&file_path).await.unwrap();
        let permissions = metadata.permissions();
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode() & 0o777;
        assert_eq!(mode, 0o644);
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Changed permissions to 644"));
            assert!(text.text.contains("test.txt"));
        }
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_directory_recursive() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create directory structure
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("test_dir");
        fs::create_dir(&dir_path).await.unwrap();
        fs::write(dir_path.join("file1.txt"), "content1").await.unwrap();
        fs::write(dir_path.join("file2.txt"), "content2").await.unwrap();
        
        let sub_dir = dir_path.join("subdir");
        fs::create_dir(&sub_dir).await.unwrap();
        fs::write(sub_dir.join("file3.txt"), "content3").await.unwrap();
        
        let chmod_tool = ChmodTool {
            path: "test_dir".to_string(),
            mode: "755".to_string(),
            recursive: true,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check permissions on directory and files
        use std::os::unix::fs::PermissionsExt;
        
        let dir_metadata = fs::metadata(&dir_path).await.unwrap();
        let dir_mode = dir_metadata.permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o755);
        
        let file_metadata = fs::metadata(dir_path.join("file1.txt")).await.unwrap();
        let file_mode = file_metadata.permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o755);
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Changed permissions to 755"));
            assert!(text.text.contains("test_dir"));
            assert!(text.text.contains("items") || text.text.contains("item"));
        }
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_with_pattern() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create multiple test files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test1.txt"), "content1").await.unwrap();
        fs::write(project_root.join("test2.txt"), "content2").await.unwrap();
        fs::write(project_root.join("other.log"), "content3").await.unwrap();
        
        let chmod_tool = ChmodTool {
            path: "test*.txt".to_string(),
            mode: "600".to_string(),
            recursive: false,
            pattern: true,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check permissions on matching files
        use std::os::unix::fs::PermissionsExt;
        
        let file1_metadata = fs::metadata(project_root.join("test1.txt")).await.unwrap();
        let file1_mode = file1_metadata.permissions().mode() & 0o777;
        assert_eq!(file1_mode, 0o600);
        
        let file2_metadata = fs::metadata(project_root.join("test2.txt")).await.unwrap();
        let file2_mode = file2_metadata.permissions().mode() & 0o777;
        assert_eq!(file2_mode, 0o600);
        
        // Non-matching file should be unchanged (check it exists but don't check permissions)
        assert!(project_root.join("other.log").exists());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Changed permissions to 600"));
            assert!(text.text.contains("test*.txt"));
        }
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_invalid_mode() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content").await.unwrap();
        
        let chmod_tool = ChmodTool {
            path: "test.txt".to_string(),
            mode: "999".to_string(), // Invalid octal mode
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Invalid mode") || error_msg.contains("octal"));
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_nonexistent_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let chmod_tool = ChmodTool {
            path: "nonexistent.txt".to_string(),
            mode: "644".to_string(),
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let chmod_tool = ChmodTool {
            path: "../outside.txt".to_string(),
            mode: "644".to_string(),
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[cfg(not(unix))]
    #[tokio::test]
    async fn test_chmod_non_unix_system() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let chmod_tool = ChmodTool {
            path: "test.txt".to_string(),
            mode: "644".to_string(),
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Unix-like systems"));
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_chmod_directory_non_recursive() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create directory with files
        let project_root = context.get_project_root().unwrap();
        let dir_path = project_root.join("test_dir");
        fs::create_dir(&dir_path).await.unwrap();
        fs::write(dir_path.join("file.txt"), "content").await.unwrap();
        
        let chmod_tool = ChmodTool {
            path: "test_dir".to_string(),
            mode: "700".to_string(),
            recursive: false,
            pattern: false,
        };
        
        let result = chmod_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check only directory permissions changed, not file
        use std::os::unix::fs::PermissionsExt;
        
        let dir_metadata = fs::metadata(&dir_path).await.unwrap();
        let dir_mode = dir_metadata.permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        
        // File permissions should be unchanged (original permissions)
        let file_metadata = fs::metadata(dir_path.join("file.txt")).await.unwrap();
        let file_mode = file_metadata.permissions().mode() & 0o777;
        assert_ne!(file_mode, 0o700); // Should not be changed to 700
    }
}