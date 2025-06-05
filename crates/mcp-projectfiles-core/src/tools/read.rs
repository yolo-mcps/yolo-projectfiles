use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use regex::RegexBuilder;
use encoding_rs;
use chrono::{DateTime, Utc};

const TOOL_NAME: &str = "read";

#[derive(Serialize, Deserialize, Debug)]
struct FileMetadata {
    size: u64,
    size_human: String,
    modified: String,
    lines: usize,
    encoding: String,
    has_bom: bool,
    is_binary: bool,
}

fn default_encoding() -> String {
    "utf-8".to_string()
}

fn default_linenumbers() -> bool {
    true
}

fn default_follow_symlinks() -> bool {
    true
}

fn default_binary_check() -> bool {
    true
}

fn default_case() -> String {
    "sensitive".to_string()
}


#[mcp_tool(name = "read", description = "Read text files within the project. Preferred over system 'cat', 'head', 'tail' commands.

IMPORTANT: Files must exist within the project directory unless accessed via symlinks with follow_symlinks=true.
NOTE: Omit optional parameters when not needed, don't pass null.
HINT: Use linenumbers:false when reading files you plan to edit.

Features:
- Line numbers by default (format: line_number<tab>content)
- Partial reading with offset/limit or line ranges
- Pattern filtering with regex (with optional inversion)
- Context lines around pattern matches
- Tail mode for reading from end
- Binary file detection with BOM awareness
- Multiple encoding support
- File metadata reporting
- Preview mode for large files
- Symlink support with configurable behavior

Parameters:
- path: File path (required)
- offset: Start line (1-indexed, default: 0 for beginning)
- limit: Max lines to read (0 for all, default: 0)
- line_range: Line range like \"10-20\" (optional, overrides offset/limit)
- linenumbers: Show line numbers (optional, default: true)
- tail: Read from end (optional, default: false)
- pattern: Regex to filter lines (optional)
- invert_match: Show lines NOT matching pattern (optional, default: false)
- context_before: Lines of context before match (optional, default: 0)
- context_after: Lines of context after match (optional, default: 0)
- case: Pattern matching case - \"sensitive\" or \"insensitive\" (optional, default: \"sensitive\")
- encoding: Text encoding - \"utf-8\", \"ascii\", \"latin1\", \"utf-16\", \"utf-16le\", \"utf-16be\" (optional, default: \"utf-8\")
- follow_symlinks: Follow symlinks to read files outside project directory (optional, default: true)
- binary_check: Perform binary file detection (optional, default: true)
- preview_only: Show file info without reading content (optional, default: false)
- include_metadata: Include file size, modified time, etc. (optional, default: false)

Examples:
- Basic read: {\"path\": \"README.md\"}
- Read specific lines: {\"path\": \"src/main.rs\", \"line_range\": \"10-20\"}
- Read with offset/limit: {\"path\": \"large.log\", \"offset\": 100, \"limit\": 50}
- Tail last 20 lines: {\"path\": \"app.log\", \"tail\": true, \"limit\": 20}
- Find TODO comments: {\"path\": \"src/lib.rs\", \"pattern\": \"TODO\"}
- Find non-comment lines: {\"path\": \"config.toml\", \"pattern\": \"^\\\\s*#\", \"invert_match\": true}
- Search with context: {\"path\": \"error.log\", \"pattern\": \"ERROR\", \"context_before\": 2, \"context_after\": 3}
- Preview large file: {\"path\": \"database.sql\", \"preview_only\": true}
- Read without line numbers: {\"path\": \"template.txt\", \"linenumbers\": false}
- Case-insensitive search: {\"path\": \"notes.txt\", \"pattern\": \"important\", \"case\": \"insensitive\"}

Returns:
- content: The file content with optional line numbers
- metadata: File information (if include_metadata=true or preview_only=true)
  - size: File size in bytes
  - size_human: Human-readable size
  - modified: Last modification time
  - lines: Total line count
  - encoding: Detected encoding
  - has_bom: Whether file has BOM
  - is_binary: Whether file appears to be binary")]
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
    /// Line range to read (e.g., \"10-20\"). Overrides offset/limit if provided
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_range: Option<String>,
    /// Perform binary file detection if true (default: true)
    #[serde(default = "default_binary_check")]
    pub binary_check: bool,
    /// Read from the end of the file (tail mode). If true, offset is from end (default: false)
    #[serde(default)]
    pub tail: bool,
    /// Pattern to filter lines (regex). Only lines matching this pattern will be returned
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Invert pattern matching - show lines that do NOT match the pattern (default: false)
    #[serde(default)]
    pub invert_match: bool,
    /// Number of context lines to show before each match (default: 0)
    #[serde(default)]
    pub context_before: u32,
    /// Number of context lines to show after each match (default: 0)
    #[serde(default)]
    pub context_after: u32,
    /// Case sensitivity for pattern matching: "sensitive" (default) or "insensitive"
    #[serde(default = "default_case")]
    pub case: String,
    /// Text encoding to use when reading the file (default: "utf-8")
    /// Supported: "utf-8", "ascii", "latin1", "utf-16", "utf-16le", "utf-16be"
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Show line numbers in output (default: true)
    #[serde(default = "default_linenumbers")]
    pub linenumbers: bool,
    /// Follow symlinks to read content outside the project directory (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    /// Preview mode - show file info without reading content (default: false)
    #[serde(default)]
    pub preview_only: bool,
    /// Include file metadata in response (default: false)
    #[serde(default)]
    pub include_metadata: bool,
}

#[async_trait]
impl StatefulTool for ReadTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Validate case parameter
        if self.case != "sensitive" && self.case != "insensitive" {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Invalid case value '{}'. Must be 'sensitive' or 'insensitive'", self.case)
            )));
        }
        
        // Use the utility function to resolve path with symlink support
        let canonical_path = resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;

        if !canonical_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME,
                &self.path
            )));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path is not a file: {}", self.path)
            )));
        }

        // Get file metadata early if needed
        let file_metadata = fs::metadata(&canonical_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?;
        
        let file_size = file_metadata.len();
        
        // If preview_only mode, return just metadata
        if self.preview_only {
            let metadata = self.create_file_metadata(&canonical_path, file_size, &file_metadata).await?;
            return Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    serde_json::to_string_pretty(&metadata).unwrap(), None,
                ))],
                is_error: Some(false),
                meta: None,
            });
        }

        // Binary file detection (unless skipped)
        if self.binary_check {
            let mut file = tokio::fs::File::open(&canonical_path).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
            
            let file_size = file.metadata().await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?
                .len() as usize;
            
            let sample_size = 8192.min(file_size);
            let mut buffer = vec![0; sample_size];
            
            let bytes_read = file.read(&mut buffer).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
            
            buffer.truncate(bytes_read);
            
            // Check for null bytes or high proportion of non-text bytes
            let non_text_bytes = buffer.iter()
                .filter(|&&b| b == 0 || (b < 32 && b != 9 && b != 10 && b != 13) || b > 126)
                .count();
            
            if non_text_bytes > buffer.len() / 10 {
                return Err(CallToolError::from(tool_errors::binary_file(TOOL_NAME, &self.path)));
            }
        }

        // Read the full file content with encoding support
        let full_content = self.read_file_with_encoding(&canonical_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        let all_lines: Vec<&str> = full_content.lines().collect();
        let original_line_count = all_lines.len();
        
        // Parse line range if provided
        let (range_start, range_end) = if let Some(ref range) = self.line_range {
            self.parse_line_range(range, all_lines.len())?
        } else {
            (None, None)
        };

        // Apply pattern filtering if specified
        let (lines, line_numbers): (Vec<&str>, Vec<usize>) = if let Some(ref pattern) = self.pattern {
            let regex = match RegexBuilder::new(pattern)
                .case_insensitive(self.case == "insensitive")
                .build()
            {
                Ok(r) => r,
                Err(e) => return Err(CallToolError::from(tool_errors::pattern_error(TOOL_NAME, pattern, &e.to_string()))),
            };
            
            let mut filtered_lines = Vec::new();
            let mut filtered_line_numbers = Vec::new();
            
            // If we need context lines, we'll need to track matches differently
            if self.context_before > 0 || self.context_after > 0 {
                let mut match_indices = HashSet::new();
                let mut expanded_indices = HashSet::new();
                
                // First pass: find all matching lines
                for (idx, line) in all_lines.iter().enumerate() {
                    let matches = if self.invert_match {
                        !regex.is_match(line)
                    } else {
                        regex.is_match(line)
                    };
                    
                    if matches {
                        match_indices.insert(idx);
                        // Add context lines
                        for i in idx.saturating_sub(self.context_before as usize)..=idx + self.context_after as usize {
                            if i < all_lines.len() {
                                expanded_indices.insert(i);
                            }
                        }
                    }
                }
                
                // Second pass: collect lines with context
                for idx in 0..all_lines.len() {
                    if expanded_indices.contains(&idx) {
                        filtered_lines.push(all_lines[idx]);
                        filtered_line_numbers.push(idx + 1);
                    }
                }
            } else {
                // Simple filtering without context
                for (idx, line) in all_lines.iter().enumerate() {
                    let matches = if self.invert_match {
                        !regex.is_match(line)
                    } else {
                        regex.is_match(line)
                    };
                    
                    if matches {
                        filtered_lines.push(*line);
                        filtered_line_numbers.push(idx + 1); // 1-indexed
                    }
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
        let (start, end) = if let (Some(rs), Some(re)) = (range_start, range_end) {
            // Use line range if provided
            let start = rs.saturating_sub(1).min(total_lines);
            let end = re.min(total_lines);
            (start, end)
        } else if self.tail {
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
                if self.linenumbers {
                    let line_num = selected_line_numbers[idx];
                    result.push_str(&format!("{:>6}\t{}\n", line_num, line));
                } else {
                    result.push_str(&format!("{}\n", line));
                }
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

        // Build response with optional metadata
        if self.include_metadata {
            let metadata = self.create_file_metadata(&canonical_path, file_size, &file_metadata).await?;
            let response = json!({
                "content": content,
                "metadata": metadata
            });
            Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    serde_json::to_string_pretty(&response).unwrap(), None,
                ))],
                is_error: Some(false),
                meta: None,
            })
        } else {
            Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    content, None,
                ))],
                is_error: Some(false),
                meta: None,
            })
        }
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

    fn parse_line_range(&self, range: &str, total_lines: usize) -> Result<(Option<usize>, Option<usize>), CallToolError> {
        let parts: Vec<&str> = range.split('-').collect();
        
        match parts.len() {
            1 => {
                // Single line number
                let line = parts[0].trim().parse::<usize>()
                    .map_err(|_| CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Invalid line number: {}", parts[0])
                    )))?;
                Ok((Some(line), Some(line)))
            }
            2 => {
                // Range start-end
                let start = if parts[0].trim().is_empty() {
                    1
                } else {
                    parts[0].trim().parse::<usize>()
                        .map_err(|_| CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Invalid start line number: {}", parts[0])
                        )))?
                };
                
                let end = if parts[1].trim().is_empty() {
                    total_lines
                } else {
                    parts[1].trim().parse::<usize>()
                        .map_err(|_| CallToolError::from(tool_errors::invalid_input(
                            TOOL_NAME,
                            &format!("Invalid end line number: {}", parts[1])
                        )))?
                };
                
                if start > end {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Invalid line range: start ({}) is greater than end ({})", start, end)
                    )));
                }
                
                Ok((Some(start), Some(end)))
            }
            _ => {
                Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Invalid line range format: {}. Expected 'N' or 'N-M'", range)
                )))
            }
        }
    }

    async fn create_file_metadata(&self, path: &Path, size: u64, metadata: &std::fs::Metadata) -> Result<FileMetadata, CallToolError> {
        // Detect BOM
        let mut has_bom = false;
        let mut is_binary = false;
        
        if size > 0 {
            let mut file = tokio::fs::File::open(path).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file for BOM check: {}", e))))?;
            
            let mut bom_buffer = vec![0; 4.min(size as usize)];
            let _ = file.read(&mut bom_buffer).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read BOM: {}", e))))?;
            
            // Check for various BOMs
            has_bom = match bom_buffer.len() {
                n if n >= 3 && bom_buffer[0..3] == [0xEF, 0xBB, 0xBF] => true, // UTF-8
                n if n >= 2 && bom_buffer[0..2] == [0xFF, 0xFE] => true, // UTF-16 LE
                n if n >= 2 && bom_buffer[0..2] == [0xFE, 0xFF] => true, // UTF-16 BE
                n if n >= 4 && bom_buffer[0..4] == [0xFF, 0xFE, 0x00, 0x00] => true, // UTF-32 LE
                n if n >= 4 && bom_buffer[0..4] == [0x00, 0x00, 0xFE, 0xFF] => true, // UTF-32 BE
                _ => false,
            };
            
            // Quick binary check
            let sample_size = 8192.min(size as usize);
            let mut buffer = vec![0; sample_size];
            file.seek(tokio::io::SeekFrom::Start(0)).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to seek: {}", e))))?;
            let bytes_read = file.read(&mut buffer).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read for binary check: {}", e))))?;
            
            buffer.truncate(bytes_read);
            let non_text_bytes = buffer.iter()
                .filter(|&&b| b == 0 || (b < 32 && b != 9 && b != 10 && b != 13) || b > 126)
                .count();
            
            is_binary = non_text_bytes > buffer.len() / 10;
        }
        
        // Count lines if text file
        let line_count = if !is_binary && size > 0 {
            let content = self.read_file_with_encoding(path).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to count lines: {}", e))))?;
            content.lines().count()
        } else {
            0
        };
        
        // Format file size
        let size_human = if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else if size < 1024 * 1024 * 1024 {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
        };
        
        // Get modification time
        let modified = metadata.modified()
            .map(|t| {
                let datetime: DateTime<Utc> = t.into();
                datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
            })
            .unwrap_or_else(|_| "Unknown".to_string());
        
        Ok(FileMetadata {
            size,
            size_human,
            modified,
            lines: line_count,
            encoding: self.encoding.clone(),
            has_bom,
            is_binary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    use tokio::fs as async_fs;

    async fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(name);
        async_fs::write(&file_path, content).await.unwrap();
        file_path
    }

    fn create_read_tool(path: &str) -> ReadTool {
        ReadTool {
            path: path.to_string(),
            offset: 0,
            limit: 0,
            line_range: None,
            binary_check: true,
            tail: false,
            pattern: None,
            invert_match: false,
            context_before: 0,
            context_after: 0,
            case: "sensitive".to_string(),
            encoding: "utf-8".to_string(),
            linenumbers: true,
            follow_symlinks: true,
            preview_only: false,
            include_metadata: false,
        }
    }



    // Clean test helper that doesn't change global state
    async fn test_read_tool_in_dir(temp_dir: &TempDir, tool: ReadTool) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        tool.call_with_context(&context).await
    }

    // Basic functionality tests
    #[tokio::test]
    async fn test_read_basic_file() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let tool = create_read_tool("test.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tLine 1"));
        assert!(output.contains("     2\tLine 2"));
        assert!(output.contains("     3\tLine 3"));
    }

    #[tokio::test]
    async fn test_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let _file_path = create_test_file(&temp_dir, "empty.txt", "").await;
        
        let tool = create_read_tool("empty.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Empty file should return "[No content at specified offset]" message
        assert_eq!(output.trim(), "[No content at specified offset]");
    }

    #[tokio::test]
    async fn test_read_single_line() {
        let temp_dir = TempDir::new().unwrap();
        let _file_path = create_test_file(&temp_dir, "single.txt", "Single line without newline").await;
        
        let tool = create_read_tool("single.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tSingle line without newline"));
    }

    #[tokio::test]
    async fn test_line_number_formatting() {
        let temp_dir = TempDir::new().unwrap();
        let content = (1..=15).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
        let _file_path = create_test_file(&temp_dir, "numbers.txt", &content).await;
        
        let tool = create_read_tool("numbers.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tLine 1"));
        assert!(output.contains("    10\tLine 10"));
        assert!(output.contains("    15\tLine 15"));
    }

    // Path security tests
    #[tokio::test]
    async fn test_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let tool = create_read_tool("nonexistent.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_directory_instead_of_file() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        async_fs::create_dir(&sub_dir).await.unwrap();
        
        let tool = create_read_tool("subdir");
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path is not a file"));
    }

    // Binary detection tests
    #[tokio::test]
    async fn test_binary_file_detected() {
        let temp_dir = TempDir::new().unwrap();
        let binary_content = vec![0, 1, 2, 3, 4, 5, 255, 254, 253]; // High ratio of non-text bytes
        let file_path = temp_dir.path().join("binary.bin");
        async_fs::write(&file_path, binary_content).await.unwrap();
        
        let tool = create_read_tool("binary.bin");
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Binary file detected"));
    }

    #[tokio::test]
    async fn test_text_file_passes_binary_check() {
        let temp_dir = TempDir::new().unwrap();
        let _file_path = create_test_file(&temp_dir, "text.txt", "Normal text content").await;
        
        let tool = create_read_tool("text.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_binary_check_flag() {
        let temp_dir = TempDir::new().unwrap();
        let binary_content = vec![0, 1, 2, 3, 4, 5, 255, 254, 253];
        let file_path = temp_dir.path().join("binary.bin");
        async_fs::write(&file_path, binary_content).await.unwrap();
        

        
        let mut tool = create_read_tool("binary.bin");
        tool.binary_check = false; // Disable binary check
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        // Should not fail due to binary detection when binary_check is false
        assert!(result.is_ok());
    }

    // Offset/limit tests
    #[tokio::test]
    async fn test_offset_from_beginning() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "offset_test.txt", content).await;
        
        let mut tool = create_read_tool("offset_test.txt");
        tool.offset = 3; // Start from line 3
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     3\tLine 3"));
        assert!(output.contains("     4\tLine 4"));
        assert!(output.contains("     5\tLine 5"));
        assert!(!output.contains("Line 1"));
        assert!(!output.contains("Line 2"));
    }

    #[tokio::test]
    async fn test_limit_partial_read() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "limit_test.txt", content).await;
        

        
        let mut tool = create_read_tool("limit_test.txt");
        tool.limit = 2; // Read only 2 lines
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tLine 1"));
        assert!(output.contains("     2\tLine 2"));
        assert!(!output.contains("Line 3"));
        assert!(output.contains("Truncated at line 2"));
    }

    #[tokio::test]
    async fn test_offset_beyond_file() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2";
        let _file_path = create_test_file(&temp_dir, "short.txt", content).await;
        

        
        let mut tool = create_read_tool("short.txt");
        tool.offset = 10; // Beyond file length
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert_eq!(output.trim(), "[No content at specified offset]");
    }

    // Tail mode tests
    #[tokio::test]
    async fn test_tail_no_offset() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "tail_test.txt", content).await;
        

        
        let mut tool = create_read_tool("tail_test.txt");
        tool.tail = true;
        tool.limit = 2; // Last 2 lines
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     4\tLine 4"));
        assert!(output.contains("     5\tLine 5"));
        assert!(!output.contains("Line 1"));
    }

    #[tokio::test]
    async fn test_tail_with_offset() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "tail_offset.txt", content).await;
        

        
        let mut tool = create_read_tool("tail_offset.txt");
        tool.tail = true;
        tool.offset = 2; // Skip last 2 lines from end
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tLine 1"));
        assert!(output.contains("     2\tLine 2"));
        assert!(output.contains("     3\tLine 3"));
        assert!(!output.contains("Line 4"));
        assert!(!output.contains("Line 5"));
    }

    // Pattern filtering tests
    #[tokio::test]
    async fn test_pattern_basic_match() {
        let temp_dir = TempDir::new().unwrap();
        let content = "fn main() {\n    println!(\"Hello\");\n}\nfn helper() {\n    // TODO: implement\n}";
        let _file_path = create_test_file(&temp_dir, "pattern_test.txt", content).await;
        
        let mut tool = create_read_tool("pattern_test.txt");
        tool.pattern = Some("fn".to_string());
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     1\tfn main()"));
        assert!(output.contains("     4\tfn helper()"));
        assert!(!output.contains("println"));
        assert!(output.contains("Pattern matched 2 lines"));
    }

    #[tokio::test]
    async fn test_pattern_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let content = "TODO: fix bug\ntodo: add tests\nDone: complete feature";
        let _file_path = create_test_file(&temp_dir, "case_test.txt", content).await;
        

        
        let mut tool = create_read_tool("case_test.txt");
        tool.pattern = Some("todo".to_string());
        tool.case = "insensitive".to_string();
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("TODO: fix bug"));
        assert!(output.contains("todo: add tests"));
        assert!(!output.contains("Done: complete"));
        assert!(output.contains("Pattern matched 2 lines"));
    }

    #[tokio::test]
    async fn test_pattern_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3";
        let _file_path = create_test_file(&temp_dir, "no_match.txt", content).await;
        

        
        let mut tool = create_read_tool("no_match.txt");
        tool.pattern = Some("nonexistent".to_string());
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert_eq!(output.trim(), "[No content at specified offset]");
    }

    #[tokio::test]
    async fn test_pattern_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2";
        let _file_path = create_test_file(&temp_dir, "regex_test.txt", content).await;
        

        
        let mut tool = create_read_tool("regex_test.txt");
        tool.pattern = Some("[invalid regex".to_string()); // Invalid regex
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        println!("Error message: {}", error_msg);
        assert!(error_msg.contains("pattern") || error_msg.contains("regex"));
    }

    // Encoding tests
    #[tokio::test]
    async fn test_utf8_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let content = "UTF-8 with special chars: àáâãäå ñúü";
        let _file_path = create_test_file(&temp_dir, "utf8.txt", content).await;
        

        
        let mut tool = create_read_tool("utf8.txt");
        tool.encoding = "utf-8".to_string();
        tool.binary_check = false; // Skip binary check for this test
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("àáâãäå"));
        assert!(output.contains("ñúü"));
    }

    // Line numbers tests
    #[tokio::test]
    async fn test_read_with_line_numbers() {
        let temp_dir = TempDir::new().unwrap();
        let content = "First line\nSecond line\nThird line";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let mut tool = create_read_tool("test.txt");
        tool.linenumbers = true; // Explicitly set to true (though it's the default)
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should have line numbers
        assert!(output.contains("     1\tFirst line"));
        assert!(output.contains("     2\tSecond line"));
        assert!(output.contains("     3\tThird line"));
    }

    #[tokio::test]
    async fn test_read_without_line_numbers() {
        let temp_dir = TempDir::new().unwrap();
        let content = "First line\nSecond line\nThird line";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let mut tool = create_read_tool("test.txt");
        tool.linenumbers = false; // Disable line numbers
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should NOT have line numbers
        assert!(!output.contains("\t"));
        assert_eq!(output.trim(), "First line\nSecond line\nThird line");
    }

    #[tokio::test]
    async fn test_read_without_line_numbers_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let mut tool = create_read_tool("test.txt");
        tool.linenumbers = false;
        tool.limit = 3;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should have first 3 lines without numbers
        assert!(output.contains("Line 1\nLine 2\nLine 3"));
        assert!(!output.contains("Line 4"));
        assert!(!output.contains("\t"));
    }

    #[tokio::test]
    async fn test_read_without_line_numbers_with_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let content = "apple\nbanana\napricot\nblueberry\navocado";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let mut tool = create_read_tool("test.txt");
        tool.linenumbers = false;
        tool.pattern = Some("^a".to_string()); // Lines starting with 'a'
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should have only lines starting with 'a', no line numbers
        assert!(output.contains("apple"));
        assert!(output.contains("apricot"));
        assert!(output.contains("avocado"));
        assert!(!output.contains("banana"));
        assert!(!output.contains("blueberry"));
        assert!(!output.contains("\t"));
    }

    // Case parameter validation tests
    #[tokio::test]
    async fn test_invalid_case_parameter() {
        let temp_dir = TempDir::new().unwrap();
        let _file_path = create_test_file(&temp_dir, "test.txt", "Test content").await;
        
        let mut tool = create_read_tool("test.txt");
        tool.case = "invalid".to_string();
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid case value"));
    }

    // Symlink following tests
    #[tokio::test]
    async fn test_read_symlink_within_project() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Content from target file";
        let target_file = create_test_file(&temp_dir, "target.txt", content).await;
        
        // Create symlink within project directory
        let symlink_path = temp_dir.path().join("link.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&target_file, &symlink_path).unwrap();
        }
        
        let tool = create_read_tool("link.txt");
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("Content from target file"));
    }

    #[tokio::test]
    async fn test_read_symlink_outside_project() {
        let temp_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        
        // Create file outside project directory
        let external_content = "External file content";
        let external_file = external_dir.path().join("external.txt");
        async_fs::write(&external_file, external_content).await.unwrap();
        
        // Create symlink inside project pointing to external file
        let symlink_path = temp_dir.path().join("external_link.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&external_file, &symlink_path).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&external_file, &symlink_path).unwrap();
        }
        
        // Should be able to read external file through symlink when follow_symlinks=true
        let mut tool = create_read_tool("external_link.txt");
        tool.follow_symlinks = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("External file content"));
    }

    #[tokio::test]
    async fn test_read_symlink_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        
        // Create file outside project directory
        let external_content = "External file content";
        let external_file = external_dir.path().join("external.txt");
        async_fs::write(&external_file, external_content).await.unwrap();
        
        // Create symlink inside project pointing to external file
        let symlink_path = temp_dir.path().join("external_link.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&external_file, &symlink_path).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&external_file, &symlink_path).unwrap();
        }
        
        // Should not be able to read external file when follow_symlinks=false
        let mut tool = create_read_tool("external_link.txt");
        tool.follow_symlinks = false;
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot access symlink"));
    }

    #[tokio::test]
    async fn test_read_file_in_symlinked_directory() {
        let temp_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        
        // Create a directory with files outside project
        let external_subdir = external_dir.path().join("subdir");
        async_fs::create_dir(&external_subdir).await.unwrap();
        let external_file = external_subdir.join("file.txt");
        async_fs::write(&external_file, "Content in symlinked dir").await.unwrap();
        
        // Create symlink to the external directory
        let symlink_path = temp_dir.path().join("linked_dir");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&external_subdir, &symlink_path).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&external_subdir, &symlink_path).unwrap();
        }
        
        // Should be able to read file within symlinked directory when follow_symlinks=true
        let mut tool = create_read_tool("linked_dir/file.txt");
        tool.follow_symlinks = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("Content in symlinked dir"));
        
        // Should not be able to read when follow_symlinks=false
        let mut tool = create_read_tool("linked_dir/file.txt");
        tool.follow_symlinks = false;
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        // The error might be about the symlinked directory or about the path being outside
        assert!(error_msg.contains("Cannot access symlink") || error_msg.contains("outside the project directory"));
    }

    // New feature tests
    #[tokio::test]
    async fn test_line_range_single_line() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "range.txt", content).await;
        
        let mut tool = create_read_tool("range.txt");
        tool.line_range = Some("3".to_string());
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     3\tLine 3"));
        assert!(!output.contains("Line 2"));
        assert!(!output.contains("Line 4"));
    }

    #[tokio::test]
    async fn test_line_range_multi_line() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let _file_path = create_test_file(&temp_dir, "range.txt", content).await;
        
        let mut tool = create_read_tool("range.txt");
        tool.line_range = Some("2-4".to_string());
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("     2\tLine 2"));
        assert!(output.contains("     3\tLine 3"));
        assert!(output.contains("     4\tLine 4"));
        assert!(!output.contains("Line 1"));
        assert!(!output.contains("Line 5"));
    }

    #[tokio::test]
    async fn test_invert_match() {
        let temp_dir = TempDir::new().unwrap();
        let content = "apple\nbanana\napricot\nblueberry";
        let _file_path = create_test_file(&temp_dir, "fruits.txt", content).await;
        
        let mut tool = create_read_tool("fruits.txt");
        tool.pattern = Some("^a".to_string());
        tool.invert_match = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        assert!(output.contains("banana"));
        assert!(output.contains("blueberry"));
        assert!(!output.contains("apple"));
        assert!(!output.contains("apricot"));
    }

    #[tokio::test]
    async fn test_context_lines() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Line 1\nLine 2\nERROR here\nLine 4\nLine 5\nLine 6\nERROR again\nLine 8\nLine 9";
        let _file_path = create_test_file(&temp_dir, "log.txt", content).await;
        
        let mut tool = create_read_tool("log.txt");
        tool.pattern = Some("ERROR".to_string());
        tool.context_before = 1;
        tool.context_after = 1;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should include context around ERROR lines
        assert!(output.contains("Line 2"));      // context before first ERROR
        assert!(output.contains("ERROR here"));   // first match
        assert!(output.contains("Line 4"));       // context after first ERROR
        assert!(output.contains("Line 6"));       // context before second ERROR
        assert!(output.contains("ERROR again"));  // second match
        assert!(output.contains("Line 8"));       // context after second ERROR
        assert!(!output.contains("Line 1"));      // not in context
        assert!(!output.contains("Line 9"));      // not in context
    }

    #[tokio::test]
    async fn test_preview_mode() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Test file content\nWith multiple lines";
        let _file_path = create_test_file(&temp_dir, "preview.txt", content).await;
        
        let mut tool = create_read_tool("preview.txt");
        tool.preview_only = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should return JSON metadata
        assert!(output.contains("\"size\""));
        assert!(output.contains("\"size_human\""));
        assert!(output.contains("\"modified\""));
        assert!(output.contains("\"lines\""));
        assert!(output.contains("\"encoding\""));
        assert!(output.contains("\"has_bom\""));
        assert!(output.contains("\"is_binary\""));
        assert!(!output.contains("Test file content")); // Should not include actual content
    }

    #[tokio::test]
    async fn test_include_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Test content";
        let _file_path = create_test_file(&temp_dir, "meta.txt", content).await;
        
        let mut tool = create_read_tool("meta.txt");
        tool.include_metadata = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        // Should include both content and metadata
        assert!(output.contains("\"content\""));
        assert!(output.contains("Test content"));
        assert!(output.contains("\"metadata\""));
        assert!(output.contains("\"size\""));
    }

    #[tokio::test]
    async fn test_bom_detection() {
        let temp_dir = TempDir::new().unwrap();
        // UTF-8 BOM + content
        let bom_content = b"\xEF\xBB\xBFTest with BOM";
        let file_path = temp_dir.path().join("bom.txt");
        async_fs::write(&file_path, bom_content).await.unwrap();
        
        let mut tool = create_read_tool("bom.txt");
        tool.preview_only = true;
        let result = test_read_tool_in_dir(&temp_dir, tool).await.unwrap();
        
        let output = match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => &text.text,
            _ => panic!("Expected text content"),
        };
        
        let metadata: serde_json::Value = serde_json::from_str(output).unwrap();
        assert_eq!(metadata["has_bom"], true);
    }

    #[tokio::test]
    async fn test_invalid_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let content = "Test content";
        let _file_path = create_test_file(&temp_dir, "test.txt", content).await;
        
        let mut tool = create_read_tool("test.txt");
        tool.line_range = Some("5-3".to_string()); // Invalid: start > end
        let result = test_read_tool_in_dir(&temp_dir, tool).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid line range"));
    }
}