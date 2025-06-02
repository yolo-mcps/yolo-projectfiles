use crate::context::{StatefulTool, ToolContext};
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

fn default_encoding() -> String {
    "utf-8".to_string()
}

#[mcp_tool(name = "write", description = "Writes content to files within the project directory only. IMPORTANT: Existing files must be read first using the read tool. Creates parent directories automatically if needed. Supports both overwrite and append modes.")]
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
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
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
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path '{}': {}", self.path, e)))?
        } else {
            // For new files, canonicalize the parent directory
            if let Some(parent) = absolute_path.parent() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve parent directory: {}", e)))?;
                
                // Ensure the parent is within the project directory
                if !canonical_parent.starts_with(&current_dir) {
                    return Err(CallToolError::unknown_tool(format!(
                        "Access denied: Path '{}' is outside the project directory",
                        self.path
                    )));
                }
                
                // Reconstruct the path with the canonical parent
                if let Some(file_name) = absolute_path.file_name() {
                    canonical_parent.join(file_name)
                } else {
                    return Err(CallToolError::unknown_tool(format!(
                        "Invalid file path: '{}'",
                        self.path
                    )));
                }
            } else {
                return Err(CallToolError::unknown_tool(format!(
                    "Invalid file path: '{}'",
                    self.path
                )));
            }
        };
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }

        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        
        if canonical_path.exists() && !read_files.contains(&canonical_path) && !self.append {
            return Err(CallToolError::unknown_tool(format!(
                "Cannot write to '{}': File must be read first before writing",
                self.path
            )));
        }

        if let Some(parent) = canonical_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to create parent directories: {}", e)))?;
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
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to create backup: {}", e)))?;
            
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
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to open file for appending: {}", e)))?;
            
            let encoded_bytes = self.encode_content()?;
            file.write_all(&encoded_bytes)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to append to file: {}", e)))?;
        } else {
            let encoded_bytes = self.encode_content()?;
            fs::write(&canonical_path, &encoded_bytes)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to write file: {}", e)))?;
        }

        let mut read_files_clone = (*read_files).clone();
        read_files_clone.insert(canonical_path.clone());
        context.set_custom_state(read_files_clone).await;

        let action = if self.append { "appended" } else { "wrote" };
        let backup_msg = if backup_created {
            " (backup created)"
        } else {
            ""
        };
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!(
                    "Successfully {} {} bytes to {}{}",
                    action,
                    self.content.len(),
                    self.path,
                    backup_msg
                ),
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