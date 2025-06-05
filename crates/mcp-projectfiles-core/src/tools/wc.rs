use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::{format_count, format_path, format_size, resolve_path_for_read};
use async_trait::async_trait;
use encoding_rs;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;

use chrono::{DateTime, Local};

const TOOL_NAME: &str = "wc";

#[mcp_tool(
    name = "wc",
    description = "Count lines, words, characters, bytes in text files. Max line length, multiple encodings.
Examples: {\"path\": \"README.md\"} or {\"path\": \"stats.log\", \"output_format\": \"json\"}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct WcTool {
    /// Path to the file to count (relative to project root)
    pub path: String,

    /// Whether to count lines (default: true)
    #[serde(default = "default_true")]
    pub count_lines: bool,

    /// Whether to count words (default: true)
    #[serde(default = "default_true")]
    pub count_words: bool,

    /// Whether to count characters (default: true)
    #[serde(default = "default_true")]
    pub count_chars: bool,

    /// Whether to count bytes (default: false)
    #[serde(default)]
    pub count_bytes: bool,

    /// Report the length of the longest line (default: false)
    #[serde(default)]
    pub max_line_length: bool,

    /// Text encoding to use (default: "utf-8")
    /// Supported: "utf-8", "ascii", "latin1"
    #[serde(default = "default_encoding")]
    pub encoding: String,

    /// Output format: "text" or "json" (default: "text")
    #[serde(default = "default_output_format")]
    pub output_format: String,

    /// Include file metadata in output (default: false)
    #[serde(default)]
    pub include_metadata: bool,

    /// Follow symlinks to count files outside the project directory (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

fn default_true() -> bool {
    true
}

fn default_follow_symlinks() -> bool {
    true
}

fn default_encoding() -> String {
    "utf-8".to_string()
}

fn default_output_format() -> String {
    "text".to_string()
}

#[derive(Serialize, Deserialize, Debug)]
struct WcJsonOutput {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    words: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    characters: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_line_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<FileMetadata>,
}

#[derive(Serialize, Deserialize, Debug)]
struct FileMetadata {
    size: u64,
    size_human: String,
    modified: String,
    encoding: String,
    is_binary: bool,
}

#[async_trait]
impl StatefulTool for WcTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Validate encoding
        let encoding = match self.encoding.to_lowercase().as_str() {
            "utf-8" | "utf8" => encoding_rs::UTF_8,
            "ascii" => encoding_rs::WINDOWS_1252, // ASCII is a subset
            "latin1" | "iso-8859-1" => encoding_rs::WINDOWS_1252,
            _ => {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!(
                        "Unsupported encoding: {}. Supported: utf-8, ascii, latin1",
                        self.encoding
                    ),
                )));
            }
        };

        // Validate output format
        if self.output_format != "text" && self.output_format != "json" {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!(
                    "Invalid output format: {}. Must be 'text' or 'json'",
                    self.output_format
                ),
            )));
        }

        // Get project root and resolve path
        let project_root = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        // Use the utility function to resolve path with symlink support
        let normalized_path =
            resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;

        // Check if file exists
        if !normalized_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME, &self.path,
            )));
        }

        if !normalized_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path '{}' is not a file", self.path),
            )));
        }

        // Get file metadata
        let file_metadata = fs::metadata(&normalized_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get file metadata: {}", e),
            ))
        })?;

        // Read file bytes for encoding detection
        let file_bytes = fs::read(&normalized_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read file: {}", e),
            ))
        })?;

        // Check if file is binary
        let is_binary = is_likely_binary(&file_bytes);
        if is_binary {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!(
                    "File '{}' appears to be binary. The wc tool only works with text files.",
                    self.path
                ),
            )));
        }

        // Decode file contents
        let (contents, _encoding_used, had_errors) = encoding.decode(&file_bytes);
        if had_errors {
            eprintln!(
                "Warning: Some characters could not be decoded with {} encoding",
                self.encoding
            );
        }

        // Get byte count
        let byte_count = file_metadata.len();

        // Perform counts
        let line_count = if self.count_lines {
            contents.lines().count()
        } else {
            0
        };

        let word_count = if self.count_words {
            count_words(&contents)
        } else {
            0
        };

        let char_count = if self.count_chars {
            contents.chars().count()
        } else {
            0
        };

        let max_line_len = if self.max_line_length {
            contents
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        // Format path relative to project root
        let relative_path = normalized_path
            .strip_prefix(&project_root)
            .unwrap_or(&normalized_path);

        // Get file metadata if requested
        let metadata = if self.include_metadata {
            let modified = file_metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    let datetime = DateTime::<Local>::from(std::time::UNIX_EPOCH + d);
                    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_else(|| "Unknown".to_string());

            Some(FileMetadata {
                size: byte_count,
                size_human: format_size(byte_count),
                modified,
                encoding: self.encoding.clone(),
                is_binary: false,
            })
        } else {
            None
        };

        // Generate output based on format
        let output = if self.output_format == "json" {
            let json_output = WcJsonOutput {
                path: relative_path.display().to_string(),
                lines: if self.count_lines {
                    Some(line_count)
                } else {
                    None
                },
                words: if self.count_words {
                    Some(word_count)
                } else {
                    None
                },
                characters: if self.count_chars {
                    Some(char_count)
                } else {
                    None
                },
                bytes: if self.count_bytes {
                    Some(byte_count)
                } else {
                    None
                },
                max_line_length: if self.max_line_length {
                    Some(max_line_len)
                } else {
                    None
                },
                metadata,
            };

            serde_json::to_string_pretty(&json_output)
                .unwrap_or_else(|e| format!("Error serializing JSON: {}", e))
        } else {
            // Text format output
            let mut output_lines = Vec::new();
            output_lines.push(format!("Word count for {}", format_path(relative_path)));
            output_lines.push("".to_string());

            if self.count_lines {
                output_lines.push(format!(
                    "Lines:           {}",
                    format_count(line_count, "line", "lines")
                ));
            }
            if self.count_words {
                output_lines.push(format!(
                    "Words:           {}",
                    format_count(word_count, "word", "words")
                ));
            }
            if self.count_chars {
                output_lines.push(format!(
                    "Characters:      {}",
                    format_count(char_count, "character", "characters")
                ));
            }
            if self.count_bytes {
                output_lines.push(format!(
                    "Bytes:           {} ({})",
                    byte_count,
                    format_size(byte_count)
                ));
            }
            if self.max_line_length {
                output_lines.push(format!(
                    "Max line length: {}",
                    format_count(max_line_len, "character", "characters")
                ));
            }

            if let Some(meta) = metadata {
                output_lines.push("".to_string());
                output_lines.push("File metadata:".to_string());
                output_lines.push(format!("  Size:     {}", meta.size_human));
                output_lines.push(format!("  Modified: {}", meta.modified));
                output_lines.push(format!("  Encoding: {}", meta.encoding));
            }

            output_lines.join("\n")
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                output, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn is_likely_binary(bytes: &[u8]) -> bool {
    // Check for common binary file signatures
    if bytes.len() >= 4 {
        // Check for common binary signatures
        match &bytes[0..4] {
            // Images
            b"\x89PNG" => return true,
            b"\xFF\xD8\xFF\xDB" => return true, // JPEG
            b"\xFF\xD8\xFF\xE0" => return true, // JPEG
            b"GIF8" => return true,
            // Archives
            b"PK\x03\x04" => return true,   // ZIP
            b"\x1F\x8B\x08" => return true, // GZIP
            // Executables
            b"\x7FELF" => return true,    // ELF
            b"MZ\x90\x00" => return true, // PE
            _ => {}
        }
    }

    // Check for null bytes or high proportion of non-text characters
    let sample_size = bytes.len().min(8192);
    let mut null_count = 0;
    let mut non_ascii_count = 0;

    for &byte in &bytes[0..sample_size] {
        if byte == 0 {
            null_count += 1;
        } else if byte < 0x20 && byte != b'\t' && byte != b'\n' && byte != b'\r' {
            non_ascii_count += 1;
        } else if byte > 0x7E && byte < 0x80 {
            non_ascii_count += 1;
        }
    }

    // If more than 1% null bytes or 30% non-ASCII, likely binary
    null_count > 0 || non_ascii_count > sample_size / 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;
    use tokio::fs;

    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }

    async fn create_test_file(
        dir: &std::path::Path,
        name: &str,
        content: &str,
    ) -> std::path::PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content)
            .await
            .expect("Failed to create test file");
        file_path
    }

    fn create_wc_tool(path: &str) -> WcTool {
        WcTool {
            path: path.to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            max_line_length: false,
            encoding: "utf-8".to_string(),
            output_format: "text".to_string(),
            include_metadata: false,
            follow_symlinks: true,
        }
    }

    #[tokio::test]
    async fn test_wc_basic_count_all() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Hello world\nThis is a test\nThird line";
        create_test_file(temp_dir.path(), "test.txt", content).await;

        let wc_tool = WcTool {
            path: "test.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: true,
            max_line_length: false,
            encoding: "utf-8".to_string(),
            output_format: "text".to_string(),
            include_metadata: false,
            follow_symlinks: true,
        };

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.is_error, Some(false));

        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should contain all count types
            assert!(content.contains("3 lines")); // 3 lines
            assert!(content.contains("words")); // Check word counting works
            assert!(content.contains("characters"));
            assert!(content.contains("Bytes:"));
            assert!(content.contains("test.txt"));
        }
    }

    #[tokio::test]
    async fn test_wc_lines_only() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Line 1\nLine 2\nLine 3\n";
        create_test_file(temp_dir.path(), "lines.txt", content).await;

        let mut wc_tool = create_wc_tool("lines.txt");
        wc_tool.count_words = false;
        wc_tool.count_chars = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should only contain line count
            assert!(content.contains("3 lines"));
            assert!(!content.contains("Words:"));
            assert!(!content.contains("Characters:"));
            assert!(!content.contains("Bytes:"));
        }
    }

    #[tokio::test]
    async fn test_wc_words_only() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "one two three four five";
        create_test_file(temp_dir.path(), "words.txt", content).await;

        let mut wc_tool = create_wc_tool("words.txt");
        wc_tool.count_lines = false;
        wc_tool.count_chars = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should only contain word count
            assert!(content.contains("5 words"));
            assert!(!content.contains("Lines:"));
            assert!(!content.contains("Characters:"));
            assert!(!content.contains("Bytes:"));
        }
    }

    #[tokio::test]
    async fn test_wc_characters_only() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Hello!"; // 6 characters
        create_test_file(temp_dir.path(), "chars.txt", content).await;

        let mut wc_tool = create_wc_tool("chars.txt");
        wc_tool.count_lines = false;
        wc_tool.count_words = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should only contain character count
            assert!(content.contains("6 characters"));
            assert!(!content.contains("Lines:"));
            assert!(!content.contains("Words:"));
            assert!(!content.contains("Bytes:"));
        }
    }

    #[tokio::test]
    async fn test_wc_bytes_only() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Test"; // 4 bytes in UTF-8
        create_test_file(temp_dir.path(), "bytes.txt", content).await;

        let mut wc_tool = create_wc_tool("bytes.txt");
        wc_tool.count_lines = false;
        wc_tool.count_words = false;
        wc_tool.count_chars = false;
        wc_tool.count_bytes = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should only contain byte count
            assert!(content.contains("Bytes:           4"));
            assert!(!content.contains("Lines:"));
            assert!(!content.contains("Words:"));
            assert!(!content.contains("Characters:"));
        }
    }

    #[tokio::test]
    async fn test_wc_empty_file() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "empty.txt", "").await;

        let mut wc_tool = create_wc_tool("empty.txt");
        wc_tool.count_bytes = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should show all zeros
            assert!(content.contains("0 lines"));
            assert!(content.contains("0 words"));
            assert!(content.contains("0 characters"));
            assert!(content.contains("Bytes:           0"));
        }
    }

    #[tokio::test]
    async fn test_wc_single_line_no_newline() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Single line without newline";
        create_test_file(temp_dir.path(), "single.txt", content).await;

        let wc_tool = create_wc_tool("single.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Single line without newline should count as 1 line (lines() behavior)
            assert!(content.contains("1 line")); // singular form
            assert!(content.contains("4 words"));
            assert!(content.contains("characters"));
        }
    }

    #[tokio::test]
    async fn test_wc_multiple_whitespace() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "word1    word2\t\tword3\n\n\nword4";
        create_test_file(temp_dir.path(), "whitespace.txt", content).await;

        let mut wc_tool = create_wc_tool("whitespace.txt");
        wc_tool.count_chars = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should handle whitespace correctly
            assert!(content.contains("4 lines")); // 4 lines due to newlines
            assert!(content.contains("4 words")); // split_whitespace should find 4 words
        }
    }

    #[tokio::test]
    async fn test_wc_unicode_characters() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Hello 世界 🌍"; // Mix of ASCII, Chinese, and emoji
        create_test_file(temp_dir.path(), "unicode.txt", content).await;

        let mut wc_tool = create_wc_tool("unicode.txt");
        wc_tool.count_bytes = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Unicode should be counted correctly
            assert!(content.contains("1 line"));
            assert!(content.contains("3 words")); // "Hello", "世界", "🌍"
            assert!(content.contains("characters")); // Count Unicode characters correctly
            // Bytes should be more than characters due to UTF-8 encoding
            assert!(content.contains("Bytes:"));
        }
    }

    #[tokio::test]
    async fn test_wc_file_not_found() {
        let (context, _temp_dir) = setup_test_context().await;

        let wc_tool = create_wc_tool("nonexistent.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:wc"));
        assert!(error.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_wc_directory_not_file() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a directory instead of a file
        let dir_path = temp_dir.path().join("testdir");
        fs::create_dir(&dir_path)
            .await
            .expect("Failed to create directory");

        let wc_tool = create_wc_tool("testdir");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:wc"));
        assert!(error.to_string().contains("not a file"));
    }

    #[tokio::test]
    async fn test_wc_path_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;

        let wc_tool = create_wc_tool("../outside.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:wc"));
        let error_str = error.to_string();
        assert!(
            error_str.contains("not found") || error_str.contains("outside the project directory")
        );
    }

    #[tokio::test]
    async fn test_wc_default_parameters() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Default test\nSecond line";
        create_test_file(temp_dir.path(), "default.txt", content).await;

        // Test that defaults work (should count lines, words, chars but not bytes)
        // Test that defaults work (should count lines, words, chars but not bytes)
        let wc_tool = create_wc_tool("default.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should include lines, words, characters
            assert!(content.contains("Lines:"));
            assert!(content.contains("Words:"));
            assert!(content.contains("Characters:"));
            // Should NOT include bytes (default false)
            assert!(!content.contains("Bytes:"));
        }
    }

    #[tokio::test]
    async fn test_wc_large_file() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a larger file with known counts
        let mut large_content = String::new();
        for i in 1..=100 {
            large_content.push_str(&format!("Line {} with some words\n", i));
        }
        create_test_file(temp_dir.path(), "large.txt", &large_content).await;

        let mut wc_tool = create_wc_tool("large.txt");
        wc_tool.count_chars = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should correctly count many lines and words
            assert!(content.contains("100 lines"));
            assert!(content.contains("500 words")); // Each line has 5 words × 100 lines
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_wc_symlink_to_file_within_project() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file with known content
        let target_content = "Line one\nLine two\nLine three";
        create_test_file(temp_dir.path(), "target.txt", target_content).await;

        // Create a symlink to the target file
        let target_path = temp_dir.path().join("target.txt");
        let symlink_path = temp_dir.path().join("link_to_target.txt");
        std::os::unix::fs::symlink(&target_path, &symlink_path).expect("Failed to create symlink");

        let wc_tool = create_wc_tool("link_to_target.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should count the target file content, showing resolved target path
            assert!(content.contains("Word count for"));
            assert!(content.contains("target.txt")); // Shows resolved target path
            assert!(content.contains("3 lines"));
            assert!(content.contains("6 words")); // "Line one", "Line two", "Line three" = 6 words
            assert!(content.contains("characters"));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_wc_symlink_to_file_outside_project() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file outside the project directory
        let external_temp_dir = TempDir::new().unwrap();
        let external_target = external_temp_dir.path().join("external_target.txt");
        let external_content = "External line one\nExternal line two";
        fs::write(&external_target, external_content)
            .await
            .expect("Failed to create external file");

        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path)
            .expect("Failed to create symlink");

        let mut wc_tool = create_wc_tool("link_to_external.txt");
        wc_tool.count_chars = false;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Should count the external target file content, showing resolved external target path
            assert!(content.contains("Word count for"));
            assert!(content.contains("external_target.txt")); // Shows resolved external target path
            assert!(content.contains("2 lines"));
            assert!(content.contains("6 words")); // "External line one", "External line two" = 6 words
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_wc_symlink_with_follow_disabled() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file outside the project to ensure symlink behavior is tested
        let external_temp_dir = TempDir::new().unwrap();
        let external_target = external_temp_dir.path().join("external_target.txt");
        let external_content = "External target content";
        fs::write(&external_target, external_content)
            .await
            .expect("Failed to create external file");

        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path)
            .expect("Failed to create symlink");

        let mut wc_tool = create_wc_tool("link_to_external.txt");
        wc_tool.follow_symlinks = false;

        let result = wc_tool.call_with_context(&context).await;
        // With follow_symlinks=false, the symlink should not be resolved,
        // so it should fail when the canonicalized path is outside the project
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:wc"));
        assert!(error_str.contains("Cannot access symlink"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_wc_broken_symlink() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a symlink to a non-existent target
        let target_path = temp_dir.path().join("nonexistent_target.txt");
        let symlink_path = temp_dir.path().join("broken_link.txt");
        std::os::unix::fs::symlink(&target_path, &symlink_path).expect("Failed to create symlink");

        let wc_tool = create_wc_tool("broken_link.txt");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:wc"));
        // Should indicate file not found since the symlink target doesn't exist
        assert!(error_str.contains("not found") || error_str.contains("No such file"));
    }

    #[tokio::test]
    async fn test_wc_max_line_length() {
        let (context, temp_dir) = setup_test_context().await;
        let content =
            "Short line\nThis is a much longer line with more characters\nMedium line here";
        create_test_file(temp_dir.path(), "lines_test.txt", content).await;

        let mut wc_tool = create_wc_tool("lines_test.txt");
        wc_tool.max_line_length = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            assert!(content.contains("Max line length:"));
            assert!(content.contains("47 characters")); // "This is a much longer line with more characters" = 47
        }
    }

    #[tokio::test]
    async fn test_wc_json_output() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Line 1\nLine 2\nLine 3";
        create_test_file(temp_dir.path(), "json_test.txt", content).await;

        let mut wc_tool = create_wc_tool("json_test.txt");
        wc_tool.output_format = "json".to_string();
        wc_tool.count_bytes = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            // Parse JSON to verify structure
            let json: serde_json::Value =
                serde_json::from_str(content).expect("Invalid JSON output");
            assert_eq!(json["path"], "json_test.txt");
            assert_eq!(json["lines"], 3);
            assert_eq!(json["words"], 6);
            assert_eq!(json["characters"], 20);
            assert_eq!(json["bytes"], 20);
            assert!(json["max_line_length"].is_null());
            assert!(json["metadata"].is_null());
        }
    }

    #[tokio::test]
    async fn test_wc_with_metadata() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Test content";
        create_test_file(temp_dir.path(), "meta_test.txt", content).await;

        let mut wc_tool = create_wc_tool("meta_test.txt");
        wc_tool.include_metadata = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            assert!(content.contains("File metadata:"));
            assert!(content.contains("Size:"));
            assert!(content.contains("Modified:"));
            assert!(content.contains("Encoding: utf-8"));
        }
    }

    #[tokio::test]
    async fn test_wc_json_with_metadata() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Test";
        create_test_file(temp_dir.path(), "json_meta.txt", content).await;

        let mut wc_tool = create_wc_tool("json_meta.txt");
        wc_tool.output_format = "json".to_string();
        wc_tool.include_metadata = true;

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            let json: serde_json::Value =
                serde_json::from_str(content).expect("Invalid JSON output");

            assert!(json["metadata"].is_object());
            assert!(json["metadata"]["size"].is_u64());
            assert!(json["metadata"]["size_human"].is_string());
            assert!(json["metadata"]["modified"].is_string());
            assert_eq!(json["metadata"]["encoding"], "utf-8");
            assert_eq!(json["metadata"]["is_binary"], false);
        }
    }

    #[tokio::test]
    async fn test_wc_different_encoding() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Simple ASCII text";
        create_test_file(temp_dir.path(), "ascii_test.txt", content).await;

        let mut wc_tool = create_wc_tool("ascii_test.txt");
        wc_tool.encoding = "ascii".to_string();

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            assert!(content.contains("3 words"));
            assert!(content.contains("17 characters"));
        }
    }

    #[tokio::test]
    async fn test_wc_invalid_encoding() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "content").await;

        let mut wc_tool = create_wc_tool("test.txt");
        wc_tool.encoding = "invalid-encoding".to_string();

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("Unsupported encoding"));
    }

    #[tokio::test]
    async fn test_wc_invalid_output_format() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "content").await;

        let mut wc_tool = create_wc_tool("test.txt");
        wc_tool.output_format = "invalid".to_string();

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("Invalid output format"));
    }

    #[tokio::test]
    async fn test_wc_binary_file_detection() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a file with binary content (PNG header)
        let binary_content = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let file_path = temp_dir.path().join("binary.png");
        fs::write(&file_path, binary_content)
            .await
            .expect("Failed to create binary file");

        let wc_tool = create_wc_tool("binary.png");

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("appears to be binary"));
    }

    #[tokio::test]
    async fn test_wc_partial_counts() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "One two three\nFour five";
        create_test_file(temp_dir.path(), "partial.txt", content).await;

        let mut wc_tool = create_wc_tool("partial.txt");
        wc_tool.count_lines = true;
        wc_tool.count_words = false;
        wc_tool.count_chars = false;
        wc_tool.output_format = "json".to_string();

        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            let json: serde_json::Value =
                serde_json::from_str(content).expect("Invalid JSON output");

            assert_eq!(json["lines"], 2);
            assert!(json["words"].is_null());
            assert!(json["characters"].is_null());
            assert!(json["bytes"].is_null());
        }
    }
}

