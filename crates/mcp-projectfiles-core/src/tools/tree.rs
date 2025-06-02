use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_count, format_path};
use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use glob::Pattern;

const TOOL_NAME: &str = "tree";

#[mcp_tool(
    name = "tree",
    description = "Displays directory structure as a tree visualization. Shows files and directories in a hierarchical format similar to the 'tree' command."
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
}

fn default_path() -> String {
    ".".to_string()
}

impl TreeTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Get project root and resolve path
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
        let target_path = current_dir.join(&self.path);
        
        // Security check - ensure path is within project directory
        let normalized_path = target_path
            .canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path: {}", e))))?;
            
        if !normalized_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory"
            )));
        }
        
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
        let relative_path = normalized_path.strip_prefix(&current_dir)
            .unwrap_or(&normalized_path);
        
        tree_output.push_str(&format!(
            "\nTree of {} - {}, {} ({})",
            format_path(relative_path),
            format_count(stats.directories, "directory", "directories"),
            format_count(stats.files, "file", "files"),
            format_size(stats.total_size)
        ));
        
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

#[derive(Default)]
struct TreeStats {
    directories: usize,
    files: usize,
    total_size: u64,
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



