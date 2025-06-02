use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;

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
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        let target_path = current_dir.join(&self.path);
        
        // Security check - ensure path is within project directory
        let normalized_path = target_path
            .canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path: {}", e)))?;
            
        if !normalized_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }
        
        // Check if file exists
        if !normalized_path.exists() {
            return Err(CallToolError::unknown_tool(format!(
                "File '{}' does not exist",
                self.path
            )));
        }
        
        if !normalized_path.is_file() {
            return Err(CallToolError::unknown_tool(format!(
                "Path '{}' is not a file",
                self.path
            )));
        }
        
        // Read file contents
        let contents = fs::read_to_string(&normalized_path).await
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to read file: {}", e)))?;
        
        // Get byte count if requested
        let byte_count = if self.count_bytes {
            fs::metadata(&normalized_path).await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to get file metadata: {}", e)))?
                .len()
        } else {
            0
        };
        
        // Perform counts
        let mut result = serde_json::Map::new();
        result.insert("path".to_string(), serde_json::Value::String(self.path.clone()));
        
        if self.count_lines {
            let line_count = contents.lines().count();
            result.insert("lines".to_string(), serde_json::Value::Number(line_count.into()));
        }
        
        if self.count_words {
            let word_count = count_words(&contents);
            result.insert("words".to_string(), serde_json::Value::Number(word_count.into()));
        }
        
        if self.count_chars {
            let char_count = contents.chars().count();
            result.insert("characters".to_string(), serde_json::Value::Number(char_count.into()));
        }
        
        if self.count_bytes {
            result.insert("bytes".to_string(), serde_json::Value::Number(byte_count.into()));
        }
        
        // Format output similar to wc command
        let mut summary_parts = Vec::new();
        if self.count_lines {
            if let Some(lines) = result.get("lines") {
                summary_parts.push(format!("{} lines", lines));
            }
        }
        if self.count_words {
            if let Some(words) = result.get("words") {
                summary_parts.push(format!("{} words", words));
            }
        }
        if self.count_chars {
            if let Some(chars) = result.get("characters") {
                summary_parts.push(format!("{} characters", chars));
            }
        }
        if self.count_bytes {
            if let Some(bytes) = result.get("bytes") {
                summary_parts.push(format!("{} bytes", bytes));
            }
        }
        
        result.insert("summary".to_string(), serde_json::Value::String(summary_parts.join(", ")));
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result)
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to serialize result: {}", e)))?,
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