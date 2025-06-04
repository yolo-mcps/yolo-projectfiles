use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
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

const TOOL_NAME: &str = "read";

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

Features:
- Line numbers by default (format: line_number<tab>content)
- Partial reading with offset/limit
- Pattern filtering with regex
- Tail mode for reading from end
- Binary file detection
- Symlink support with configurable behavior

Parameters:
- path: File path (required)
- offset: Start line (1-indexed, default: 0 for beginning)
- limit: Max lines to read (0 for all, default: 0)
- linenumbers: Show line numbers (default: true, set to false when reading for edit)
- tail: Read from end (default: false)
- pattern: Regex to filter lines
- case: For pattern matching (\"sensitive\" or \"insensitive\", default: \"sensitive\")
- encoding: Text encoding (default: \"utf-8\")
- follow_symlinks: Follow symlinks to read files outside project directory (default: true). When false, symlinks cannot be accessed.

NOTE: Omit optional parameters when not needed, don't pass null.
HINT: Use linenumbers:false when reading files you plan to edit.")]
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
    /// Perform binary file detection if true (default: true)
    #[serde(default = "default_binary_check")]
    pub binary_check: bool,
    /// Read from the end of the file (tail mode). If true, offset is from end (default: false)
    #[serde(default)]
    pub tail: bool,
    /// Pattern to filter lines (regex). Only lines matching this pattern will be returned
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
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

        // Binary file detection (unless skipped)
        if self.binary_check {
            let mut file = tokio::fs::File::open(&canonical_path).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
            
            let file_size = file.metadata().await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?
                .len() as usize;
            
            let sample_size = 8192.min(file_size);
            let mut buffer = vec![0; sample_size];
            
            use tokio::io::AsyncReadExt;
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
            binary_check: true,
            tail: false,
            pattern: None,
            case: "sensitive".to_string(),
            encoding: "utf-8".to_string(),
            linenumbers: true,
            follow_symlinks: true,
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
}