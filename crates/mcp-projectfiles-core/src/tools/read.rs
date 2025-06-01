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

#[mcp_tool(name = "read", description = "Reads text file contents within the project directory only. Returns content with line numbers (format: line_number<tab>content). Supports partial reading via offset/limit, binary file detection, and handles large files efficiently.")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ReadTool {
    /// Path to the file to read (relative to project root)
    pub path: String,
    /// Starting line number (1-indexed). Use 0 for beginning of file (default)
    #[serde(default)]
    pub offset: u32,
    /// Maximum number of lines to read. Use 0 for all lines (default)
    #[serde(default)]
    pub limit: u32,
    /// Skip binary file detection if true (default: false)
    #[serde(default)]
    pub skip_binary_check: bool,
}

#[async_trait]
impl StatefulTool for ReadTool {
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
        
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path '{}': {}", self.path, e)))?;
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "File not found: {}",
                self.path
            )));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::unknown_tool(format!(
                "Path is not a file: {}",
                self.path
            )));
        }

        // Binary file detection (unless skipped)
        if !self.skip_binary_check {
            let mut file = tokio::fs::File::open(&canonical_path).await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to open file: {}", e)))?;
            
            let file_size = file.metadata().await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to get file metadata: {}", e)))?
                .len() as usize;
            
            let sample_size = 8192.min(file_size);
            let mut buffer = vec![0; sample_size];
            
            use tokio::io::AsyncReadExt;
            let bytes_read = file.read(&mut buffer).await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read file: {}", e)))?;
            
            buffer.truncate(bytes_read);
            
            // Check for null bytes or high proportion of non-text bytes
            let non_text_bytes = buffer.iter()
                .filter(|&&b| b == 0 || (b < 32 && b != 9 && b != 10 && b != 13) || b > 126)
                .count();
            
            if non_text_bytes > buffer.len() / 10 {
                return Err(CallToolError::unknown_tool(format!(
                    "Cannot read binary file: {}. Use skip_binary_check=true to force reading.",
                    self.path
                )));
            }
        }

        // Read the full file content
        let full_content = fs::read_to_string(&canonical_path)
            .await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read file: {}", e)))?;
        
        let lines: Vec<&str> = full_content.lines().collect();
        let total_lines = lines.len();
        
        // Determine the range to read
        let start = if self.offset > 0 {
            (self.offset as usize).saturating_sub(1)
        } else {
            0
        };
        
        let end = if self.limit > 0 {
            (start + self.limit as usize).min(total_lines)
        } else {
            total_lines
        };
        
        // Format the output
        let content = if start >= total_lines {
            String::from("[No content at specified offset]")
        } else {
            let selected_lines = &lines[start..end];
            let mut result = String::with_capacity((end - start) * 80); // Estimate capacity
            
            for (idx, line) in selected_lines.iter().enumerate() {
                let line_num = start + idx + 1;
                result.push_str(&format!("{:>6}\t{}\n", line_num, line));
            }
            
            // Add truncation notice if needed
            if self.limit > 0 && end < total_lines {
                result.push_str(&format!(
                    "\n[Truncated at line {}. File has {} total lines. Use offset={} to continue reading]",
                    end, total_lines, end + 1
                ));
            }
            
            result
        };

        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut read_files_clone = (*read_files).clone();
        read_files_clone.insert(canonical_path.clone());
        context.set_custom_state(read_files_clone).await;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                content, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl ReadTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}