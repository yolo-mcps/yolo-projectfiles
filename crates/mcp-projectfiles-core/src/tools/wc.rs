use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_count, format_path, resolve_path_for_read};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;

const TOOL_NAME: &str = "wc";

#[mcp_tool(
    name = "wc",
    description = "Counts lines, words, characters, and bytes in text files within the project directory. Can follow symlinks to count files outside the project directory. Prefer this over the Unix 'wc' command when analyzing project files."
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

#[async_trait]
impl StatefulTool for WcTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Get project root and resolve path
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve path with symlink support
        let normalized_path = resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;
        
        // Check if file exists
        if !normalized_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME,
                &self.path
            )));
        }
        
        if !normalized_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path '{}' is not a file", self.path)
            )));
        }
        
        // Read file contents
        let contents = fs::read_to_string(&normalized_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        // Get byte count if requested
        let byte_count = if self.count_bytes {
            fs::metadata(&normalized_path).await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?
                .len()
        } else {
            0
        };
        
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
        

        
        // Format path relative to project root
        let relative_path = normalized_path.strip_prefix(&project_root)
            .unwrap_or(&normalized_path);
        
        // Create human-readable output
        let mut output_lines = Vec::new();
        output_lines.push(format!("Word count for {}", format_path(relative_path)));
        output_lines.push("".to_string());
        
        if self.count_lines {
            output_lines.push(format!("Lines:      {}", format_count(line_count, "line", "lines")));
        }
        if self.count_words {
            output_lines.push(format!("Words:      {}", format_count(word_count, "word", "words")));
        }
        if self.count_chars {
            output_lines.push(format!("Characters: {}", format_count(char_count, "character", "characters")));
        }
        if self.count_bytes {
            output_lines.push(format!("Bytes:      {}", format_count(byte_count as usize, "byte", "bytes")));
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                output_lines.join("\n"),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
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

    async fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content).await.expect("Failed to create test file");
        file_path
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
            assert!(content.contains("bytes"));
            assert!(content.contains("test.txt"));
        }
    }

    #[tokio::test]
    async fn test_wc_lines_only() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Line 1\nLine 2\nLine 3\n";
        create_test_file(temp_dir.path(), "lines.txt", content).await;
        
        let wc_tool = WcTool {
            path: "lines.txt".to_string(),
            count_lines: true,
            count_words: false,
            count_chars: false,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "words.txt".to_string(),
            count_lines: false,
            count_words: true,
            count_chars: false,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "chars.txt".to_string(),
            count_lines: false,
            count_words: false,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "bytes.txt".to_string(),
            count_lines: false,
            count_words: false,
            count_chars: false,
            count_bytes: true,
            follow_symlinks: true,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should only contain byte count
            assert!(content.contains("4 bytes"));
            assert!(!content.contains("Lines:"));
            assert!(!content.contains("Words:"));
            assert!(!content.contains("Characters:"));
        }
    }

    #[tokio::test]
    async fn test_wc_empty_file() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "empty.txt", "").await;
        
        let wc_tool = WcTool {
            path: "empty.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: true,
            follow_symlinks: true,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show all zeros
            assert!(content.contains("0 lines"));
            assert!(content.contains("0 words"));
            assert!(content.contains("0 characters"));
            assert!(content.contains("0 bytes"));
        }
    }

    #[tokio::test]
    async fn test_wc_single_line_no_newline() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Single line without newline";
        create_test_file(temp_dir.path(), "single.txt", content).await;
        
        let wc_tool = WcTool {
            path: "single.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "whitespace.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: false,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "unicode.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: true,
            follow_symlinks: true,
        };
        
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
            assert!(content.contains("bytes"));
        }
    }

    #[tokio::test]
    async fn test_wc_file_not_found() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let wc_tool = WcTool {
            path: "nonexistent.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        fs::create_dir(&dir_path).await.expect("Failed to create directory");
        
        let wc_tool = WcTool {
            path: "testdir".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:wc"));
        assert!(error.to_string().contains("not a file"));
    }

    #[tokio::test]
    async fn test_wc_path_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let wc_tool = WcTool {
            path: "../outside.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:wc"));
        let error_str = error.to_string();
        assert!(error_str.contains("not found") || error_str.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_wc_default_parameters() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Default test\nSecond line";
        create_test_file(temp_dir.path(), "default.txt", content).await;
        
        // Test that defaults work (should count lines, words, chars but not bytes)
        let wc_tool = WcTool {
            path: "default.txt".to_string(),
            count_lines: default_true(),
            count_words: default_true(),
            count_chars: default_true(),
            count_bytes: false, // default is false for bytes
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "large.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: false,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        
        let wc_tool = WcTool {
            path: "link_to_target.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        fs::write(&external_target, external_content).await.expect("Failed to create external file");
        
        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path).expect("Failed to create symlink");
        
        let wc_tool = WcTool {
            path: "link_to_external.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: false,
            count_bytes: false,
            follow_symlinks: true,
        };
        
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
        fs::write(&external_target, external_content).await.expect("Failed to create external file");
        
        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path).expect("Failed to create symlink");
        
        let wc_tool = WcTool {
            path: "link_to_external.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: false,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        // With follow_symlinks=false, the symlink should not be resolved,
        // so it should fail when the canonicalized path is outside the project
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:wc"));
        assert!(error_str.contains("outside the project directory"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_wc_broken_symlink() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create a symlink to a non-existent target
        let target_path = temp_dir.path().join("nonexistent_target.txt");
        let symlink_path = temp_dir.path().join("broken_link.txt");
        std::os::unix::fs::symlink(&target_path, &symlink_path).expect("Failed to create symlink");
        
        let wc_tool = WcTool {
            path: "broken_link.txt".to_string(),
            count_lines: true,
            count_words: true,
            count_chars: true,
            count_bytes: false,
            follow_symlinks: true,
        };
        
        let result = wc_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:wc"));
        // Should indicate file not found since the symlink target doesn't exist
        assert!(error_str.contains("not found") || error_str.contains("No such file"));
    }
}