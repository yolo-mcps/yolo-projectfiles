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

/// Edit tool for performing string replacements in files.
///
/// # Single Edit Mode
/// Use this mode for simple, one-time replacements:
/// ```json
/// {
///   "file_path": "src/main.rs",
///   "old_string": "println!(\"Hello\")",
///   "new_string": "println!(\"Hello, World\")",
///   "expected_replacements": 1
/// }
/// ```
///
/// # Multi-Edit Mode
/// Use this mode for multiple sequential replacements:
/// ```json
/// {
///   "file_path": "src/config.rs",
///   "edits": [
///     {
///       "old_string": "const VERSION: &str = \"1.0.0\"",
///       "new_string": "const VERSION: &str = \"1.1.0\"",
///       "expected_replacements": 1
///     },
///     {
///       "old_string": "debug = false",
///       "new_string": "debug = true",
///       "expected_replacements": 2
///     }
///   ]
/// }
/// ```
///
/// # Creating New Files
/// To create a new file, use multi-edit mode with an empty old_string in the first edit:
/// ```json
/// {
///   "file_path": "src/new_file.rs",
///   "edits": [
///     {
///       "old_string": "",
///       "new_string": "fn main() {\n    println!(\"New file!\");\n}",
///       "expected_replacements": 1
///     }
///   ]
/// }
/// ```
#[mcp_tool(
    name = "edit",
    description = "Replace exact strings in files. Preferred over system text editors.

CRITICAL: File MUST be read first. String must match EXACTLY (whitespace, indentation, line endings).
HINT: When reading a file for editing, use linenumbers:false to get raw content without line prefixes.

Two modes:
1) Single edit: Use old_string/new_string/expected_replacements
2) Multi-edit: Use edits array for sequential replacements

Parameters:
- file_path: File to edit (required)
- old_string: Exact string to find (single mode)
- new_string: Replacement string (single mode)
- expected_replacements: Expected match count (default: 1)
- edits: Array of {old_string, new_string, expected_replacements} (multi mode)
- show_diff: Show changes made (default: false)

Common errors: Including line numbers from Read output, wrong whitespace/indentation."
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

    /// Show a diff of the changes made (default: false)
    #[serde(default)]
    pub show_diff: bool,
}

#[async_trait]
impl StatefulTool for EditTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Validate that single and multi-edit parameters are not mixed
        if self.edits.is_some()
            && (self.old_string.is_some()
                || self.new_string.is_some()
                || self.expected_replacements.is_some())
        {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Cannot mix single edit parameters (old_string/new_string/expected_replacements) with multi-edit (edits array)",
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
                "Must provide either 'edits' array or 'old_string'/'new_string' pair",
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

        let requested_path = Path::new(&self.file_path);
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
                    &format!("Failed to resolve path '{}': {}", self.file_path, e),
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
                &self.file_path,
                "Path is outside the project directory",
            )));
        }

        if !canonical_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME,
                &self.file_path,
            )));
        }

        if !canonical_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path is not a file: {}", self.file_path),
            )));
        }

        // Check if this is a new file creation (first edit has empty old_string)
        let is_new_file = edits
            .first()
            .map(|e| e.old_string.is_empty())
            .unwrap_or(false);

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
                &format!("File must be read before editing: {}", self.file_path),
            )));
        }

        // Validate all edits first
        for (idx, edit) in edits.iter().enumerate() {
            if edit.old_string == edit.new_string {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!(
                        "Edit {}: old_string and new_string cannot be the same",
                        idx + 1
                    ),
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

        // Capture original content for diff if requested
        let original_content = if self.show_diff {
            Some(content.clone())
        } else {
            None
        };

        // Apply edits sequentially
        let mut total_replacements = 0;
        let mut first_edit_line = None;
        for (idx, edit) in edits.iter().enumerate() {
            // Count occurrences
            let occurrence_count = content.matches(&edit.old_string).count();

            if occurrence_count == 0 && !edit.old_string.is_empty() {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!(
                        "Edit {}: String '{}' not found in content",
                        idx + 1,
                        edit.old_string
                    ),
                )));
            }

            if occurrence_count != edit.expected_replacements as usize {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!(
                        "Edit {}: Expected {} replacements but found {} occurrences",
                        idx + 1,
                        edit.expected_replacements,
                        occurrence_count
                    ),
                )));
            }

            // Track line number for the first edit
            if first_edit_line.is_none() && !edit.old_string.is_empty() {
                if let Some(pos) = content.find(&edit.old_string) {
                    let line_number = content[..pos].matches('\n').count() + 1;
                    first_edit_line = Some(line_number);
                }
            }

            // Perform replacement
            content = content.replace(&edit.old_string, &edit.new_string);
            total_replacements += occurrence_count;
        }

        // Write back to file
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

        // Format path relative to project root
        let relative_path = canonical_path
            .strip_prefix(&project_root)
            .unwrap_or(&canonical_path);

        let mut message = if edits.len() == 1 {
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
        };

        // Generate colored diff if requested
        if self.show_diff {
            if let Some(original) = original_content {
                message.push_str("\n\n");

                // Generate colored diff
                let colored_diff = generate_colored_diff(&original, &content, &self.file_path);

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
            binary_check: true,
            tail: false,
            pattern: None,
            case: "sensitive".to_string(),
            encoding: "utf-8".to_string(),
            linenumbers: true,
            follow_symlinks: true,
        };
        let _ = read_tool.call_with_context(context).await.unwrap();
    }

    #[tokio::test]
    async fn test_parameter_validation_mixed_modes() {
        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());

        // Try to mix single edit and multi-edit parameters
        let tool = EditTool {
            file_path: "test.txt".to_string(),
            old_string: Some("old".to_string()),
            new_string: Some("new".to_string()),
            expected_replacements: Some(1),
            edits: Some(vec![EditOperation {
                old_string: "foo".to_string(),
                new_string: "bar".to_string(),
                expected_replacements: 1,
            }]),
            show_diff: false,
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
            file_path: "test.txt".to_string(),
            old_string: Some("world".to_string()),
            new_string: Some("Rust".to_string()),
            expected_replacements: Some(1),
            edits: None,
            show_diff: false,
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
            file_path: "test.txt".to_string(),
            old_string: None,
            new_string: None,
            expected_replacements: None,
            edits: Some(vec![
                EditOperation {
                    old_string: "foo".to_string(),
                    new_string: "FOO".to_string(),
                    expected_replacements: 2,
                },
                EditOperation {
                    old_string: "bar".to_string(),
                    new_string: "BAR".to_string(),
                    expected_replacements: 1,
                },
            ]),
            show_diff: false,
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
            file_path: "test.txt".to_string(),
            old_string: Some("Line 2".to_string()),
            new_string: Some("Modified Line 2".to_string()),
            expected_replacements: Some(1),
            edits: None,
            show_diff: true,
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
            file_path: "test.txt".to_string(),
            old_string: Some("content".to_string()),
            new_string: Some("new content".to_string()),
            expected_replacements: Some(1),
            edits: None,
            show_diff: false,
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
}

