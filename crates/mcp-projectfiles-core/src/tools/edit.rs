use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::theme::DiffTheme;
use crate::tools::utils::{format_count, format_path};
use async_trait::async_trait;
use colored::control;
use colored::*;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

const TOOL_NAME: &str = "edit";
const MAX_OCCURRENCES_TO_SHOW: usize = 5;

/// Find all occurrences of a string in content and return their locations with context
fn find_occurrences_with_context(content: &str, search_str: &str, context_chars: usize) -> Vec<(usize, String)> {
    let mut occurrences = Vec::new();
    let mut start = 0;
    
    while let Some(pos) = content[start..].find(search_str) {
        let absolute_pos = start + pos;
        
        // Find line number
        let line_num = content[..absolute_pos].matches('\n').count() + 1;
        
        // Get context around the occurrence
        let context_start = absolute_pos.saturating_sub(context_chars);
        let context_end = (absolute_pos + search_str.len() + context_chars).min(content.len());
        
        // Find line boundaries for better context
        let line_start = content[..absolute_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = content[absolute_pos..].find('\n').map(|p| absolute_pos + p).unwrap_or(content.len());
        
        // Use the wider context (either character-based or line-based)
        let actual_start = context_start.min(line_start);
        let actual_end = context_end.max(line_end);
        
        let context = &content[actual_start..actual_end];
        
        // Replace newlines with \n for display
        let display_context = context.replace('\n', "\\n");
        
        // Add ellipsis if truncated
        let mut display = String::new();
        if actual_start > 0 {
            display.push_str("...");
        }
        display.push_str(&display_context);
        if actual_end < content.len() {
            display.push_str("...");
        }
        
        occurrences.push((line_num, display));
        
        start = absolute_pos + search_str.len();
    }
    
    occurrences
}

/// Generate a colored unified diff using the similar crate
fn generate_colored_diff(original: &str, modified: &str, file_path: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);
    let mut output = String::new();

    // Get the current theme from environment
    let theme = DiffTheme::current();

    // Check if we're in a test environment (no colors for tests)
    let use_colors = if cfg!(test) {
        false
    } else if theme == DiffTheme::None {
        false
    } else {
        control::set_override(true);
        true
    };

    // Add header
    if use_colors {
        output.push_str(&format!(
            "{} {}\n",
            theme.colorize_header_old("---"),
            theme.colorize_header_old(file_path)
        ));
        output.push_str(&format!(
            "{} {}\n",
            theme.colorize_header_new("+++"),
            theme.colorize_header_new(file_path)
        ));
    } else {
        output.push_str(&format!("--- {}\n", file_path));
        output.push_str(&format!("+++ {}\n", file_path));
    }

    // Generate hunks
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        // Add hunk header
        if use_colors {
            output.push_str(&format!(
                "{}\n",
                theme.colorize_hunk_header(&hunk.header().to_string())
            ));
        } else {
            output.push_str(&format!("{}\n", hunk.header().to_string()));
        }

        // Process each line in the hunk
        for change in hunk.iter_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    output.push_str(&format!(" {}", change.value()));
                }
                ChangeTag::Delete => {
                    if use_colors {
                        output.push_str(&format!(
                            "{}{}",
                            theme.colorize_deletion_marker("-"),
                            theme.colorize_deletion(change.value())
                        ));
                    } else {
                        output.push_str(&format!("-{}", change.value()));
                    }
                }
                ChangeTag::Insert => {
                    if use_colors {
                        output.push_str(&format!(
                            "{}{}",
                            theme.colorize_addition_marker("+"),
                            theme.colorize_addition(change.value())
                        ));
                    } else {
                        output.push_str(&format!("+{}", change.value()));
                    }
                }
            }
        }
    }

    output
}

#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditOperation {
    /// The exact string to find and replace
    pub old: String,
    /// The string to replace it with
    pub new: String,
    /// Expected number of replacements (defaults to 1)
    #[serde(default = "default_expected")]
    pub expected: u32,
}

fn default_expected() -> u32 {
    1
}

/// Replace exact strings in files. Preferred over system text editors.
///
/// IMPORTANT: You must choose either single edit mode OR multi-edit mode, not both.
/// NOTE: Omit optional parameters when not needed, don't pass null.
///
/// Parameters:
/// - path: File to edit (required)
/// - old: Exact string to find (optional - required for single mode)
/// - new: Replacement string (optional - required for single mode)
/// - expected: Expected match count (optional - default: 1)
/// - edits: Array of edit operations (optional - required for multi mode)
/// - show_diff: Show changes made (optional - default: false)
///
/// # Single Edit Mode
/// Use for simple, one-time replacements. Requires 'old' and 'new' parameters:
/// ```json
/// {
///   "path": "src/main.rs",
///   "old": "println!(\"Hello\")",
///   "new": "println!(\"Hello, World\")"
/// }
/// ```
///
/// With expected count:
/// ```json
/// {
///   "path": "src/config.rs",
///   "old": "debug = false",
///   "new": "debug = true",
///   "expected": 2
/// }
/// ```
///
/// # Multi-Edit Mode  
/// Use for multiple sequential replacements. Requires 'edits' array:
/// ```json
/// {
///   "path": "src/config.rs",
///   "edits": [
///     {
///       "old": "const VERSION: &str = \"1.0.0\"",
///       "new": "const VERSION: &str = \"1.1.0\"",
///       "expected": 1
///     },
///     {
///       "old": "debug = false",
///       "new": "debug = true",
///       "expected": 2
///     }
///   ]
/// }
/// ```
///
/// # Creating New Files
/// To create a new file, use multi-edit mode with an empty old in the first edit:
/// ```json
/// {
///   "path": "src/new_file.rs",
///   "edits": [
///     {
///       "old": "",
///       "new": "fn main() {\n    println!(\"New file!\");\n}",
///       "expected": 1
///     }
///   ]
/// }
/// ```
///
/// # Show Diff
/// To see changes before they're applied:
/// ```json
/// {
///   "path": "README.md",
///   "old": "version 1.0",
///   "new": "version 2.0",
///   "show_diff": true
/// }
/// ```
#[mcp_tool(
    name = "edit",
    description = "Replace exact strings in files. Preferred over system text editors.

CRITICAL: File MUST be read first. String must match EXACTLY (whitespace, indentation, line endings).
HINT: When reading a file for editing, use linenumbers:false to get raw content without line prefixes.
NOTE: Omit optional parameters when not needed, don't pass null.

Two modes:
1) Single edit: Use old/new/expected
2) Multi-edit: Use edits array for sequential replacements

Parameters:
- path: File to edit (required)
- old: Exact string to find (optional, required for single mode)
- new: Replacement string (optional, required for single mode)  
- expected: Expected match count (optional, default: 1)
- edits: Array of {old, new, expected} (optional, required for multi mode)
- show_diff: Show changes made (optional, default: false)
- dry_run: Preview changes without modifying file (optional, default: false)

When to use show_diff:
- Set to true when you want to review changes before they're applied
- Useful for complex edits where you want visual confirmation
- Helpful when making sensitive changes to configuration files
- Recommended for first-time edits to unfamiliar code
- Not needed for simple, straightforward replacements
- Large diffs are truncated to first 100 lines for readability

When to use dry_run:
- Set to true when user asks to 'carefully' make changes
- Use when editing critical configuration files
- Recommended for complex multi-edit operations
- Allows preview of all changes before committing
- Returns diff output without modifying the file
- After showing the dry run preview, ask the user if they would like to proceed with the actual changes

Error Handling:
- When multiple occurrences are found, the error shows where they are located
- When string is not found, hints about possible issues are provided
- Use 'replace_all: true' to replace all occurrences when needed
"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditTool {
    /// Path to the file to edit (relative to project root)
    pub path: String,

    /// The exact string to find and replace (for single edit mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// The string to replace it with (for single edit mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    /// Expected number of replacements (for single edit mode, defaults to 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<u32>,

    // Multiple edit mode
    /// Array of edit operations to perform sequentially
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<EditOperation>>,

    /// Show a diff of the changes made (default: false)
    #[serde(default)]
    pub show_diff: bool,
    
    /// Perform a dry run - show what would be changed without actually modifying the file (default: false)
    #[serde(default)]
    pub dry_run: bool,
}

#[async_trait]
impl StatefulTool for EditTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Validate that single and multi-edit parameters are not mixed
        if self.edits.is_some()
            && (self.old.is_some() || self.new.is_some() || self.expected.is_some())
        {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Cannot mix single edit parameters (old/new/expected) with multi-edit (edits array)",
            )));
        }

        // Determine which mode we're in and normalize to a list of edits
        let edits = if let Some(edits) = self.edits {
            // Multi-edit mode
            if !edits.is_empty() {
                edits
            } else {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    "No edit operations provided",
                )));
            }
        } else if let (Some(old), Some(new)) = (self.old, self.new) {
            // Single edit mode - convert to list
            vec![EditOperation {
                old,
                new,
                expected: self.expected.unwrap_or(1),
            }]
        } else {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Must provide either 'edits' array or 'old'/'new' pair",
            )));
        };
        let project_root = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        // Canonicalize project root for consistent path comparison
        let canonical_project_root = project_root.canonicalize().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to canonicalize project root: {}", e),
            ))
        })?;

        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            project_root.join(requested_path)
        };

        // For edit operations, we need to handle files that might not exist yet
        let canonical_path = if absolute_path.exists() {
            absolute_path.canonicalize().map_err(|e| {
                CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Failed to resolve path '{}': {}", self.path, e),
                ))
            })?
        } else {
            // For non-existent files, canonicalize the parent directory
            let parent = absolute_path.parent().ok_or_else(|| {
                CallToolError::from(tool_errors::invalid_input(TOOL_NAME, "Invalid file path"))
            })?;
            let canonical_parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            canonical_parent.join(absolute_path.file_name().unwrap())
        };

        if !canonical_path.starts_with(&canonical_project_root) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory",
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME, &self.path,
            )));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path is not a file: {}", self.path),
            )));
        }

        // Check if this is a new file creation (first edit has empty old)
        let is_new_file = edits.first().map(|e| e.old.is_empty()).unwrap_or(false);

        if !canonical_path.exists() && is_new_file {
            // Create the file with empty content
            if let Some(parent) = canonical_path.parent() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to create directory: {}", e),
                    ))
                })?;
            }
            fs::write(&canonical_path, "").await.map_err(|e| {
                CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Failed to create file: {}", e),
                ))
            })?;
        }

        // Check if file has been read (unless it's a new file)
        let read_files = context
            .get_custom_state::<HashSet<PathBuf>>()
            .await
            .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));

        if !is_new_file && !read_files.contains(&canonical_path) {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME,
                &format!("File must be read before editing: {}", self.path),
            )));
        }

        // Validate all edits first
        for (idx, edit) in edits.iter().enumerate() {
            if edit.old == edit.new {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Edit {}: old and new cannot be the same", idx + 1),
                )));
            }
        }

        // Read the file
        let mut content = fs::read_to_string(&canonical_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read file: {}", e),
            ))
        })?;

        // Capture original content for diff if requested or dry run
        let original_content = if self.show_diff || self.dry_run {
            Some(content.clone())
        } else {
            None
        };

        // Apply edits sequentially
        let mut total_replacements = 0;
        let mut first_edit_line = None;
        for (idx, edit) in edits.iter().enumerate() {
            // Count occurrences
            let occurrence_count = content.matches(&edit.old).count();

            if occurrence_count == 0 && !edit.old.is_empty() {
                let mut error_msg = format!(
                    "Edit {}: String not found in content:\n{}",
                    idx + 1,
                    edit.old
                );
                
                // Try to find similar strings if the search string is reasonably sized
                if edit.old.len() >= 10 && edit.old.len() <= 200 {
                    // Look for partial matches (beginning or end of search string)
                    let search_start = &edit.old[..edit.old.len().min(20)];
                    let search_end = if edit.old.len() > 20 {
                        &edit.old[edit.old.len().saturating_sub(20)..]
                    } else {
                        ""
                    };
                    
                    let mut suggestions = Vec::new();
                    
                    // Find lines containing the start of the search string
                    if content.contains(search_start) {
                        suggestions.push(format!("String starting with '{}' was found", search_start));
                    }
                    
                    // Find lines containing the end of the search string
                    if !search_end.is_empty() && content.contains(search_end) {
                        suggestions.push(format!("String ending with '{}' was found", search_end));
                    }
                    
                    if !suggestions.is_empty() {
                        error_msg.push_str("\n\nPossible issues:");
                        for suggestion in suggestions {
                            error_msg.push_str(&format!("\n  - {}", suggestion));
                        }
                        error_msg.push_str("\n\nHint: Check for extra/missing whitespace, line breaks, or special characters.");
                    }
                }
                
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &error_msg,
                )));
            }

            if occurrence_count != edit.expected as usize {
                let mut error_msg = format!(
                    "Edit {}: Expected {} replacements but found {} occurrences",
                    idx + 1,
                    edit.expected,
                    occurrence_count
                );
                
                // Show where the occurrences are when there are multiple
                if occurrence_count > 1 && occurrence_count <= 10 {
                    error_msg.push_str("\n\nOccurrences found:");
                    let occurrences = find_occurrences_with_context(&content, &edit.old, 40);
                    
                    for (i, (line_num, context)) in occurrences.iter().take(MAX_OCCURRENCES_TO_SHOW).enumerate() {
                        error_msg.push_str(&format!("\n  {}. Line {}: {}", i + 1, line_num, context));
                    }
                    
                    if occurrences.len() > MAX_OCCURRENCES_TO_SHOW {
                        error_msg.push_str(&format!(
                            "\n  ... and {} more occurrences",
                            occurrences.len() - MAX_OCCURRENCES_TO_SHOW
                        ));
                    }
                    
                    error_msg.push_str("\n\nHint: You can:\n");
                    error_msg.push_str("  - Use 'replace_all: true' to replace all occurrences\n");
                    error_msg.push_str("  - Make your search string more specific by including surrounding context\n");
                    error_msg.push_str("  - Use multi-edit mode with an 'edits' array to handle different occurrences separately");
                }
                
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &error_msg,
                )));
            }

            // Track line number for the first edit
            if first_edit_line.is_none() && !edit.old.is_empty() {
                if let Some(pos) = content.find(&edit.old) {
                    let line_number = content[..pos].matches('\n').count() + 1;
                    first_edit_line = Some(line_number);
                }
            }

            // Perform replacement
            content = content.replace(&edit.old, &edit.new);
            total_replacements += occurrence_count;
        }

        // Write back to file (unless dry run)
        if !self.dry_run {
            fs::write(&canonical_path, &content).await.map_err(|e| {
                CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Failed to write file: {}", e),
                ))
            })?;

            // Track written files
            let written_files = context
                .get_custom_state::<HashSet<PathBuf>>()
                .await
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
        }

        // Format path relative to project root
        let relative_path = canonical_path
            .strip_prefix(&project_root)
            .unwrap_or(&canonical_path);

        let mut message = if self.dry_run {
            if edits.len() == 1 {
                format!(
                    "[DRY RUN] Would edit file {} ({} at line {})",
                    format_path(relative_path),
                    format_count(total_replacements as usize, "change", "changes"),
                    first_edit_line.map_or("unknown".to_string(), |line| line.to_string())
                )
            } else {
                format!(
                    "[DRY RUN] Would edit file {} ({} in {} edits)",
                    format_path(relative_path),
                    format_count(total_replacements as usize, "change", "changes"),
                    edits.len()
                )
            }
        } else {
            if edits.len() == 1 {
                format!(
                    "Edited file {} ({} at line {})",
                    format_path(relative_path),
                    format_count(total_replacements as usize, "change", "changes"),
                    first_edit_line.map_or("unknown".to_string(), |line| line.to_string())
                )
            } else {
                format!(
                    "Edited file {} ({} in {} edits)",
                    format_path(relative_path),
                    format_count(total_replacements as usize, "change", "changes"),
                    edits.len()
                )
            }
        };

        // Generate colored diff if requested or dry run
        if self.show_diff || self.dry_run {
            if let Some(original) = original_content {
                message.push_str("\n\n");

                // Generate colored diff
                let colored_diff = generate_colored_diff(&original, &content, &self.path);

                if colored_diff.lines().count() > 2 {
                    // More than just headers
                    message.push_str(&colored_diff);

                    // For very large diffs, truncate after 100 lines
                    let line_count = message.lines().count();
                    if line_count > 100 {
                        let lines: Vec<&str> = message.lines().take(100).collect();
                        message = lines.join("\n");
                        message.push_str(&format!(
                            "\n{}",
                            "... (diff truncated, showing first 100 lines)".yellow()
                        ));
                    }
                } else {
                    message.push_str(&format!(
                        "{}",
                        "(no visible changes in diff - possibly whitespace only)".yellow()
                    ));
                }
            }
        }
        
        // Add note for dry run
        if self.dry_run {
            message.push_str("\n\n");
            message.push_str("No changes were made to the file (dry run mode).");
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ReadTool;
    use tempfile::TempDir;

    fn extract_text_content(result: &CallToolResult) -> String {
        match &result.content[0] {
            CallToolResultContentItem::TextContent(text) => text.text.clone(),
            _ => panic!("Expected text content"),
        }
    }

    async fn setup_test_file_with_read(context: &ToolContext, file_name: &str, content: &str) {
        // Create the file
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join(file_name);
        tokio::fs::write(&file_path, content).await.unwrap();

        // Mark it as read using the same context
        let read_tool = ReadTool {
            path: file_name.to_string(),
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
        };
        let _ = read_tool.call_with_context(context).await.unwrap();
    }

    #[tokio::test]
    async fn test_parameter_validation_mixed_modes() {
        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());

        // Try to mix single edit and multi-edit parameters
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: Some("old".to_string()),
            new: Some("new".to_string()),
            expected: Some(1),
            edits: Some(vec![EditOperation {
                old: "foo".to_string(),
                new: "bar".to_string(),
                expected: 1,
            }]),
            show_diff: false,
            dry_run: false,
        };

        let result = tool.call_with_context(&context).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Cannot mix single edit parameters")
        );
    }

    #[tokio::test]
    async fn test_single_edit_mode() {
        let temp_dir = TempDir::new().unwrap();

        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        let file_path = temp_dir.path().join("test.txt");

        // Setup test file
        setup_test_file_with_read(&context, "test.txt", "Hello world").await;

        // Perform single edit
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: Some("world".to_string()),
            new: Some("Rust".to_string()),
            expected: Some(1),
            edits: None,
            show_diff: false,
            dry_run: false,
        };

        let result = tool.call_with_context(&context).await.unwrap();
        let message = extract_text_content(&result);
        assert!(message.contains("Edited file"));

        // Verify file content
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello Rust");
    }

    #[tokio::test]
    async fn test_multi_edit_mode() {
        let temp_dir = TempDir::new().unwrap();

        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        let file_path = temp_dir.path().join("test.txt");

        // Setup test file
        setup_test_file_with_read(&context, "test.txt", "foo bar foo baz").await;

        // Perform multiple edits
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: None,
            new: None,
            expected: None,
            edits: Some(vec![
                EditOperation {
                    old: "foo".to_string(),
                    new: "FOO".to_string(),
                    expected: 2,
                },
                EditOperation {
                    old: "bar".to_string(),
                    new: "BAR".to_string(),
                    expected: 1,
                },
            ]),
            show_diff: false,
            dry_run: false,
        };

        let result = tool.call_with_context(&context).await.unwrap();
        let message = extract_text_content(&result);
        assert!(message.contains("Edited file"));

        // Verify file content
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "FOO BAR FOO baz");
    }

    #[tokio::test]
    async fn test_show_diff() {
        let temp_dir = TempDir::new().unwrap();

        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());

        // Setup test file
        setup_test_file_with_read(&context, "test.txt", "Line 1\nLine 2\nLine 3").await;

        // Edit with diff enabled
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: Some("Line 2".to_string()),
            new: Some("Modified Line 2".to_string()),
            expected: Some(1),
            edits: None,
            show_diff: true,
            dry_run: false,
        };

        let result = tool.call_with_context(&context).await.unwrap();
        let message = extract_text_content(&result);

        // Check that diff is included
        assert!(message.contains("--- test.txt"));
        assert!(message.contains("+++ test.txt"));
        assert!(message.contains("-Line 2"));
        assert!(message.contains("+Modified Line 2"));
    }

    #[tokio::test]
    async fn test_file_not_read_error() {
        let temp_dir = TempDir::new().unwrap();

        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        let file_path = temp_dir.path().join("test.txt");

        // Create file but don't read it
        tokio::fs::write(&file_path, "content").await.unwrap();

        let tool = EditTool {
            path: "test.txt".to_string(),
            old: Some("content".to_string()),
            new: Some("new content".to_string()),
            expected: Some(1),
            edits: None,
            show_diff: false,
            dry_run: false,
        };

        let result = tool.call_with_context(&context).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("File must be read before editing"),
            "Expected 'File must be read before editing' but got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_dry_run_mode() {
        let temp_dir = TempDir::new().unwrap();
        
        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        let file_path = temp_dir.path().join("test.txt");
        
        // Setup test file
        setup_test_file_with_read(&context, "test.txt", "Hello world").await;
        
        // Perform dry run edit
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: Some("world".to_string()),
            new: Some("Rust".to_string()),
            expected: Some(1),
            edits: None,
            show_diff: false,
            dry_run: true,
        };
        
        let result = tool.call_with_context(&context).await.unwrap();
        let message = extract_text_content(&result);
        
        // Check dry run indicators
        assert!(message.contains("[DRY RUN]"));
        assert!(message.contains("Would edit file"));
        assert!(message.contains("No changes were made to the file"));
        
        // Verify diff is shown
        assert!(message.contains("--- test.txt"));
        assert!(message.contains("+++ test.txt"));
        assert!(message.contains("-Hello world"));
        assert!(message.contains("+Hello Rust"));
        
        // Verify file was NOT modified
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello world");
    }

    #[tokio::test]
    async fn test_dry_run_multi_edit() {
        let temp_dir = TempDir::new().unwrap();
        
        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
        let file_path = temp_dir.path().join("test.txt");
        
        // Setup test file
        setup_test_file_with_read(&context, "test.txt", "foo bar foo baz").await;
        
        // Perform dry run with multiple edits
        let tool = EditTool {
            path: "test.txt".to_string(),
            old: None,
            new: None,
            expected: None,
            edits: Some(vec![
                EditOperation {
                    old: "foo".to_string(),
                    new: "FOO".to_string(),
                    expected: 2,
                },
                EditOperation {
                    old: "bar".to_string(),
                    new: "BAR".to_string(),
                    expected: 1,
                },
            ]),
            show_diff: false,
            dry_run: true,
        };
        
        let result = tool.call_with_context(&context).await.unwrap();
        let message = extract_text_content(&result);
        
        // Check dry run indicators
        assert!(message.contains("[DRY RUN]"));
        assert!(message.contains("Would edit file"));
        assert!(message.contains("3 changes in 2 edits")); // 2 foo replacements + 1 bar replacement = 3 total
        
        // Verify file was NOT modified
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "foo bar foo baz");
    }
}
