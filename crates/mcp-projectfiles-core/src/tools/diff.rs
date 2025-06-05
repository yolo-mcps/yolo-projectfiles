use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;

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
    description = "Compare files showing unified diff. Configurable context lines, whitespace handling.
Examples: {\"file1\": \"old.txt\", \"file2\": \"new.txt\"}, {\"file1\": \"a.js\", \"file2\": \"b.js\", \"ignore_whitespace\": true}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct DiffTool {
    /// First file to compare (relative to project root)
    pub file1: String,
    
    /// Second file to compare (relative to project root)
    pub file2: String,
    
    /// Number of context lines to show around changes (optional, default: 3)
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
    
    /// Whether to ignore whitespace differences (optional, default: false)
    #[serde(default)]
    pub ignore_whitespace: bool,
    
    /// Follow symlinks to compare files outside the project directory (optional, default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    

}

fn default_context_lines() -> u32 {
    3
}

fn default_follow_symlinks() -> bool {
    true
}



impl Default for DiffTool {
    fn default() -> Self {
        Self {
            file1: String::new(),
            file2: String::new(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        }
    }
}

#[async_trait]
impl StatefulTool for DiffTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve both file paths with symlink support
        let canonical_file1 = resolve_path_for_read(&self.file1, &project_root, self.follow_symlinks, TOOL_NAME)?;
        let canonical_file2 = resolve_path_for_read(&self.file2, &project_root, self.follow_symlinks, TOOL_NAME)?;
        
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
    async fn test_diff_identical_files() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Line 1\nLine 2\nLine 3\n";
        create_test_file(temp_dir.path(), "file1.txt", content).await;
        create_test_file(temp_dir.path(), "file2.txt", content).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        assert_eq!(output.is_error, Some(false));
        
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            assert!(content.contains("Files are identical"));
            assert!(content.contains("0 additions(+), 0 deletions(-), 3 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_added_lines() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nLine 2\n";
        let content2 = "Line 1\nLine 2\nLine 3\nLine 4\n";
        create_test_file(temp_dir.path(), "original.txt", content1).await;
        create_test_file(temp_dir.path(), "modified.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "original.txt".to_string(),
            file2: "modified.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain diff header
            assert!(content.contains("--- original.txt"));
            assert!(content.contains("+++ modified.txt"));
            
            // Should show additions
            assert!(content.contains("+Line 3"));
            assert!(content.contains("+Line 4"));
            
            // Should show summary
            assert!(content.contains("2 additions(+), 0 deletions(-), 2 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_deleted_lines() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nLine 2\nLine 3\nLine 4\n";
        let content2 = "Line 1\nLine 2\n";
        create_test_file(temp_dir.path(), "original.txt", content1).await;
        create_test_file(temp_dir.path(), "modified.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "original.txt".to_string(),
            file2: "modified.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show deletions
            assert!(content.contains("-Line 3"));
            assert!(content.contains("-Line 4"));
            
            // Should show summary
            assert!(content.contains("0 additions(+), 2 deletions(-), 2 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_modified_lines() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nOriginal Line\nLine 3\n";
        let content2 = "Line 1\nModified Line\nLine 3\n";
        create_test_file(temp_dir.path(), "file1.txt", content1).await;
        create_test_file(temp_dir.path(), "file2.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show both deletion and addition for modification
            assert!(content.contains("-Original Line"));
            assert!(content.contains("+Modified Line"));
            
            // Should show summary
            assert!(content.contains("1 additions(+), 1 deletions(-), 2 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_ignore_whitespace() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nLine 2   \nLine 3\n";
        let content2 = "Line 1\nLine 2\nLine 3\n";
        create_test_file(temp_dir.path(), "file1.txt", content1).await;
        create_test_file(temp_dir.path(), "file2.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: true,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should be identical when ignoring whitespace
            assert!(content.contains("Files are identical"));
        }
    }

    #[tokio::test]
    async fn test_diff_dont_ignore_whitespace() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nLine 2   \nLine 3\n";
        let content2 = "Line 1\nLine 2\nLine 3\n";
        create_test_file(temp_dir.path(), "file1.txt", content1).await;
        create_test_file(temp_dir.path(), "file2.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show difference when not ignoring whitespace
            assert!(!content.contains("Files are identical"));
            assert!(content.contains("1 additions(+), 1 deletions(-), 2 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_custom_context_lines() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\n";
        let content2 = "Line 1\nLine 2\nLine 3\nChanged\nLine 5\nLine 6\nLine 7\n";
        create_test_file(temp_dir.path(), "file1.txt", content1).await;
        create_test_file(temp_dir.path(), "file2.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: 1, // Only 1 context line
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain the change
            assert!(content.contains("-Line 4"));
            assert!(content.contains("+Changed"));
        }
    }

    #[tokio::test]
    async fn test_diff_empty_files() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "empty1.txt", "").await;
        create_test_file(temp_dir.path(), "empty2.txt", "").await;
        
        let diff_tool = DiffTool {
            file1: "empty1.txt".to_string(),
            file2: "empty2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should be identical
            assert!(content.contains("Files are identical"));
            assert!(content.contains("0 additions(+), 0 deletions(-), 0 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_file_vs_empty() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Line 1\nLine 2\n";
        create_test_file(temp_dir.path(), "content.txt", content).await;
        create_test_file(temp_dir.path(), "empty.txt", "").await;
        
        let diff_tool = DiffTool {
            file1: "content.txt".to_string(),
            file2: "empty.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show all lines as deletions
            assert!(content.contains("-Line 1"));
            assert!(content.contains("-Line 2"));
            assert!(content.contains("0 additions(+), 2 deletions(-), 0 unchanged lines"));
        }
    }

    #[tokio::test]
    async fn test_diff_nested_files() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create nested directories
        let dir1 = temp_dir.path().join("dir1");
        let dir2 = temp_dir.path().join("dir2");
        fs::create_dir(&dir1).await.expect("Failed to create dir1");
        fs::create_dir(&dir2).await.expect("Failed to create dir2");
        
        create_test_file(&dir1, "file.txt", "Content A\n").await;
        create_test_file(&dir2, "file.txt", "Content B\n").await;
        
        let diff_tool = DiffTool {
            file1: "dir1/file.txt".to_string(),
            file2: "dir2/file.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show the difference
            assert!(content.contains("-Content A"));
            assert!(content.contains("+Content B"));
        }
    }

    #[tokio::test]
    async fn test_diff_file1_not_found() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "exists.txt", "content").await;
        
        let diff_tool = DiffTool {
            file1: "nonexistent.txt".to_string(),
            file2: "exists.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:diff"));
        assert!(error.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_diff_file2_not_found() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "exists.txt", "content").await;
        
        let diff_tool = DiffTool {
            file1: "exists.txt".to_string(),
            file2: "nonexistent.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:diff"));
        assert!(error.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_diff_file1_outside_project() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "inside.txt", "content").await;
        
        let diff_tool = DiffTool {
            file1: "../outside.txt".to_string(),
            file2: "inside.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:diff"));
        let error_str = error.to_string();
        assert!(error_str.contains("not found") || error_str.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_diff_file2_outside_project() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "inside.txt", "content").await;
        
        let diff_tool = DiffTool {
            file1: "inside.txt".to_string(),
            file2: "../outside.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:diff"));
        let error_str = error.to_string();
        assert!(error_str.contains("not found") || error_str.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_diff_default_context_lines() {
        let (context, temp_dir) = setup_test_context().await;
        let content1 = "Line 1\n";
        let content2 = "Line 1\nLine 2\n";
        create_test_file(temp_dir.path(), "file1.txt", content1).await;
        create_test_file(temp_dir.path(), "file2.txt", content2).await;
        
        let diff_tool = DiffTool {
            file1: "file1.txt".to_string(),
            file2: "file2.txt".to_string(),
            context_lines: default_context_lines(),
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain the addition
            assert!(content.contains("+Line 2"));
        }
    }

    #[tokio::test]
    async fn test_diff_large_files() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create large files with differences
        let mut content1 = String::new();
        let mut content2 = String::new();
        
        for i in 1..=100 {
            content1.push_str(&format!("Line {}\n", i));
            if i == 50 {
                content2.push_str("Modified Line 50\n");
            } else {
                content2.push_str(&format!("Line {}\n", i));
            }
        }
        
        create_test_file(temp_dir.path(), "large1.txt", &content1).await;
        create_test_file(temp_dir.path(), "large2.txt", &content2).await;
        
        let diff_tool = DiffTool {
            file1: "large1.txt".to_string(),
            file2: "large2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show the modification
            assert!(content.contains("-Line 50"));
            assert!(content.contains("+Modified Line 50"));
            assert!(content.contains("1 additions(+), 1 deletions(-), 99 unchanged lines"));
        }
    }
    
    #[tokio::test]
    async fn test_diff_symlink_within_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file1.txt"), "line1\nline2\nline3").await.unwrap();
        fs::write(project_root.join("file2.txt"), "line1\nmodified line2\nline3").await.unwrap();
        
        // Create symlinks within project directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("file1.txt", project_root.join("link1.txt")).unwrap();
            symlink("file2.txt", project_root.join("link2.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file("file1.txt", project_root.join("link1.txt")).unwrap();
            symlink_file("file2.txt", project_root.join("link2.txt")).unwrap();
        }
        
        let diff_tool = DiffTool {
            file1: "link1.txt".to_string(),
            file2: "link2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content_item = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content_item {
            // Should show diff between target files
            assert!(text.text.contains("-line2"));
            assert!(text.text.contains("+modified line2"));
        }
    }
    
    #[tokio::test]
    async fn test_diff_symlink_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create external files
        let external_temp = TempDir::new().unwrap();
        let external_file1 = external_temp.path().join("external1.txt");
        let external_file2 = external_temp.path().join("external2.txt");
        fs::write(&external_file1, "original content\nline2").await.unwrap();
        fs::write(&external_file2, "modified content\nline2").await.unwrap();
        
        let project_root = context.get_project_root().unwrap();
        
        // Create symlinks pointing outside project directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&external_file1, project_root.join("external_link1.txt")).unwrap();
            symlink(&external_file2, project_root.join("external_link2.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file(&external_file1, project_root.join("external_link1.txt")).unwrap();
            symlink_file(&external_file2, project_root.join("external_link2.txt")).unwrap();
        }
        
        let diff_tool = DiffTool {
            file1: "external_link1.txt".to_string(),
            file2: "external_link2.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content_item = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content_item {
            // Should show diff between external target files
            assert!(text.text.contains("-original content"));
            assert!(text.text.contains("+modified content"));
        }
    }
    
    #[tokio::test]
    async fn test_diff_symlink_disabled() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create external file
        let external_temp = TempDir::new().unwrap();
        let external_file = external_temp.path().join("external.txt");
        fs::write(&external_file, "external content").await.unwrap();
        
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("internal.txt"), "internal content").await.unwrap();
        
        // Create symlink pointing outside project directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&external_file, project_root.join("external_link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file(&external_file, project_root.join("external_link.txt")).unwrap();
        }
        
        let diff_tool = DiffTool {
            file1: "internal.txt".to_string(),
            file2: "external_link.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: false,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Cannot access symlink"));
    }
    
    #[tokio::test]
    async fn test_diff_broken_symlink() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("real_file.txt"), "real content").await.unwrap();
        
        // Create symlink pointing to non-existent file
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("nonexistent.txt", project_root.join("broken_link.txt")).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file("nonexistent.txt", project_root.join("broken_link.txt")).unwrap();
        }
        
        let diff_tool = DiffTool {
            file1: "real_file.txt".to_string(),
            file2: "broken_link.txt".to_string(),
            context_lines: 3,
            ignore_whitespace: false,
            follow_symlinks: true,
        };
        
        let result = diff_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }
}