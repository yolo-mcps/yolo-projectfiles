use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

const TOOL_NAME: &str = "edit";

#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditOperation {
    /// The exact string to find and replace
    pub old_string: String,
    /// The string to replace it with
    pub new_string: String,
    /// Expected number of replacements (defaults to 1)
    #[serde(default = "default_expected_replacements")]
    pub expected_replacements: u32,
}

fn default_expected_replacements() -> u32 {
    1
}

#[mcp_tool(
    name = "edit", 
    description = "Performs exact string replacements in files within the project directory only. Validates occurrence count before replacing. Supports single or multiple sequential edits. IMPORTANT: File must be read first using the read tool."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditTool {
    /// Path to the file to edit (relative to project root)
    pub file_path: String,
    
    // Single edit mode (backwards compatibility)
    /// The exact string to find and replace (for single edit mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_string: Option<String>,
    /// The string to replace it with (for single edit mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_string: Option<String>,
    /// Expected number of replacements (for single edit mode, defaults to 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_replacements: Option<u32>,
    
    // Multiple edit mode
    /// Array of edit operations to perform sequentially
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<EditOperation>>,
}

#[async_trait]
impl StatefulTool for EditTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Determine which mode we're in and normalize to a list of edits
        let edits = if let Some(edits) = self.edits {
            // Multi-edit mode
            if !edits.is_empty() {
                edits
            } else {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME, 
                    "No edit operations provided"
                )));
            }
        } else if let (Some(old_string), Some(new_string)) = (self.old_string, self.new_string) {
            // Single edit mode - convert to list
            vec![EditOperation {
                old_string,
                new_string,
                expected_replacements: self.expected_replacements.unwrap_or(1),
            }]
        } else {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME, 
                "Must provide either 'edits' array or 'old_string'/'new_string' pair"
            )));
        };
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
        let requested_path = Path::new(&self.file_path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path '{}': {}", self.file_path, e))))?;
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME, 
                &self.file_path, 
                "Path is outside the project directory"
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.file_path)));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME, 
                &format!("Path is not a file: {}", self.file_path)
            )));
        }

        // Check if this is a new file creation (first edit has empty old_string)
        let is_new_file = edits.first().map(|e| e.old_string.is_empty()).unwrap_or(false);
        
        if !canonical_path.exists() && is_new_file {
            // Create the file with empty content
            if let Some(parent) = canonical_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create directory: {}", e))))?;
            }
            fs::write(&canonical_path, "")
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create file: {}", e))))?;
        }
        
        // Check if file has been read (unless it's a new file)
        let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        
        if !is_new_file && !read_files.contains(&canonical_path) {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME, 
                &format!("File must be read before editing: {}", self.file_path)
            )));
        }

        // Validate all edits first
        for (idx, edit) in edits.iter().enumerate() {
            if edit.old_string == edit.new_string {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME, 
                    &format!("Edit {}: old_string and new_string cannot be the same", idx + 1)
                )));
            }
        }

        // Read the file
        let mut content = fs::read_to_string(&canonical_path)
            .await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;

        // Apply edits sequentially
        let mut total_replacements = 0;
        for (idx, edit) in edits.iter().enumerate() {
            // Count occurrences
            let occurrence_count = content.matches(&edit.old_string).count();
            
            if occurrence_count == 0 && !edit.old_string.is_empty() {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME, 
                    &format!("Edit {}: String '{}' not found in content", idx + 1, edit.old_string)
                )));
            }

            if occurrence_count != edit.expected_replacements as usize {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME, 
                    &format!("Edit {}: Expected {} replacements but found {} occurrences", idx + 1, edit.expected_replacements, occurrence_count)
                )));
            }

            // Perform replacement
            content = content.replace(&edit.old_string, &edit.new_string);
            total_replacements += occurrence_count;
        }

        // Write back to file
        fs::write(&canonical_path, &content)
            .await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to write file: {}", e))))?;

        // Track written files
        let written_files = context.get_custom_state::<HashSet<PathBuf>>().await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
        let mut written_files_clone = (*written_files).clone();
        written_files_clone.insert(canonical_path.clone());
        context.set_custom_state(written_files_clone).await;

        // If this was a new file, also add it to read files
        if is_new_file {
            let mut read_files_clone = (*read_files).clone();
            read_files_clone.insert(canonical_path.clone());
            context.set_custom_state(read_files_clone).await;
        }
        
        let message = if edits.len() == 1 {
            format!(
                "Successfully replaced {} occurrence{} in {}",
                total_replacements,
                if total_replacements == 1 { "" } else { "s" },
                self.file_path
            )
        } else {
            format!(
                "Successfully applied {} edit{} with {} total replacement{} in {}",
                edits.len(),
                if edits.len() == 1 { "" } else { "s" },
                total_replacements,
                if total_replacements == 1 { "" } else { "s" },
                self.file_path
            )
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl EditTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}