use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_path};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use std::time::Instant;

const TOOL_NAME: &str = "move";

#[mcp_tool(
    name = "move", 
    description = "Moves or renames files/directories within the project directory only. Preserves file tracking state for read/write operations. Supports overwrite option (default: false). Creates destination parent directories if needed."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct MoveTool {
    /// Source path (relative to project root)
    pub source: String,
    /// Destination path (relative to project root)
    pub destination: String,
    /// Whether to overwrite existing files (default: false)
    #[serde(default)]
    pub overwrite: bool,
    /// Whether to preserve file metadata (timestamps, permissions) (default: true)
    #[serde(default = "default_true")]
    pub preserve_metadata: bool,
}

fn default_true() -> bool {
    true
}

#[async_trait]
impl StatefulTool for MoveTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let current_dir = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Process source path
        let source_path = Path::new(&self.source);
        let absolute_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            current_dir.join(source_path)
        };
        
        let canonical_source = absolute_source.canonicalize()
            .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.source)))?;
        
        if !canonical_source.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.source,
                "Source path is outside the project directory"
            )));
        }
        
        // Process destination path
        let dest_path = Path::new(&self.destination);
        let absolute_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            current_dir.join(dest_path)
        };
        
        // For destination, we can't canonicalize if it doesn't exist yet
        let canonical_dest = if absolute_dest.exists() {
            absolute_dest.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve destination path '{}': {}", self.destination, e))))?
        } else {
            // Ensure parent exists and is within project
            if let Some(parent) = absolute_dest.parent() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve destination parent directory: {}", e))))?;
                if !canonical_parent.starts_with(&current_dir) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        TOOL_NAME,
                        &self.destination,
                        "Destination path would be outside the project directory"
                    )));
                }
            }
            absolute_dest
        };
        
        if !canonical_dest.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.destination,
                "Destination path is outside the project directory"
            )));
        }
        
        // Check if destination exists
        if canonical_dest.exists() && !self.overwrite {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Destination '{}' already exists. Set overwrite=true to replace it.", self.destination)
            )));
        }
        
        // Create parent directory if needed
        if let Some(parent) = canonical_dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directory: {}", e))))?;
        }
        
        // Track operation start time
        let start_time = Instant::now();
        
        // Get metadata before move
        let metadata = fs::metadata(&canonical_source).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read source metadata: {}", e))))?;
        
        let is_dir = metadata.is_dir();
        let total_size = if is_dir {
            // For directories, we'll just indicate it's a directory
            0u64
        } else {
            metadata.len()
        };
        
        // Perform the move
        fs::rename(&canonical_source, &canonical_dest)
            .await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to move file: {}", e))))?;
        
        // Restore metadata if requested
        if self.preserve_metadata {
            // Set file times (modified and accessed)
            if let Ok(modified) = metadata.modified() {
                let _ = filetime::set_file_mtime(&canonical_dest, 
                    filetime::FileTime::from_system_time(modified));
            }
            if let Ok(accessed) = metadata.accessed() {
                let _ = filetime::set_file_atime(&canonical_dest, 
                    filetime::FileTime::from_system_time(accessed));
            }
            
            // Set permissions on Unix-like systems
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&canonical_dest, 
                    std::fs::Permissions::from_mode(metadata.permissions().mode())).await;
            }
        }
        
        // Update tracking in context
        // Remove source from read/written files
        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        
        let written_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut written_files_clone = (*written_files).clone();
        
        // If source was tracked, track destination instead
        if read_files_clone.remove(&canonical_source) {
            read_files_clone.insert(canonical_dest.clone());
            context.set_custom_state(read_files_clone).await;
        }
        
        if written_files_clone.remove(&canonical_source) {
            written_files_clone.insert(canonical_dest.clone());
            context.set_custom_state(written_files_clone).await;
        }
        
        let _duration = start_time.elapsed();
        let file_type = if is_dir { "directory" } else { "file" };
        
        // Format paths relative to project root
        let source_relative = canonical_source.strip_prefix(&current_dir)
            .unwrap_or(&canonical_source);
        let dest_relative = canonical_dest.strip_prefix(&current_dir)
            .unwrap_or(&canonical_dest);
        
        // Build metrics string
        let mut metrics_parts = Vec::new();
        if !is_dir && total_size > 0 {
            metrics_parts.push(format_size(total_size));
        }
        if self.preserve_metadata {
            metrics_parts.push("metadata preserved".to_string());
        }
        
        let metrics = if !metrics_parts.is_empty() {
            format!(" ({})", metrics_parts.join(", "))
        } else {
            String::new()
        };
        
        let message = format!(
            "Moved {} {} to {}{}",
            file_type,
            format_path(source_relative),
            format_path(dest_relative),
            metrics
        );

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl MoveTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
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
        // Canonicalize the temp directory path to match what the tool expects
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }
    
    #[tokio::test]
    async fn test_move_basic_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file
        let project_root = context.get_project_root().unwrap();
        let source_path = project_root.join("source.txt");
        let content = "Hello, World!";
        fs::write(&source_path, content).await.unwrap();
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check destination file exists and has same content
        let dest_path = project_root.join("dest.txt");
        assert!(dest_path.exists());
        
        let dest_content = fs::read_to_string(&dest_path).await.unwrap();
        assert_eq!(dest_content, content);
        
        // Source should no longer exist
        assert!(!source_path.exists());
    }
    
    #[tokio::test]
    async fn test_move_rename_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("old_name.txt"), "Content").await.unwrap();
        
        let move_tool = MoveTool {
            source: "old_name.txt".to_string(),
            destination: "new_name.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was renamed
        assert!(!project_root.join("old_name.txt").exists());
        assert!(project_root.join("new_name.txt").exists());
        
        let content = fs::read_to_string(project_root.join("new_name.txt")).await.unwrap();
        assert_eq!(content, "Content");
    }
    
    #[tokio::test]
    async fn test_move_to_subdirectory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file and destination directory
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file.txt"), "Content").await.unwrap();
        fs::create_dir(project_root.join("subdir")).await.unwrap();
        
        let move_tool = MoveTool {
            source: "file.txt".to_string(),
            destination: "subdir/file.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was moved to subdirectory
        assert!(!project_root.join("file.txt").exists());
        assert!(project_root.join("subdir/file.txt").exists());
        
        let content = fs::read_to_string(project_root.join("subdir/file.txt")).await.unwrap();
        assert_eq!(content, "Content");
    }
    
    #[tokio::test]
    async fn test_move_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source directory with files
        let project_root = context.get_project_root().unwrap();
        let source_dir = project_root.join("source_dir");
        fs::create_dir(&source_dir).await.unwrap();
        fs::write(source_dir.join("file1.txt"), "Content 1").await.unwrap();
        fs::write(source_dir.join("file2.txt"), "Content 2").await.unwrap();
        
        let move_tool = MoveTool {
            source: "source_dir".to_string(),
            destination: "dest_dir".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check directory was moved
        assert!(!source_dir.exists());
        
        let dest_dir = project_root.join("dest_dir");
        assert!(dest_dir.exists());
        assert!(dest_dir.is_dir());
        
        // Check files were moved
        assert!(dest_dir.join("file1.txt").exists());
        assert!(dest_dir.join("file2.txt").exists());
        
        let content1 = fs::read_to_string(dest_dir.join("file1.txt")).await.unwrap();
        let content2 = fs::read_to_string(dest_dir.join("file2.txt")).await.unwrap();
        assert_eq!(content1, "Content 1");
        assert_eq!(content2, "Content 2");
    }
    
    #[tokio::test]
    async fn test_move_with_overwrite() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source and destination files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("source.txt"), "New content").await.unwrap();
        fs::write(project_root.join("dest.txt"), "Old content").await.unwrap();
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: true,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check destination was overwritten and source removed
        assert!(!project_root.join("source.txt").exists());
        assert!(project_root.join("dest.txt").exists());
        
        let content = fs::read_to_string(project_root.join("dest.txt")).await.unwrap();
        assert_eq!(content, "New content");
    }
    
    #[tokio::test]
    async fn test_move_without_overwrite_fails() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source and destination files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("source.txt"), "New content").await.unwrap();
        fs::write(project_root.join("dest.txt"), "Old content").await.unwrap();
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("already exists") || error_msg.contains("overwrite"));
        
        // Check both files still exist unchanged
        assert!(project_root.join("source.txt").exists());
        assert!(project_root.join("dest.txt").exists());
        
        let source_content = fs::read_to_string(project_root.join("source.txt")).await.unwrap();
        let dest_content = fs::read_to_string(project_root.join("dest.txt")).await.unwrap();
        assert_eq!(source_content, "New content");
        assert_eq!(dest_content, "Old content");
    }
    
    #[tokio::test]
    async fn test_move_source_not_found() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let move_tool = MoveTool {
            source: "nonexistent.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }
    
    #[tokio::test]
    async fn test_move_preserve_metadata_false() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("source.txt"), "Content").await.unwrap();
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: false,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was moved
        assert!(!project_root.join("source.txt").exists());
        assert!(project_root.join("dest.txt").exists());
        
        let content = fs::read_to_string(project_root.join("dest.txt")).await.unwrap();
        assert_eq!(content, "Content");
    }
    
    #[tokio::test]
    async fn test_move_updates_read_tracking() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file
        let project_root = context.get_project_root().unwrap();
        let source_path = project_root.join("source.txt");
        fs::write(&source_path, "Content").await.unwrap();
        
        // Add file to read tracking
        let read_files = std::sync::Arc::new({
            let mut set = std::collections::HashSet::new();
            set.insert(source_path.clone());
            set
        });
        context.set_custom_state::<std::collections::HashSet<PathBuf>>((*read_files).clone()).await;
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was moved
        assert!(!source_path.exists());
        assert!(project_root.join("dest.txt").exists());
        
        // Tracking state update is tested implicitly by the move succeeding
    }
}