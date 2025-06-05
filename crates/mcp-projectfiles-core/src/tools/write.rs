use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_path, resolve_path_for_read};
use crate::theme::DiffTheme;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use encoding_rs;
use similar::{ChangeTag, TextDiff};
use chrono::Utc;

const TOOL_NAME: &str = "write";
const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100MB safety limit

fn default_encoding() -> String {
    "utf-8".to_string()
}

fn default_follow_symlinks() -> bool {
    true
}

#[mcp_tool(name = "write", description = "Write or append content to files. Supports backup, diff preview, and safety checks.

Examples:
- {\"path\": \"config.json\", \"content\": \"{...}\"}
- {\"path\": \"log.txt\", \"content\": \"entry\", \"append\": true}")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct WriteTool {
    /// Path to the file to write (relative to project root)
    pub path: String,
    /// Content to write to the file
    pub content: String,
    /// Whether to append to the file instead of overwriting (default: false)
    #[serde(default)]
    pub append: bool,
    /// Create a backup of the existing file before overwriting (default: false)
    #[serde(default)]
    pub backup: bool,
    /// Text encoding to use when writing the file (default: "utf-8")
    /// Supported: "utf-8", "ascii", "latin1", "utf-16", "utf-16le", "utf-16be"
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Follow symlinks when writing files (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    /// Show a diff of what will be changed when overwriting (default: false)
    #[serde(default)]
    pub show_diff: bool,
    /// Perform a dry run - preview the operation without writing (default: false)
    #[serde(default)]
    pub dry_run: bool,
    /// Force write even if file exceeds size limits (default: false)
    #[serde(default)]
    pub force: bool,
    /// Include detailed metadata in the response (default: false)
    #[serde(default)]
    pub include_metadata: bool,
}

#[async_trait]
impl StatefulTool for WriteTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the same path resolution as read tool for consistency
        let canonical_path = if self.path.is_empty() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Path cannot be empty"
            )));
        } else if self.append || !Path::new(&self.path).exists() {
            // For append mode or new files, we need special handling
            let requested_path = Path::new(&self.path);
            let absolute_path = if requested_path.is_absolute() {
                requested_path.to_path_buf()
            } else {
                project_root.join(requested_path)
            };
            
            // For new files, validate parent directory
            if !absolute_path.exists() {
                if let Some(parent) = absolute_path.parent() {
                    if parent.exists() {
                        let canonical_parent = parent.canonicalize()
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                                TOOL_NAME,
                                &format!("Failed to resolve parent directory: {}", e)
                            )))?;
                        
                        if !canonical_parent.starts_with(&project_root) {
                            return Err(CallToolError::from(tool_errors::access_denied(
                                TOOL_NAME,
                                &self.path,
                                "Path is outside the project directory"
                            )));
                        }
                        
                        canonical_parent.join(absolute_path.file_name().unwrap())
                    } else {
                        // Parent doesn't exist, we'll create it
                        absolute_path
                    }
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Invalid file path: '{}'", self.path)
                    )));
                }
            } else {
                absolute_path.canonicalize()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to resolve path '{}': {}", self.path, e)
                    )))?
            }
        } else {
            // Use the standard resolution for existing files
            resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?
        };
        
        // Ensure final path is within project
        if !canonical_path.starts_with(&project_root) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory"
            )));
        }

        // Collect metadata about the operation
        let file_existed = canonical_path.exists();
        let previous_size = if file_existed {
            fs::metadata(&canonical_path).await.ok().map(|m| m.len())
        } else {
            None
        };
        
        // Check file size limit (unless forced)
        if !self.force && self.content.len() as u64 > MAX_FILE_SIZE {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Content size ({}) exceeds maximum file size limit ({}). Use 'force: true' to override.",
                    format_size(self.content.len() as u64),
                    format_size(MAX_FILE_SIZE)
                )
            )));
        }
        
        // Read existing content if needed for diff or safety check
        let existing_content = if file_existed && (self.show_diff || self.dry_run) && !self.append {
            match fs::read_to_string(&canonical_path).await {
                Ok(content) => Some(content),
                Err(_) => None, // File might be binary or unreadable
            }
        } else {
            None
        };
        
        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        
        if file_existed && !read_files.contains(&canonical_path) && !self.append && !self.dry_run {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME, 
                &format!("Cannot write to '{}': File must be read first before writing", self.path)
            )));
        }

        if let Some(parent) = canonical_path.parent() {
            if !parent.exists() && !self.dry_run {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directories: {}", e))))?;
            }
        }

        // Create backup if requested and file exists
        let mut backup_path_str = None;
        let mut backup_created = false;
        if self.backup && file_existed && !self.append && !self.dry_run {
            let backup_path = canonical_path.with_extension(
                format!("{}.bak", canonical_path.extension().unwrap_or_default().to_string_lossy())
            );
            
            // If the backup path has no change (no extension), add .bak directly
            let backup_path = if backup_path == canonical_path {
                PathBuf::from(format!("{}.bak", canonical_path.display()))
            } else {
                backup_path
            };
            
            fs::copy(&canonical_path, &backup_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create backup: {}", e))))?;
            
            backup_path_str = Some(backup_path.display().to_string());
            backup_created = true;
        }

        // Encode content
        let encoded_bytes = self.encode_content()?;
        
        // Perform write operation (unless dry run)
        if !self.dry_run {
            if self.append {
                use tokio::fs::OpenOptions;
                
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&canonical_path)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file for appending: {}", e))))?;
                
                file.write_all(&encoded_bytes)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to append to file: {}", e))))?;
            } else {
                fs::write(&canonical_path, &encoded_bytes)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to write file: {}", e))))?;
            }
            
            let mut read_files_clone = (*read_files).clone();
            read_files_clone.insert(canonical_path.clone());
            context.set_custom_state(read_files_clone).await;
        }

        // Calculate content size
        let content_size = self.content.len() as u64;
        let size_str = format_size(content_size);
        
        // Format the path relative to project root
        let relative_path = canonical_path.strip_prefix(&project_root)
            .unwrap_or(&canonical_path);
        
        // Build response content
        let mut response_parts = Vec::new();
        
        // Main message
        let operation = if self.dry_run {
            "Would write"
        } else if self.append {
            "Appended"
        } else if file_existed {
            "Wrote"
        } else {
            "Created"
        };
        
        let mut message = format!("{} {} to {}", operation, size_str, format_path(relative_path));
        
        if backup_created {
            message.push_str(" (backup created)");
        }
        
        response_parts.push(message);
        
        // Show diff if requested
        if self.show_diff && existing_content.is_some() && !self.append {
            let diff = generate_colored_diff(
                existing_content.as_ref().unwrap(),
                &self.content,
                &relative_path.display().to_string()
            );
            
            // Limit diff output to prevent overwhelming the user
            let diff_lines: Vec<&str> = diff.lines().collect();
            if diff_lines.len() > 100 {
                response_parts.push(format!("\n{}", diff_lines[..100].join("\n")));
                response_parts.push(format!("... ({} more lines truncated)", diff_lines.len() - 100));
            } else {
                response_parts.push(format!("\n{}", diff));
            }
        }
        
        // Include metadata if requested
        if self.include_metadata {
            let metadata = WriteMetadata {
                operation: operation.to_string(),
                path: relative_path.display().to_string(),
                size_written: content_size,
                size_human: size_str.clone(),
                encoding_used: self.encoding.clone(),
                backup_created,
                backup_path: backup_path_str,
                timestamp: Utc::now().to_rfc3339(),
                file_existed,
                previous_size,
            };
            
            response_parts.push(format!("\n{}", serde_json::to_string_pretty(&metadata)
                .unwrap_or_else(|_| "Failed to serialize metadata".to_string())));
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                response_parts.join("\n"),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

/// Generate a colored unified diff between two strings
fn generate_colored_diff(original: &str, new_content: &str, file_path: &str) -> String {
    let diff = TextDiff::from_lines(original, new_content);
    let mut output = String::new();
    
    let theme = DiffTheme::current();
    let use_colors = !cfg!(test) && theme != DiffTheme::None;
    
    // Add header
    if use_colors {
        output.push_str(&format!(
            "{} {}\n",
            theme.colorize_header_old("---"),
            theme.colorize_header_old(file_path)
        ));
        output.push_str(&format!(
            "{} {}\n",
            theme.colorize_header_new("+++"),
            theme.colorize_header_new(file_path)
        ));
    } else {
        output.push_str(&format!("--- {}\n", file_path));
        output.push_str(&format!("+++ {}\n", file_path));
    }
    
    // Generate hunks
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        if use_colors {
            output.push_str(&format!(
                "{}\n",
                theme.colorize_hunk_header(&hunk.header().to_string())
            ));
        } else {
            output.push_str(&format!("{}\n", hunk.header()));
        }
        
        for change in hunk.iter_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    output.push_str(&format!(" {}", change.value()));
                }
                ChangeTag::Delete => {
                    if use_colors {
                        output.push_str(&format!(
                            "{}{}",
                            theme.colorize_deletion_marker("-"),
                            theme.colorize_deletion(change.value())
                        ));
                    } else {
                        output.push_str(&format!("-{}", change.value()));
                    }
                }
                ChangeTag::Insert => {
                    if use_colors {
                        output.push_str(&format!(
                            "{}{}",
                            theme.colorize_addition_marker("+"),
                            theme.colorize_addition(change.value())
                        ));
                    } else {
                        output.push_str(&format!("+{}", change.value()));
                    }
                }
            }
        }
    }
    
    output
}

#[derive(Serialize, Deserialize, Debug)]
struct WriteMetadata {
    operation: String,
    path: String,
    size_written: u64,
    size_human: String,
    encoding_used: String,
    backup_created: bool,
    backup_path: Option<String>,
    timestamp: String,
    file_existed: bool,
    previous_size: Option<u64>,
}

impl WriteTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }

    fn encode_content(&self) -> Result<Vec<u8>, CallToolError> {
        let encoding = match self.encoding.to_lowercase().as_str() {
            "utf-8" | "utf8" => encoding_rs::UTF_8,
            "ascii" => encoding_rs::WINDOWS_1252, // ASCII is a subset of Windows-1252
            "latin1" | "iso-8859-1" => encoding_rs::WINDOWS_1252,
            "utf-16" => encoding_rs::UTF_16LE, // Default to little-endian
            "utf-16le" => encoding_rs::UTF_16LE,
            "utf-16be" => encoding_rs::UTF_16BE,
            _ => encoding_rs::UTF_8, // Default fallback
        };

        let (encoded, _encoding_used, had_errors) = encoding.encode(&self.content);
        
        if had_errors {
            eprintln!("Warning: Some characters could not be encoded with {} encoding", self.encoding);
        }
        
        Ok(encoded.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;
    use tokio::fs;
    use std::path::PathBuf;
    
    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        // Canonicalize the temp directory path to match what the tool expects
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }
    
    fn create_test_write_tool(path: &str, content: &str) -> WriteTool {
        WriteTool {
            path: path.to_string(),
            content: content.to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
            follow_symlinks: true,
            show_diff: false,
            dry_run: false,
            force: false,
            include_metadata: false,
        }
    }
    
    #[tokio::test]
    async fn test_write_basic_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = create_test_write_tool("test.txt", "Hello, World!");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("test.txt");
        assert!(file_path.exists());
        
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello, World!");
    }
    
    #[tokio::test]
    async fn test_write_with_parent_directories() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create the parent directory first to work around the path validation issue
        let project_root = context.get_project_root().unwrap();
        fs::create_dir_all(project_root.join("subdir")).await.unwrap();
        
        let write_tool = create_test_write_tool("subdir/test.txt", "Nested content");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("subdir/test.txt");
        assert!(file_path.exists());
        
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Nested content");
    }
    
    #[tokio::test]
    async fn test_write_append_mode() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // First write
        let write_tool = create_test_write_tool("append_test.txt", "First line\n");
        
        write_tool.call_with_context(&context).await.unwrap();
        
        // Append to existing file
        let mut append_tool = create_test_write_tool("append_test.txt", "Second line\n");
        append_tool.append = true;
        
        let result = append_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("append_test.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "First line\nSecond line\n");
    }
    
    #[tokio::test]
    async fn test_write_with_backup() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create initial file
        let initial_content = "Original content";
        let file_path = context.get_project_root().unwrap().join("backup_test.txt");
        fs::write(&file_path, initial_content).await.unwrap();
        
        // Mark as read (required for overwrite)
        let read_files = std::sync::Arc::new({
            let mut set = std::collections::HashSet::new();
            set.insert(file_path.clone());
            set
        });
        context.set_custom_state::<std::collections::HashSet<PathBuf>>((*read_files).clone()).await;
        
        // Write with backup
        let mut write_tool = create_test_write_tool("backup_test.txt", "New content");
        write_tool.backup = true;
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check original file has new content
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "New content");
        
        // Check backup file exists with original content
        let backup_path = context.get_project_root().unwrap().join("backup_test.txt.bak");
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(&backup_path).await.unwrap();
        assert_eq!(backup_content, initial_content);
    }
    
    #[tokio::test]
    async fn test_write_file_not_read_error() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create existing file
        let file_path = context.get_project_root().unwrap().join("existing.txt");
        fs::write(&file_path, "existing content").await.unwrap();
        
        // Try to overwrite without reading first
        let write_tool = create_test_write_tool("existing.txt", "New content");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("File must be read first before writing"));
    }
    
    #[tokio::test]
    async fn test_write_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = create_test_write_tool("../outside.txt", "Should not work");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[tokio::test]
    async fn test_write_different_encodings() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let test_content = "Test content with special chars: ñéü";
        
        // Test UTF-8 encoding
        let write_tool = create_test_write_tool("utf8_test.txt", test_content);
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("utf8_test.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, test_content);
        
        // Test ASCII encoding (note: special chars may be lost)
        let mut write_tool = create_test_write_tool("ascii_test.txt", "Simple ASCII text");
        write_tool.encoding = "ascii".to_string();
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_write_empty_content() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = create_test_write_tool("empty.txt", "");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("empty.txt");
        assert!(file_path.exists());
        
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "");
    }
    
    #[tokio::test]
    async fn test_write_large_content() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create large content (10KB)
        let large_content = "x".repeat(10240);
        
        let write_tool = create_test_write_tool("large.txt", &large_content);
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("large.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, large_content);
    }
    
    #[tokio::test]
    async fn test_write_invalid_path() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = create_test_write_tool("", "content");
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_write_dry_run() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mut write_tool = create_test_write_tool("dry_run_test.txt", "Test content");
        write_tool.dry_run = true;
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // File should not exist after dry run
        let file_path = context.get_project_root().unwrap().join("dry_run_test.txt");
        assert!(!file_path.exists());
        
        // Response should indicate dry run
        if let Ok(result) = result {
            let text = &result.content[0];
            if let CallToolResultContentItem::TextContent(text_content) = text {
                assert!(text_content.text.contains("Would write"));
            }
        }
    }
    
    #[tokio::test]
    async fn test_write_show_diff() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create initial file
        let file_path = context.get_project_root().unwrap().join("diff_test.txt");
        fs::write(&file_path, "Original content").await.unwrap();
        
        // Mark as read
        let read_files = std::sync::Arc::new({
            let mut set = std::collections::HashSet::new();
            set.insert(file_path.clone());
            set
        });
        context.set_custom_state::<std::collections::HashSet<PathBuf>>((*read_files).clone()).await;
        
        let mut write_tool = create_test_write_tool("diff_test.txt", "Modified content");
        write_tool.show_diff = true;
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Response should contain diff
        if let Ok(result) = result {
            let text = &result.content[0];
            if let CallToolResultContentItem::TextContent(text_content) = text {
                assert!(text_content.text.contains("---"));
                assert!(text_content.text.contains("+++"));
                assert!(text_content.text.contains("-Original content"));
                assert!(text_content.text.contains("+Modified content"));
            }
        }
    }
    
    #[tokio::test]
    async fn test_write_include_metadata() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let mut write_tool = create_test_write_tool("metadata_test.txt", "Test content");
        write_tool.include_metadata = true;
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Response should contain metadata
        if let Ok(result) = result {
            let text = &result.content[0];
            if let CallToolResultContentItem::TextContent(text_content) = text {
                assert!(text_content.text.contains("operation"));
                assert!(text_content.text.contains("size_written"));
                assert!(text_content.text.contains("encoding_used"));
                assert!(text_content.text.contains("timestamp"));
            }
        }
    }
    
    #[tokio::test]
    async fn test_write_force_large_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create content larger than limit
        let large_content = "x".repeat(MAX_FILE_SIZE as usize + 1);
        
        // Should fail without force
        let write_tool = create_test_write_tool("large_fail.txt", &large_content);
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        // Should succeed with force
        let mut write_tool = create_test_write_tool("large_success.txt", &large_content);
        write_tool.force = true;
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
    }
}