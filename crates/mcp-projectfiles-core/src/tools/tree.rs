use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_count, format_path, resolve_path_for_read};
use async_trait::async_trait;
use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::fs;
use glob::Pattern;

const TOOL_NAME: &str = "tree";

#[mcp_tool(
    name = "tree",
    description = "Display directory tree with sizes, patterns, depth limits. Supports tree/json output.
Examples: {\"path\": \"src\", \"max_depth\": 2}, {\"path\": \".\", \"dirs_only\": true, \"pattern_filter\": \"*.rs\"}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct TreeTool {
    /// Directory to display as tree (relative to project root)
    #[serde(default = "default_path")]
    pub path: String,
    
    /// Maximum depth to traverse (None = unlimited)
    #[serde(default)]
    pub max_depth: Option<u32>,
    
    /// Whether to show hidden files (files starting with dot)
    #[serde(default)]
    pub show_hidden: bool,
    
    /// Whether to show only directories (no files)
    #[serde(default)]
    pub dirs_only: bool,
    
    /// File pattern filter (e.g., "*.rs", "*.{js,ts}")
    #[serde(default)]
    pub pattern_filter: Option<String>,
    
    // /// File pattern to exclude (e.g., "*.log", "test_*")
    // #[serde(default)]
    // pub exclude_pattern: Option<String>,
    
    /// Follow symlinks for the tree root directory (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    
    /// Output format: "tree" (default) or "json"
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
    
    /// Maximum number of files to include (optional, default: 1000)
    #[serde(default = "default_max_files")]
    pub max_files: Option<u32>,
}

fn default_path() -> String {
    ".".to_string()
}

fn default_follow_symlinks() -> bool {
    true
}

fn default_output_format() -> Option<String> {
    None
}

fn default_max_files() -> Option<u32> {
    Some(1000)
}

#[async_trait]
impl StatefulTool for TreeTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Get project root and resolve path
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve path with symlink support
        let normalized_path = resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;
        
        // Check if path exists and is a directory
        if !normalized_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME,
                &self.path
            )));
        }
        
        if !normalized_path.is_dir() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path '{}' is not a directory", self.path)
            )));
        }
        
        let output_format = self.output_format.as_deref().unwrap_or("tree");
        
        match output_format {
            "json" => {
                let mut stats = TreeStats::default();
                
                // Build JSON tree structure
                let root_name = normalized_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.path)
                    .to_string();
                
                let relative_path = normalized_path.strip_prefix(&project_root)
                    .unwrap_or(&normalized_path);
                
                let root_node = build_json_tree(
                    &normalized_path,
                    &root_name,
                    relative_path.to_string_lossy().to_string(),
                    &self,
                    &mut stats,
                    0,
                ).await?;
                
                let tree_output = TreeOutput {
                    root: root_node,
                    stats: TreeSummary {
                        total_directories: stats.directories,
                        total_files: stats.files,
                        total_size: stats.total_size,
                        total_size_human: format_size(stats.total_size),
                        files_shown: stats.files_shown,
                        files_omitted: stats.files_omitted,
                    },
                };
                
                let json_output = serde_json::to_string_pretty(&tree_output)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to serialize JSON: {}", e))))?;
                
                Ok(CallToolResult {
                    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                        json_output,
                        None,
                    ))],
                    is_error: None,
                    meta: None,
                })
            },
            "tree" | _ => {
                let mut tree_output = String::new();
                let mut stats = TreeStats::default();
                
                // Start with the root directory name
                tree_output.push_str(&format!(
                    "{}\n",
                    normalized_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&self.path)
                ));
                
                // Build the tree
                build_tree(
                    &normalized_path,
                    &mut tree_output,
                    "",
                    true,
                    &self,
                    &mut stats,
                    0,
                ).await?;
                
                // Add summary with path
                let relative_path = normalized_path.strip_prefix(&project_root)
                    .unwrap_or(&normalized_path);
                
                let mut summary = format!(
                    "\nTree of {} - {}, {} ({})",
                    format_path(relative_path),
                    format_count(stats.directories, "directory", "directories"),
                    format_count(stats.files, "file", "files"),
                    format_size(stats.total_size)
                );
                
                if stats.files_omitted > 0 {
                    summary.push_str(&format!(
                        "\n[{} files omitted due to max_files limit of {}]",
                        stats.files_omitted,
                        self.max_files.unwrap_or(1000)
                    ));
                }
                
                tree_output.push_str(&summary);
                
                Ok(CallToolResult {
                    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                        tree_output,
                        None,
                    ))],
                    is_error: None,
                    meta: None,
                })
            }
        }
    }
}

#[derive(Default)]
struct TreeStats {
    directories: usize,
    files: usize,
    total_size: u64,
    files_shown: usize,
    files_omitted: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct TreeNode {
    name: String,
    path: String,
    #[serde(rename = "type")]
    node_type: String,
    size: Option<u64>,
    children: Option<Vec<TreeNode>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct TreeOutput {
    root: TreeNode,
    stats: TreeSummary,
}

#[derive(Serialize, Deserialize, Debug)]
struct TreeSummary {
    total_directories: usize,
    total_files: usize,
    total_size: u64,
    total_size_human: String,
    files_shown: usize,
    files_omitted: usize,
}

async fn build_tree(
    dir: &Path,
    output: &mut String,
    prefix: &str,
    _is_last: bool,
    request: &TreeTool,
    stats: &mut TreeStats,
    current_depth: u32,
) -> Result<(), CallToolError> {
    // Check max depth
    if let Some(max_depth) = request.max_depth {
        if current_depth >= max_depth {
            return Ok(());
        }
    }
    
    // Read directory entries
    let mut entries = fs::read_dir(dir).await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
    
    let mut items = Vec::new();
    
    // Collect all entries first to avoid Send issues
    let mut dir_entries = Vec::new();
    while let Some(entry) = entries.next_entry().await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))? {
        dir_entries.push(entry);
    }
    
    // Process entries
    for entry in dir_entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        // Filter hidden files if requested
        if !request.show_hidden && name_str.starts_with('.') {
            continue;
        }
        
        // Get metadata
        let metadata = entry.metadata().await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get metadata: {}", e))))?;
        
        // Filter directories if dirs_only is set
        if request.dirs_only && !metadata.is_dir() {
            continue;
        }
        
        // Filter by pattern if provided
        if let Some(pattern_str) = &request.pattern_filter {
            let pattern = Pattern::new(pattern_str)
                .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, pattern_str, &format!("Invalid pattern: {}", e))))?;
            if !pattern.matches(&name_str) {
                continue;
            }
        }
        
        // // Exclude by pattern if provided
        // if let Some(exclude_str) = &request.exclude_pattern {
        //     let exclude = Pattern::new(exclude_str)
        //         .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, exclude_str, &format!("Invalid exclude pattern: {}", e))))?;
        //     if exclude.matches(&name_str) {
        //         continue;
        //     }
        // }
        
        items.push((entry.path(), name_str.to_string(), metadata));
    }
    
    // Sort entries (directories first, then alphabetically)
    items.sort_by(|(_, a_name, a_meta), (_, b_name, b_meta)| {
        match (a_meta.is_dir(), b_meta.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a_name.cmp(b_name),
        }
    });
    
    let entry_count = items.len();
    
    for (index, (path, name, metadata)) in items.iter().enumerate() {
        let is_last_entry = index == entry_count - 1;
        let is_dir = metadata.is_dir();
        
        // Update stats
        if is_dir {
            stats.directories += 1;
        } else {
            stats.files += 1;
            stats.total_size += metadata.len();
            
            // Check if we've reached the file limit
            if let Some(max_files) = request.max_files {
                if stats.files_shown >= max_files as usize {
                    stats.files_omitted += 1;
                    continue;
                }
            }
            stats.files_shown += 1;
        }
        
        // Build the tree branch
        let branch = if is_last_entry { "└── " } else { "├── " };
        let size_info = if !is_dir {
            format!(" ({})", format_size(metadata.len()))
        } else {
            String::new()
        };
        
        output.push_str(&format!(
            "{}{}{}{}\n",
            prefix,
            branch,
            name,
            size_info
        ));
        
        // Recursively process subdirectories
        if is_dir {
            let new_prefix = format!(
                "{}{}",
                prefix,
                if is_last_entry { "    " } else { "│   " }
            );
            
            Box::pin(build_tree(
                path,
                output,
                &new_prefix,
                is_last_entry,
                request,
                stats,
                current_depth + 1,
            )).await?;
        }
    }
    
    Ok(())
}

async fn build_json_tree(
    dir: &Path,
    name: &str,
    path: String,
    request: &TreeTool,
    stats: &mut TreeStats,
    current_depth: u32,
) -> Result<TreeNode, CallToolError> {
    // Directory node
    stats.directories += 1;
    
    let mut node = TreeNode {
        name: name.to_string(),
        path,
        node_type: "directory".to_string(),
        size: None,
        children: Some(Vec::new()),
    };
    
    // Check max depth
    if let Some(max_depth) = request.max_depth {
        if current_depth >= max_depth {
            return Ok(node);
        }
    }
    
    // Read directory entries
    let mut entries = fs::read_dir(dir).await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;
    
    let mut items = Vec::new();
    
    // Collect all entries
    let mut dir_entries = Vec::new();
    while let Some(entry) = entries.next_entry().await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read entry: {}", e))))? {
        dir_entries.push(entry);
    }
    
    // Process entries
    for entry in dir_entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        // Filter hidden files if requested
        if !request.show_hidden && name_str.starts_with('.') {
            continue;
        }
        
        // Get metadata
        let metadata = entry.metadata().await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get metadata: {}", e))))?;
        
        // Filter directories if dirs_only is set
        if request.dirs_only && !metadata.is_dir() {
            continue;
        }
        
        // Filter by pattern if provided
        if let Some(pattern_str) = &request.pattern_filter {
            let pattern = Pattern::new(pattern_str)
                .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, pattern_str, &format!("Invalid pattern: {}", e))))?;
            if !pattern.matches(&name_str) {
                continue;
            }
        }
        
        // // Exclude by pattern if provided
        // if let Some(exclude_str) = &request.exclude_pattern {
        //     let exclude = Pattern::new(exclude_str)
        //         .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, exclude_str, &format!("Invalid exclude pattern: {}", e))))?;
        //     if exclude.matches(&name_str) {
        //         continue;
        //     }
        // }
        
        items.push((entry.path(), name_str.to_string(), metadata));
    }
    
    // Sort entries (directories first, then alphabetically)
    items.sort_by(|(_, a_name, a_meta), (_, b_name, b_meta)| {
        match (a_meta.is_dir(), b_meta.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a_name.cmp(b_name),
        }
    });
    
    let children = node.children.as_mut().unwrap();
    
    for (path, name, metadata) in items {
        let is_dir = metadata.is_dir();
        let relative_path = path.strip_prefix(std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current dir: {}", e))))?
        ).unwrap_or(&path).to_string_lossy().to_string();
        
        if is_dir {
            // Recursively process subdirectory
            let child_node = Box::pin(build_json_tree(
                &path,
                &name,
                relative_path,
                request,
                stats,
                current_depth + 1,
            )).await?;
            children.push(child_node);
        } else {
            // File node
            stats.files += 1;
            stats.total_size += metadata.len();
            
            // Check if we've reached the file limit
            if let Some(max_files) = request.max_files {
                if stats.files_shown >= max_files as usize {
                    stats.files_omitted += 1;
                    continue;
                }
            }
            stats.files_shown += 1;
            
            children.push(TreeNode {
                name,
                path: relative_path,
                node_type: "file".to_string(),
                size: Some(metadata.len()),
                children: None,
            });
        }
    }
    
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;
    use tokio::fs;
    use serde_json;

    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }

    async fn create_test_structure(base: &std::path::Path) {
        // Create nested directory structure for testing
        fs::create_dir(base.join("dir1")).await.unwrap();
        fs::create_dir(base.join("dir2")).await.unwrap();
        fs::create_dir(base.join("dir1/subdir1")).await.unwrap();
        fs::create_dir(base.join("dir1/subdir2")).await.unwrap();
        
        // Create files
        fs::write(base.join("file1.txt"), "content1").await.unwrap();
        fs::write(base.join("file2.rs"), "fn main() {}").await.unwrap();
        fs::write(base.join("dir1/nested.txt"), "nested").await.unwrap();
        fs::write(base.join("dir1/subdir1/deep.txt"), "deep").await.unwrap();
        
        // Create hidden files
        fs::write(base.join(".hidden"), "hidden").await.unwrap();
        fs::write(base.join("dir1/.gitignore"), "*.tmp").await.unwrap();
    }

    #[tokio::test]
    async fn test_tree_basic_structure() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        assert_eq!(output.is_error, None);
        
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain tree structure
            assert!(content.contains("├──") || content.contains("└──"));
            assert!(content.contains("dir1"));
            assert!(content.contains("dir2"));
            assert!(content.contains("file1.txt"));
            assert!(content.contains("file2.rs"));
            
            // Should contain summary
            assert!(content.contains("directories"));
            assert!(content.contains("files"));
        }
    }

    #[tokio::test]
    async fn test_tree_dirs_only() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: true,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain directories
            assert!(content.contains("dir1"));
            assert!(content.contains("dir2"));
            assert!(content.contains("subdir1"));
            
            // Should NOT contain files
            assert!(!content.contains("file1.txt"));
            assert!(!content.contains("file2.rs"));
            assert!(!content.contains("nested.txt"));
        }
    }

    #[tokio::test]
    async fn test_tree_max_depth() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: Some(1),
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain top-level items
            assert!(content.contains("dir1"));
            assert!(content.contains("file1.txt"));
            
            // Should NOT contain deep nested items
            assert!(!content.contains("subdir1"));
            assert!(!content.contains("deep.txt"));
        }
    }

    #[tokio::test]
    async fn test_tree_show_hidden() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: true,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain hidden files
            assert!(content.contains(".hidden"));
            assert!(content.contains(".gitignore"));
        }
    }

    #[tokio::test]
    async fn test_tree_pattern_filter() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: Some("*.rs".to_string()),
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            
            // Should contain .rs files
            assert!(content.contains("file2.rs"));
            
            // Should NOT contain .txt files
            assert!(!content.contains("file1.txt"));
            assert!(!content.contains("nested.txt"));
        }
    }

    #[tokio::test]
    async fn test_tree_specific_directory() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: "dir1".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            
            // Should contain items from dir1
            assert!(content.contains("subdir1"));
            assert!(content.contains("nested.txt"));
            
            // Should NOT contain items from root (but dir2 might appear in summary path)
            assert!(!content.contains("file1.txt"));
        }
    }

    #[tokio::test]
    async fn test_tree_empty_directory() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create only an empty directory
        fs::create_dir(temp_dir.path().join("empty_dir")).await.unwrap();
        
        let tree_tool = TreeTool {
            path: "empty_dir".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should contain summary showing 0 files
            assert!(content.contains("0 files"));
            assert!(content.contains("0 directories"));
        }
    }

    #[tokio::test]
    async fn test_tree_nonexistent_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let tree_tool = TreeTool {
            path: "nonexistent".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:tree"));
        assert!(error_str.contains("not found"));
    }

    #[tokio::test]
    async fn test_tree_path_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let tree_tool = TreeTool {
            path: "../outside".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: false, // Disable symlink following to test security
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:tree"));
        assert!(error_str.contains("not found") || error_str.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_tree_invalid_pattern() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: Some("[invalid".to_string()), // Invalid glob pattern
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:tree"));
        assert!(error.to_string().contains("pattern") || error.to_string().contains("glob"));
    }

    #[tokio::test]
    async fn test_tree_file_sizes() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create files with known sizes
        let large_content = "x".repeat(1024); // 1KB
        fs::write(temp_dir.path().join("large.txt"), &large_content).await.unwrap();
        fs::write(temp_dir.path().join("small.txt"), "small").await.unwrap();
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;

            
            // Should show file sizes
            assert!(content.contains("(1.0 KiB)") || content.contains("(1024 B)"));
            assert!(content.contains("(5 B)"));
        }
    }
    
    #[tokio::test]
    async fn test_tree_json_output() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_structure(temp_dir.path()).await;
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: Some(2),
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: Some("json".to_string()),
            max_files: None,
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should be valid JSON
            let parsed: serde_json::Value = serde_json::from_str(content).expect("Invalid JSON");
            
            // Should have root and stats
            assert!(parsed.get("root").is_some());
            assert!(parsed.get("stats").is_some());
            
            // Check stats structure
            let stats = parsed.get("stats").unwrap();
            assert!(stats.get("total_directories").is_some());
            assert!(stats.get("total_files").is_some());
            assert!(stats.get("total_size").is_some());
            assert!(stats.get("total_size_human").is_some());
            
            // Check root structure
            let root = parsed.get("root").unwrap();
            assert!(root.get("name").is_some());
            assert!(root.get("type").is_some());
            assert_eq!(root.get("type").unwrap().as_str().unwrap(), "directory");
            assert!(root.get("children").is_some());
        }
    }
    
    #[tokio::test]
    async fn test_tree_max_files_limit() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create many files
        for i in 0..10 {
            fs::write(temp_dir.path().join(format!("file{}.txt", i)), "content").await.unwrap();
        }
        
        let tree_tool = TreeTool {
            path: ".".to_string(),
            max_depth: None,
            show_hidden: false,
            dirs_only: false,
            pattern_filter: None,
            follow_symlinks: true,
            output_format: None,
            max_files: Some(5),
        };
        
        let result = tree_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            
            // Should show warning about omitted files
            assert!(content.contains("files omitted"));
            assert!(content.contains("max_files limit"));
            
            // Count actual file entries (look for .txt files in tree structure)
            let file_count = content.lines()
                .filter(|line| line.contains(".txt") && (line.contains("├──") || line.contains("└──")))
                .count();
            assert_eq!(file_count, 5); // Should only show 5 files
        }
    }
}
