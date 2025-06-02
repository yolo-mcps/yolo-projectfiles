use crate::config::tool_errors;
use crate::tools::utils::{format_count, format_path};
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;

const TOOL_NAME: &str = "wc";

#[mcp_tool(
    name = "wc",
    description = "Counts lines, words, characters, and bytes in text files. Similar to the Unix 'wc' command."
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
}

fn default_true() -> bool {
    true
}

impl WcTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Get current directory and resolve path
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
        let target_path = current_dir.join(&self.path);
        
        // Security check - ensure path is within project directory
        let normalized_path = target_path
            .canonicalize()
            .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.path)))?;
            
        if !normalized_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory"
            )));
        }
        
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
        let relative_path = normalized_path.strip_prefix(&current_dir)
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