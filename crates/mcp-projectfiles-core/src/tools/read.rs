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
use regex::RegexBuilder;
use encoding_rs;

fn default_encoding() -> String {
    "utf-8".to_string()
}

fn deserialize_optional_pattern<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.and_then(|s| if s.is_empty() || s == "null" { None } else { Some(s) }))
}

#[mcp_tool(name = "read", description = "Reads text file contents within the project directory only. Returns content with line numbers (format: line_number<tab>content). Supports partial reading via offset/limit, tail mode for reading from end, pattern filtering, binary file detection, and handles large files efficiently. NOTE: Optional parameters should be omitted entirely when not needed, rather than passed as null.")]
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
    /// Read from the end of the file (tail mode). If true, offset is from end (default: false)
    #[serde(default)]
    pub tail: bool,
    /// Pattern to filter lines (regex). Only lines matching this pattern will be returned
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_optional_pattern")]
    pub pattern: Option<String>,
    /// Whether pattern matching should be case-insensitive (default: false)
    #[serde(default)]
    pub pattern_case_insensitive: bool,
    /// Text encoding to use when reading the file (default: "utf-8")
    /// Supported: "utf-8", "ascii", "latin1", "utf-16", "utf-16le", "utf-16be"
    #[serde(default = "default_encoding")]
    pub encoding: String,
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

        // Read the full file content with encoding support
        let full_content = self.read_file_with_encoding(&canonical_path).await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read file: {}", e)))?;
        
        let all_lines: Vec<&str> = full_content.lines().collect();
        let original_line_count = all_lines.len();
        
        // Apply pattern filtering if specified
        let (lines, line_numbers): (Vec<&str>, Vec<usize>) = if let Some(ref pattern) = self.pattern {
            let regex = match RegexBuilder::new(pattern)
                .case_insensitive(self.pattern_case_insensitive)
                .build()
            {
                Ok(r) => r,
                Err(e) => return Err(CallToolError::unknown_tool(format!(
                    "Invalid regex pattern '{}': {}",
                    pattern, e
                ))),
            };
            
            let mut filtered_lines = Vec::new();
            let mut filtered_line_numbers = Vec::new();
            
            for (idx, line) in all_lines.iter().enumerate() {
                if regex.is_match(line) {
                    filtered_lines.push(*line);
                    filtered_line_numbers.push(idx + 1); // 1-indexed
                }
            }
            
            (filtered_lines, filtered_line_numbers)
        } else {
            // No filtering - include all lines with sequential line numbers
            let line_numbers = (1..=all_lines.len()).collect();
            (all_lines, line_numbers)
        };
        
        let total_lines = lines.len();
        
        // Determine the range to read
        let (start, end) = if self.tail {
            // Tail mode: read from the end
            let end_offset = if self.offset > 0 {
                total_lines.saturating_sub(self.offset as usize)
            } else {
                total_lines
            };
            
            let start = if self.limit > 0 {
                end_offset.saturating_sub(self.limit as usize)
            } else {
                0
            };
            
            (start, end_offset)
        } else {
            // Normal mode: read from the beginning
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
            
            (start, end)
        };
        
        // Format the output
        let content = if start >= total_lines {
            String::from("[No content at specified offset]")
        } else {
            let selected_lines = &lines[start..end];
            let selected_line_numbers = &line_numbers[start..end];
            let mut result = String::with_capacity((end - start) * 80); // Estimate capacity
            
            for (idx, line) in selected_lines.iter().enumerate() {
                let line_num = selected_line_numbers[idx];
                result.push_str(&format!("{:>6}\t{}\n", line_num, line));
            }
            
            // Add truncation notice if needed
            if self.pattern.is_some() {
                if self.limit > 0 && end < total_lines {
                    result.push_str(&format!(
                        "\n[Pattern matched {} lines out of {} total. Showing lines {}-{}. Use offset={} to continue]",
                        total_lines, original_line_count, start + 1, end, end + 1
                    ));
                } else if total_lines < original_line_count {
                    result.push_str(&format!(
                        "\n[Pattern matched {} lines out of {} total lines]",
                        total_lines, original_line_count
                    ));
                }
            } else if self.tail && self.limit > 0 && start > 0 {
                result.push_str(&format!(
                    "\n[Tail mode: Showing last {} lines. File has {} total lines. Use limit={} to see more]",
                    end - start, total_lines, self.limit + 10
                ));
            } else if !self.tail && self.limit > 0 && end < total_lines {
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

    async fn read_file_with_encoding(&self, path: &Path) -> Result<String, std::io::Error> {
        let bytes = fs::read(path).await?;
        
        let encoding = match self.encoding.to_lowercase().as_str() {
            "utf-8" | "utf8" => encoding_rs::UTF_8,
            "ascii" => encoding_rs::WINDOWS_1252, // ASCII is a subset of Windows-1252
            "latin1" | "iso-8859-1" => encoding_rs::WINDOWS_1252,
            "utf-16" => encoding_rs::UTF_16LE, // Default to little-endian
            "utf-16le" => encoding_rs::UTF_16LE,
            "utf-16be" => encoding_rs::UTF_16BE,
            _ => encoding_rs::UTF_8, // Default fallback
        };

        let (decoded, _encoding_used, had_errors) = encoding.decode(&bytes);
        
        if had_errors {
            eprintln!("Warning: Some characters could not be decoded with {} encoding", self.encoding);
        }
        
        Ok(decoded.into_owned())
    }
}