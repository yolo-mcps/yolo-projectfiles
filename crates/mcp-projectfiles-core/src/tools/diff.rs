use crate::config::tool_errors;
use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use similar::{ChangeTag, TextDiff};

const TOOL_NAME: &str = "diff";

#[mcp_tool(
    name = "diff",
    description = "Compares two files within the project directory and shows differences in unified diff format. Replaces the need for 'git diff' or other diff commands."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct DiffTool {
    /// First file to compare (relative to project root)
    pub file1: String,
    
    /// Second file to compare (relative to project root)
    pub file2: String,
    
    /// Number of context lines to show around changes (default: 3)
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
    
    /// Whether to ignore whitespace differences (default: false)
    #[serde(default)]
    pub ignore_whitespace: bool,
}

fn default_context_lines() -> u32 {
    3
}

impl DiffTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
        // Validate and canonicalize file1
        let file1_path = if Path::new(&self.file1).is_absolute() {
            Path::new(&self.file1).to_path_buf()
        } else {
            current_dir.join(&self.file1)
        };
        
        let canonical_file1 = file1_path.canonicalize()
            .map_err(|_| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.file1)))?;
        
        if !canonical_file1.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.file1,
                "File1 is outside the project directory"
            )));
        }
        
        // Validate and canonicalize file2
        let file2_path = if Path::new(&self.file2).is_absolute() {
            Path::new(&self.file2).to_path_buf()
        } else {
            current_dir.join(&self.file2)
        };
        
        let canonical_file2 = file2_path.canonicalize()
            .map_err(|_| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.file2)))?;
        
        if !canonical_file2.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.file2,
                "File2 is outside the project directory"
            )));
        }
        
        // Read both files
        let content1 = fs::read_to_string(&canonical_file1).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file1 '{}': {}", self.file1, e))))?;
        
        let content2 = fs::read_to_string(&canonical_file2).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file2 '{}': {}", self.file2, e))))?;
        
        // Process content if ignoring whitespace
        let (text1, text2) = if self.ignore_whitespace {
            (normalize_whitespace(&content1), normalize_whitespace(&content2))
        } else {
            (content1, content2)
        };
        
        // Create the diff
        let diff = TextDiff::from_lines(&text1, &text2);
        
        // Generate unified diff format
        let mut output = String::new();
        
        // Add header
        output.push_str(&format!("--- {}\n", self.file1));
        output.push_str(&format!("+++ {}\n", self.file2));
        
        // Generate hunks with context
        for hunk in diff.unified_diff().context_radius(self.context_lines as usize).iter_hunks() {
            output.push_str(&hunk.to_string());
        }
        
        // If files are identical
        if output.lines().count() <= 2 {
            output.push_str("\nFiles are identical\n");
        }
        
        // Also provide a summary
        let mut stats = DiffStats::default();
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => stats.additions += 1,
                ChangeTag::Delete => stats.deletions += 1,
                ChangeTag::Equal => stats.unchanged += 1,
            }
        }
        
        let summary = format!(
            "\n--- Summary ---\n{} additions(+), {} deletions(-), {} unchanged lines\n",
            stats.additions, stats.deletions, stats.unchanged
        );
        
        output.push_str(&summary);
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                output,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[derive(Default)]
struct DiffStats {
    additions: usize,
    deletions: usize,
    unchanged: usize,
}

/// Normalize whitespace for comparison
fn normalize_whitespace(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}