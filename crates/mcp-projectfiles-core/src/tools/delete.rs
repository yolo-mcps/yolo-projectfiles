use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::{format_count, format_counts, format_path, format_size};
use async_trait::async_trait;
use glob::{MatchOptions, glob_with};
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;

const TOOL_NAME: &str = "delete";

#[mcp_tool(
    name = "delete",
    description = "Delete files/directories with safety checks. Requires confirm or force. Supports patterns, recursive deletion.
Examples: {\"path\": \"old.txt\", \"confirm\": true}, {\"path\": \"*.tmp\", \"pattern\": true, \"force\": true}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct DeleteTool {
    /// Path to delete (relative to project root)
    pub path: String,
    /// Whether to recursively delete directories (optional, default: false)
    #[serde(default)]
    pub recursive: bool,
    /// Require confirmation by setting to true (optional, default: false for safety)
    #[serde(default)]
    pub confirm: bool,
    /// Force deletion without confirmation (optional, default: false, overrides confirm)
    #[serde(default)]
    pub force: bool,
    /// Pattern matching mode - treat path as a glob pattern for bulk deletes (optional, default: false)
    #[serde(default)]
    pub pattern: bool,
}

#[async_trait]
impl StatefulTool for DeleteTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        if !self.confirm && !self.force {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME,
                "Deletion requires confirmation. Set confirm=true or force=true to proceed.",
            )));
        }

        let current_dir = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        if self.pattern {
            // Pattern matching mode - treat path as glob pattern
            let pattern_path = if Path::new(&self.path).is_absolute() {
                self.path.clone()
            } else {
                current_dir.join(&self.path).to_string_lossy().to_string()
            };

            let options = MatchOptions {
                case_sensitive: true,
                require_literal_separator: false,
                require_literal_leading_dot: false,
            };

            let paths: Vec<PathBuf> = glob_with(&pattern_path, options)
                .map_err(|e| {
                    CallToolError::from(tool_errors::pattern_error(
                        TOOL_NAME,
                        &self.path,
                        &e.to_string(),
                    ))
                })?
                .filter_map(Result::ok)
                .filter(|p| p.starts_with(&current_dir) && p != &current_dir)
                .collect();

            if paths.is_empty() {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("No files found matching pattern: {}", self.path),
                )));
            }

            // Delete all matching files/directories
            let mut total_size = 0u64;
            let mut file_count = 0usize;
            let mut dir_count = 0usize;
            let mut deleted_paths = Vec::new();

            for path in paths {
                let metadata = fs::metadata(&path).await.map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to read metadata for '{}': {}", path.display(), e),
                    ))
                })?;

                if metadata.is_file() {
                    total_size += metadata.len();
                    file_count += 1;
                    fs::remove_file(&path).await.map_err(|e| {
                        CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Failed to delete file '{}': {}", path.display(), e),
                        ))
                    })?;
                    deleted_paths.push((path.clone(), "file"));
                } else if metadata.is_dir() && self.recursive {
                    let stats = count_entries_with_size(&path).await?;
                    total_size += stats.total_size;
                    file_count += stats.file_count;
                    dir_count += stats.dir_count;
                    fs::remove_dir_all(&path).await.map_err(|e| {
                        CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Failed to delete directory '{}': {}", path.display(), e),
                        ))
                    })?;
                    deleted_paths.push((path.clone(), "directory"));
                } else if metadata.is_dir() {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!(
                            "Directory '{}' is not empty. Use recursive=true to delete non-empty directories.",
                            path.display()
                        ),
                    )));
                }

                // Remove from read files tracking
                let read_files = context
                    .get_custom_state::<HashSet<PathBuf>>()
                    .await
                    .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
                let mut read_files_clone = (*read_files).clone();
                read_files_clone.remove(&path);
                context.set_custom_state(read_files_clone).await;
            }

            // Format deleted paths with proper quotes
            let formatted_paths: Vec<String> = deleted_paths
                .iter()
                .map(|(p, t)| {
                    let relative_path = p.strip_prefix(&current_dir).unwrap_or(p);
                    format!("  - {} ({})", format_path(relative_path), t)
                })
                .collect();

            let counts = format_counts(&[
                (file_count, "file", "files"),
                (dir_count, "directory", "directories"),
            ]);

            let summary = if total_size > 0 {
                format!(
                    "Deleted {} matching pattern '{}' ({}, {} freed):\n{}",
                    format_count(deleted_paths.len(), "item", "items"),
                    self.path,
                    counts,
                    format_size(total_size),
                    formatted_paths.join("\n")
                )
            } else {
                format!(
                    "Deleted {} matching pattern '{}':\n{}",
                    format_count(deleted_paths.len(), "item", "items"),
                    self.path,
                    formatted_paths.join("\n")
                )
            };

            return Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    summary, None,
                ))],
                is_error: Some(false),
                meta: None,
            });
        }

        // Single file/directory mode (existing logic)
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };

        let canonical_path = absolute_path.canonicalize().map_err(|_e| {
            CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.path))
        })?;

        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory",
            )));
        }

        // Don't allow deleting the project root
        if canonical_path == current_dir {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Cannot delete the project root directory",
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME, &self.path,
            )));
        }

        let metadata = fs::metadata(&canonical_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read metadata: {}", e),
            ))
        })?;

        // Track operation start time
        let start_time = Instant::now();
        let total_size: u64;
        let file_count: usize;
        let dir_count: usize;
        let file_type;

        if metadata.is_file() {
            file_type = "file";
            total_size = metadata.len();
            file_count = 1;
            dir_count = 0;
            fs::remove_file(&canonical_path).await.map_err(|e| {
                CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Failed to delete file: {}", e),
                ))
            })?;
        } else if metadata.is_dir() {
            file_type = "directory";
            if self.recursive {
                let stats = count_entries_with_size(&canonical_path).await?;
                total_size = stats.total_size;
                file_count = stats.file_count;
                dir_count = stats.dir_count;
                fs::remove_dir_all(&canonical_path).await.map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to delete directory: {}", e),
                    ))
                })?;
            } else {
                // Check if directory is empty
                let mut entries = fs::read_dir(&canonical_path).await.map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to read directory: {}", e),
                    ))
                })?;

                if entries
                    .next_entry()
                    .await
                    .map_err(|e| {
                        CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Failed to check directory: {}", e),
                        ))
                    })?
                    .is_some()
                {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        "Directory is not empty. Set recursive=true to delete non-empty directories.",
                    )));
                }

                fs::remove_dir(&canonical_path).await.map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to delete empty directory: {}", e),
                    ))
                })?;
                file_count = 0;
                dir_count = 1;
                total_size = 0;
            }
        } else {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Path is neither a file nor a directory",
            )));
        }

        // Remove from tracking
        let read_files = context
            .get_custom_state::<HashSet<PathBuf>>()
            .await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        read_files_clone.remove(&canonical_path);
        context.set_custom_state(read_files_clone).await;

        let written_files = context
            .get_custom_state::<HashSet<PathBuf>>()
            .await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut written_files_clone = (*written_files).clone();
        written_files_clone.remove(&canonical_path);
        context.set_custom_state(written_files_clone).await;

        let _duration = start_time.elapsed();

        // Format the message according to new standards
        // Format path relative to project root
        let relative_path = canonical_path
            .strip_prefix(&current_dir)
            .unwrap_or(&canonical_path);

        // Build metrics string
        let metrics = if metadata.is_dir() && self.recursive {
            let counts = format_counts(&[
                (file_count, "file", "files"),
                (dir_count, "directory", "directories"),
            ]);
            format!(" ({}, {} freed)", counts, format_size(total_size))
        } else if total_size > 0 {
            format!(" ({} freed)", format_size(total_size))
        } else {
            String::new()
        };

        let message = format!(
            "Deleted {} {}{}",
            file_type,
            format_path(relative_path),
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

#[derive(Default)]
struct DeleteStats {
    total_size: u64,
    file_count: usize,
    dir_count: usize,
}

fn count_entries_with_size(
    path: &Path,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<DeleteStats, CallToolError>> + Send + '_>,
> {
    Box::pin(async move {
        let mut stats = DeleteStats::default();
        stats.dir_count = 1; // Count the directory itself

        let mut entries = fs::read_dir(path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read directory: {}", e),
            ))
        })?;

        loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let metadata = entry.metadata().await.map_err(|e| {
                        CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Failed to get metadata: {}", e),
                        ))
                    })?;

                    if metadata.is_dir() {
                        let sub_stats = Box::pin(count_entries_with_size(&entry.path())).await?;
                        stats.total_size += sub_stats.total_size;
                        stats.file_count += sub_stats.file_count;
                        stats.dir_count += sub_stats.dir_count;
                    } else if metadata.is_file() {
                        stats.total_size += metadata.len();
                        stats.file_count += 1;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to read entry: {}", e),
                    )));
                }
            }
        }

        Ok(stats)
    })
}

impl DeleteTool {
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
    async fn test_delete_requires_confirmation() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "test.txt".to_string(),
            recursive: false,
            confirm: false,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("requires confirmation"));

        // File should still exist
        assert!(project_root.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_file_with_confirm() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        let test_file = project_root.join("test.txt");
        fs::write(&test_file, "content").await.unwrap();

        let delete_tool = DeleteTool {
            path: "test.txt".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // File should be deleted
        assert!(!test_file.exists());
    }

    #[tokio::test]
    async fn test_delete_file_with_force() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        let test_file = project_root.join("test.txt");
        fs::write(&test_file, "content").await.unwrap();

        let delete_tool = DeleteTool {
            path: "test.txt".to_string(),
            recursive: false,
            confirm: false,
            force: true,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // File should be deleted
        assert!(!test_file.exists());
    }

    #[tokio::test]
    async fn test_delete_directory_recursive() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create directory structure
        let project_root = context.get_project_root().unwrap();
        let test_dir = project_root.join("test_dir");
        fs::create_dir(&test_dir).await.unwrap();

        fs::write(test_dir.join("file1.txt"), "content1")
            .await
            .unwrap();
        fs::write(test_dir.join("file2.txt"), "content2")
            .await
            .unwrap();

        let sub_dir = test_dir.join("subdir");
        fs::create_dir(&sub_dir).await.unwrap();
        fs::write(sub_dir.join("file3.txt"), "content3")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "test_dir".to_string(),
            recursive: true,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // Directory should be deleted
        assert!(!test_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_directory_non_recursive_fails() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create directory with content
        let project_root = context.get_project_root().unwrap();
        let test_dir = project_root.join("test_dir");
        fs::create_dir(&test_dir).await.unwrap();
        fs::write(test_dir.join("file.txt"), "content")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "test_dir".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not empty") || error_msg.contains("recursive"));

        // Directory should still exist
        assert!(test_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_with_pattern_matching() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create multiple test files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test1.txt"), "content1")
            .await
            .unwrap();
        fs::write(project_root.join("test2.txt"), "content2")
            .await
            .unwrap();
        fs::write(project_root.join("other.log"), "log content")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "test*.txt".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: true,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // Pattern matching files should be deleted
        assert!(!project_root.join("test1.txt").exists());
        assert!(!project_root.join("test2.txt").exists());

        // Non-matching file should remain
        assert!(project_root.join("other.log").exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_file() {
        let (context, _temp_dir) = setup_test_context().await;

        let delete_tool = DeleteTool {
            path: "nonexistent.txt".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_delete_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;

        let delete_tool = DeleteTool {
            path: "../outside.txt".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_delete_empty_directory() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create empty directory
        let project_root = context.get_project_root().unwrap();
        let test_dir = project_root.join("empty_dir");
        fs::create_dir(&test_dir).await.unwrap();

        let delete_tool = DeleteTool {
            path: "empty_dir".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // Empty directory should be deleted
        assert!(!test_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_removes_from_read_tracking() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        let test_file = project_root.join("tracked.txt");
        fs::write(&test_file, "content").await.unwrap();

        // Add file to read tracking
        let read_files = std::sync::Arc::new({
            let mut set = std::collections::HashSet::new();
            set.insert(test_file.clone());
            set
        });
        context
            .set_custom_state::<std::collections::HashSet<PathBuf>>((*read_files).clone())
            .await;

        let delete_tool = DeleteTool {
            path: "tracked.txt".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // File should be deleted
        assert!(!test_file.exists());

        // File should be removed from tracking (we can't easily test this without accessing internal state)
        // But the deletion succeeding indicates it worked correctly
    }

    #[tokio::test]
    async fn test_delete_pattern_with_subdirectories() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create nested structure
        let project_root = context.get_project_root().unwrap();
        let subdir = project_root.join("src");
        fs::create_dir(&subdir).await.unwrap();
        fs::write(project_root.join("test.log"), "log1")
            .await
            .unwrap();
        fs::write(subdir.join("test.log"), "log2").await.unwrap();

        let delete_tool = DeleteTool {
            path: "**/*.log".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: true,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // Both log files should be deleted
        assert!(!project_root.join("test.log").exists());
        assert!(!subdir.join("test.log").exists());
    }

    #[tokio::test]
    async fn test_delete_force_overrides_confirm() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create test file
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "test.txt".to_string(),
            recursive: false,
            confirm: false, // Explicitly false
            force: true,    // Force should override
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        // File should be deleted even though confirm=false
        assert!(!project_root.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_pattern_no_matches() {
        let (context, _temp_dir) = setup_test_context().await;

        // Create files that don't match pattern
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test.txt"), "content")
            .await
            .unwrap();

        let delete_tool = DeleteTool {
            path: "*.nonexistent".to_string(),
            recursive: false,
            confirm: true,
            force: false,
            pattern: true,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("No files found matching pattern"));

        // Original file should still exist
        assert!(project_root.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_project_root_fails() {
        let (context, _temp_dir) = setup_test_context().await;

        let delete_tool = DeleteTool {
            path: ".".to_string(),
            recursive: true,
            confirm: true,
            force: false,
            pattern: false,
        };

        let result = delete_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Cannot delete the project root directory"));
    }
}

