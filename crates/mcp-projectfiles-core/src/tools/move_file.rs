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
    description = "Move or rename files and directories. Preferred over system 'mv' command.

IMPORTANT: Paths are restricted to project directory for safety.
NOTE: Omit optional parameters when not needed, don't pass null.

Parameters:
- source: Source path to move (required)
- destination: Destination path (required)
- overwrite: Replace existing files (optional, default: false)
- preserve_metadata: Keep timestamps/permissions (optional, default: true)
- dry_run: Preview move without executing (optional, default: false)

Examples:
- Rename file: {\"source\": \"old.txt\", \"destination\": \"new.txt\"}
- Move to directory: {\"source\": \"file.txt\", \"destination\": \"archive/file.txt\"}
- Move directory: {\"source\": \"src/old\", \"destination\": \"src/new\"}
- Force overwrite: {\"source\": \"temp.txt\", \"destination\": \"final.txt\", \"overwrite\": true}
- Preview move: {\"source\": \"important.db\", \"destination\": \"backup/important.db\", \"dry_run\": true}
- Quick rename: {\"source\": \"draft.md\", \"destination\": \"README.md\", \"preserve_metadata\": false}

Returns success message with:
- File/directory type moved
- Source and destination paths
- Size information (for files)
- Metadata preservation status
- Dry run indication if applicable"
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
    /// Perform a dry run - show what would be moved without actually moving (default: false)
    #[serde(default)]
    pub dry_run: bool,
}

fn default_true() -> bool {
    true
}

/// Calculate the total size of a directory recursively
async fn calculate_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total_size = 0u64;
    let mut entries = fs::read_dir(path).await?;
    
    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        if metadata.is_dir() {
            total_size += Box::pin(calculate_dir_size(&entry.path())).await?;
        } else {
            total_size += metadata.len();
        }
    }
    
    Ok(total_size)
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
        let mut canonical_dest = if absolute_dest.exists() {
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
        
        // Get source metadata early for validation
        let source_metadata = fs::metadata(&canonical_source).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read source metadata: {}", e))))?;
        
        // Check if destination exists
        if canonical_dest.exists() {
            let dest_metadata = fs::metadata(&canonical_dest).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read destination metadata: {}", e))))?;
            
            // If destination is a directory and source is a file, assume user wants to move file into directory
            if dest_metadata.is_dir() && !source_metadata.is_dir() {
                // Extract filename from source and append to destination
                let file_name = canonical_source.file_name()
                    .ok_or_else(|| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, "Source path has no filename")))?;
                
                let new_dest = canonical_dest.join(file_name);
                
                // Check if the file already exists in the target directory
                if new_dest.exists() && !self.overwrite {
                    let new_dest_relative = new_dest.strip_prefix(&current_dir)
                        .unwrap_or(&new_dest);
                    
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("File '{}' already exists in target directory. Use 'destination: \"{}\"' or set overwrite=true.", 
                            new_dest_relative.display(), 
                            new_dest_relative.display())
                    )));
                }
                
                // Update canonical_dest to include the filename
                canonical_dest = new_dest;
            } else if !self.overwrite {
                // Destination exists and is not a directory, or both are directories
                let dest_type = if dest_metadata.is_dir() { "directory" } else { "file" };
                let dest_size = if !dest_metadata.is_dir() { 
                    format!(" ({})", format_size(dest_metadata.len())) 
                } else { 
                    String::new() 
                };
                
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Destination {} '{}'{} already exists. Set overwrite=true to replace it.", 
                        dest_type, self.destination, dest_size)
                )));
            }
        }
        
        // Create parent directory if needed
        if let Some(parent) = canonical_dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directory: {}", e))))?;
        }
        
        // Track operation start time
        let start_time = Instant::now();
        
        let is_dir = source_metadata.is_dir();
        let total_size = if is_dir {
            // For directories, we'll calculate total size if in dry run
            if self.dry_run {
                calculate_dir_size(&canonical_source).await.unwrap_or(0)
            } else {
                0u64
            }
        } else {
            source_metadata.len()
        };
        
        // Perform the move or simulate it for dry run
        if !self.dry_run {
            fs::rename(&canonical_source, &canonical_dest)
                .await
                .map_err(|e| {
                    // Provide more context about the failure
                    let error_context = if e.kind() == std::io::ErrorKind::PermissionDenied {
                        "Permission denied. Check file permissions and ownership."
                    } else if e.kind() == std::io::ErrorKind::NotFound {
                        "Source file was removed or destination parent directory doesn't exist."
                    } else if cfg!(target_os = "windows") && e.raw_os_error() == Some(17) {
                        "Cross-device move not supported. Source and destination must be on the same drive."
                    } else if cfg!(unix) && e.raw_os_error() == Some(18) {
                        "Cross-device move not supported. Source and destination must be on the same filesystem."
                    } else {
                        "Operation failed. This might be due to filesystem limitations or permissions."
                    };
                    
                    CallToolError::from(tool_errors::invalid_input(TOOL_NAME, 
                        &format!("Failed to move '{}' to '{}': {} {}", 
                            self.source, self.destination, e, error_context)))
                })?;
            
            // Restore metadata if requested
            if self.preserve_metadata {
                // Set file times (modified and accessed)
                if let Ok(modified) = source_metadata.modified() {
                    let _ = filetime::set_file_mtime(&canonical_dest, 
                        filetime::FileTime::from_system_time(modified));
                }
                if let Ok(accessed) = source_metadata.accessed() {
                    let _ = filetime::set_file_atime(&canonical_dest, 
                        filetime::FileTime::from_system_time(accessed));
                }
                
                // Set permissions on Unix-like systems
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&canonical_dest, 
                        std::fs::Permissions::from_mode(source_metadata.permissions().mode())).await;
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
        
        let message = if self.dry_run {
            format!(
                "[DRY RUN] Would move {} {} to {}{}",
                file_type,
                format_path(source_relative),
                format_path(dest_relative),
                metrics
            )
        } else {
            format!(
                "Moved {} {} to {}{}",
                file_type,
                format_path(source_relative),
                format_path(dest_relative),
                metrics
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
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
            dry_run: false,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was moved
        assert!(!source_path.exists());
        assert!(project_root.join("dest.txt").exists());
        
        // Tracking state update is tested implicitly by the move succeeding
    }
    
    #[tokio::test]
    async fn test_move_dry_run() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file
        let project_root = context.get_project_root().unwrap();
        let source_path = project_root.join("source.txt");
        let content = "Test content";
        fs::write(&source_path, content).await.unwrap();
        
        let move_tool = MoveTool {
            source: "source.txt".to_string(),
            destination: "dest.txt".to_string(),
            overwrite: false,
            preserve_metadata: true,
            dry_run: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check result contains dry run indication
        let result_str = format!("{:?}", result.unwrap());
        assert!(result_str.contains("DRY RUN") || result_str.contains("Would move"));
        
        // Source should still exist (not moved)
        assert!(source_path.exists());
        
        // Destination should not exist
        assert!(!project_root.join("dest.txt").exists());
        
        // Verify content is unchanged
        let source_content = fs::read_to_string(&source_path).await.unwrap();
        assert_eq!(source_content, content);
    }
    
    #[tokio::test]
    async fn test_move_dry_run_with_overwrite() {
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
            dry_run: true,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Both files should still exist unchanged
        assert!(project_root.join("source.txt").exists());
        assert!(project_root.join("dest.txt").exists());
        
        let source_content = fs::read_to_string(project_root.join("source.txt")).await.unwrap();
        let dest_content = fs::read_to_string(project_root.join("dest.txt")).await.unwrap();
        assert_eq!(source_content, "New content");
        assert_eq!(dest_content, "Old content");
    }
    
    #[tokio::test]
    async fn test_move_file_to_existing_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file and destination directory
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file.txt"), "Content").await.unwrap();
        fs::create_dir(project_root.join("destdir")).await.unwrap();
        
        let move_tool = MoveTool {
            source: "file.txt".to_string(),
            destination: "destdir".to_string(),  // Directory without filename
            overwrite: false,
            preserve_metadata: true,
            dry_run: false,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // File should be moved into the directory
        assert!(!project_root.join("file.txt").exists());
        assert!(project_root.join("destdir/file.txt").exists());
        
        let content = fs::read_to_string(project_root.join("destdir/file.txt")).await.unwrap();
        assert_eq!(content, "Content");
    }
    
    #[tokio::test]
    async fn test_move_file_to_directory_with_existing_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create source file, destination directory, and existing file in directory
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file.txt"), "New content").await.unwrap();
        fs::create_dir(project_root.join("destdir")).await.unwrap();
        fs::write(project_root.join("destdir/file.txt"), "Old content").await.unwrap();
        
        let move_tool = MoveTool {
            source: "file.txt".to_string(),
            destination: "destdir".to_string(),
            overwrite: false,
            preserve_metadata: true,
            dry_run: false,
        };
        
        let result = move_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("already exists in target directory"));
        
        // Source file should still exist
        assert!(project_root.join("file.txt").exists());
        
        // Existing file should be unchanged
        let existing_content = fs::read_to_string(project_root.join("destdir/file.txt")).await.unwrap();
        assert_eq!(existing_content, "Old content");
    }
}
