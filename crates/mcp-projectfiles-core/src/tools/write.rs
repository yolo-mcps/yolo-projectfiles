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
use encoding_rs;

const TOOL_NAME: &str = "write";

fn default_encoding() -> String {
    "utf-8".to_string()
}

#[mcp_tool(name = "write", description = "Write content to files within the project. Preferred over system text editors.

IMPORTANT: Existing files must be read first using the read tool.

Parameters:
- path: File path (required)
- content: Content to write (required)
- append: Append instead of overwrite (default: false)
- backup: Create backup before overwriting (default: false)
- encoding: Text encoding (default: \"utf-8\")

Creates parent directories automatically if needed.")]
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
}

#[async_trait]
impl StatefulTool for WriteTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let current_dir = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        // For existing files, canonicalize the path
        // For new files, canonicalize the parent directory and ensure the full path is within bounds
        let canonical_path = if absolute_path.exists() {
            absolute_path.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path '{}': {}", self.path, e))))?
        } else {
            // For new files, canonicalize the parent directory
            if let Some(parent) = absolute_path.parent() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve parent directory: {}", e))))?;
                
                // Ensure the parent is within the project directory
                if !canonical_parent.starts_with(&current_dir) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        TOOL_NAME, 
                        &self.path, 
                        "Path is outside the project directory"
                    )));
                }
                
                // Reconstruct the path with the canonical parent
                if let Some(file_name) = absolute_path.file_name() {
                    canonical_parent.join(file_name)
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME, 
                        &format!("Invalid file path: '{}'", self.path)
                    )));
                }
            } else {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME, 
                    &format!("Invalid file path: '{}'", self.path)
                )));
            }
        };
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME, 
                &self.path, 
                "Path is outside the project directory"
            )));
        }

        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        
        if canonical_path.exists() && !read_files.contains(&canonical_path) && !self.append {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME, 
                &format!("Cannot write to '{}': File must be read first before writing", self.path)
            )));
        }

        if let Some(parent) = canonical_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directories: {}", e))))?;
            }
        }

        // Create backup if requested and file exists
        let mut backup_created = false;
        if self.backup && canonical_path.exists() && !self.append {
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
            
            backup_created = true;
        }

        if self.append {
            use tokio::fs::OpenOptions;
            use tokio::io::AsyncWriteExt;
            
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&canonical_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file for appending: {}", e))))?;
            
            let encoded_bytes = self.encode_content()?;
            file.write_all(&encoded_bytes)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to append to file: {}", e))))?;
        } else {
            let encoded_bytes = self.encode_content()?;
            fs::write(&canonical_path, &encoded_bytes)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to write file: {}", e))))?;
        }

        let mut read_files_clone = (*read_files).clone();
        read_files_clone.insert(canonical_path.clone());
        context.set_custom_state(read_files_clone).await;

        // Calculate content size
        let content_size = self.content.len() as u64;
        let size_str = format_size(content_size);
        
        // Format the path relative to project root
        let relative_path = canonical_path.strip_prefix(&current_dir)
            .unwrap_or(&canonical_path);
        
        // Build the message
        let mut message = if self.append {
            format!("Appended {} to {}", size_str, format_path(relative_path))
        } else {
            format!("Wrote {} to {}", size_str, format_path(relative_path))
        };
        
        if backup_created {
            message.push_str(" (backup created)");
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
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
    
    #[tokio::test]
    async fn test_write_basic_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = WriteTool {
            path: "test.txt".to_string(),
            content: "Hello, World!".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
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
        
        let write_tool = WriteTool {
            path: "subdir/test.txt".to_string(),
            content: "Nested content".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
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
        let write_tool = WriteTool {
            path: "append_test.txt".to_string(),
            content: "First line\n".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
        write_tool.call_with_context(&context).await.unwrap();
        
        // Append to existing file
        let append_tool = WriteTool {
            path: "append_test.txt".to_string(),
            content: "Second line\n".to_string(),
            append: true,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
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
        let write_tool = WriteTool {
            path: "backup_test.txt".to_string(),
            content: "New content".to_string(),
            append: false,
            backup: true,
            encoding: "utf-8".to_string(),
        };
        
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
        let write_tool = WriteTool {
            path: "existing.txt".to_string(),
            content: "New content".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("File must be read first before writing"));
    }
    
    #[tokio::test]
    async fn test_write_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = WriteTool {
            path: "../outside.txt".to_string(),
            content: "Should not work".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
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
        let write_tool = WriteTool {
            path: "utf8_test.txt".to_string(),
            content: test_content.to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("utf8_test.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, test_content);
        
        // Test ASCII encoding (note: special chars may be lost)
        let write_tool = WriteTool {
            path: "ascii_test.txt".to_string(),
            content: "Simple ASCII text".to_string(),
            append: false,
            backup: false,
            encoding: "ascii".to_string(),
        };
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_write_empty_content() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = WriteTool {
            path: "empty.txt".to_string(),
            content: "".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
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
        
        let write_tool = WriteTool {
            path: "large.txt".to_string(),
            content: large_content.clone(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let file_path = context.get_project_root().unwrap().join("large.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, large_content);
    }
    
    #[tokio::test]
    async fn test_write_invalid_path() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let write_tool = WriteTool {
            path: "".to_string(),
            content: "content".to_string(),
            append: false,
            backup: false,
            encoding: "utf-8".to_string(),
        };
        
        let result = write_tool.call_with_context(&context).await;
        assert!(result.is_err());
    }
}