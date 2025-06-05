use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use glob::{Pattern, MatchOptions};
use chrono::{DateTime, Local};
use async_trait::async_trait;
use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::{format_count, format_path, resolve_path_for_read};

const TOOL_NAME: &str = "list";

#[mcp_tool(
    name = "list",
    description = "List directory contents with filtering, sorting, and metadata options.

Examples:
- {\"path\": \"src\", \"filter\": \"*.rs\"}
- {\"path\": \".\", \"recursive\": true, \"show_metadata\": true}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ListTool {
    /// Path to list files from
    pub path: String,
    
    /// Whether to list subdirectories recursively (default: false)
    #[serde(default)]
    pub recursive: bool,
    
    /// Filter pattern for files (e.g., "*.rs", "*.{js,ts}", "test_*")
    #[serde(default)]
    pub filter: Option<String>,
    
    /// Sort by: "name" (default), "size", "modified"
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    
    /// Whether to show hidden files (files starting with dot) (default: false)
    #[serde(default)]
    pub show_hidden: bool,
    
    /// Whether to include file metadata (size, permissions, modified time) (default: false)
    #[serde(default)]
    pub show_metadata: bool,
    
    /// Follow symlinks to list directories outside the project directory (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

fn default_sort_by() -> String {
    "name".to_string()
}

fn default_follow_symlinks() -> bool {
    true
}

#[derive(Debug)]
struct FileEntry {
    name: String,
    _path: PathBuf,
    is_dir: bool,
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    mode: u32,
}

#[async_trait]
impl StatefulTool for ListTool {
    async fn call_with_context(self, context: &ToolContext) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve path with symlink support
        let canonical_path = resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;
        
        let path = &canonical_path;

        if !path.is_dir() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path is not a directory: {}", self.path)
            )));
        }

        // Prepare glob pattern if provided
        let glob_pattern = self.filter.as_ref().map(|f| {
            Pattern::new(f)
                .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, f, &e.to_string())))
        }).transpose()?;

        let mut entries = if self.recursive {
            self.list_recursive(path, &project_root, &glob_pattern).await?
        } else {
            self.list_directory(path, &glob_pattern).await?
        };

        // Sort entries based on sort_by parameter
        match self.sort_by.as_str() {
            "name" => entries.sort_by(|a, b| a.name.cmp(&b.name)),
            "size" => entries.sort_by(|a, b| {
                // Directories first, then by size
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.size.cmp(&b.size),
                }
            }),
            "modified" => entries.sort_by(|a, b| a.modified.cmp(&b.modified)),
            _ => return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Invalid sort_by value '{}'. Use 'name', 'size', or 'modified'", self.sort_by)
            ))),
        }

        // Format output
        let mut output_lines = Vec::new();
        for entry in &entries {
            let line = if self.show_metadata {
                self.format_with_metadata(entry)?
            } else {
                self.format_simple(entry)
            };
            output_lines.push(line);
        }

        let listing = output_lines.join("\n");
        
        // Add summary
        let _file_count = entries.iter().filter(|e| !e.is_dir).count();
        let _dir_count = entries.iter().filter(|e| e.is_dir).count();
        
        let relative_path = canonical_path.strip_prefix(&project_root)
            .unwrap_or(&canonical_path);
        
        let summary = format!("\nListed {} in {}", 
            format_count(entries.len(), "item", "items"),
            format_path(relative_path)
        );
        
        let final_output = if !listing.is_empty() {
            format!("{}{}", listing, summary)
        } else {
            summary.trim_start().to_string()
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                final_output, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl ListTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::default();
        StatefulTool::call_with_context(self, &context).await
    }

    async fn list_directory(&self, path: &Path, glob_pattern: &Option<Pattern>) -> Result<Vec<FileEntry>, CallToolError> {
        let mut entries_stream = fs::read_dir(path)
            .await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;

        let mut entries = Vec::new();
        
        loop {
            let entry = match entries_stream.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory entry: {}", e)))),
            };
            
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files if not requested
            if !self.show_hidden && file_name.starts_with('.') {
                continue;
            }

            // Apply filter if provided
            if let Some(pattern) = glob_pattern {
                let match_options = MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: false,
                    require_literal_leading_dot: !self.show_hidden,
                };
                if !pattern.matches_with(&file_name, match_options) {
                    continue;
                }
            }

            let metadata = entry.metadata().await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata for '{}': {}", file_name, e))))?;

            entries.push(FileEntry {
                name: file_name,
                _path: entry.path(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get modified time: {}", e))))?,
                #[cfg(unix)]
                mode: {
                    use std::os::unix::fs::MetadataExt;
                    metadata.mode()
                },
            });
        }

        Ok(entries)
    }

    async fn list_recursive(&self, path: &Path, _project_root: &Path, glob_pattern: &Option<Pattern>) -> Result<Vec<FileEntry>, CallToolError> {
        let mut all_entries = Vec::new();
        let mut dirs_to_process = vec![path.to_path_buf()];

        while let Some(current_dir) = dirs_to_process.pop() {
            let mut entries_stream = fs::read_dir(&current_dir)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory '{}': {}", current_dir.display(), e))))?;

            loop {
                let entry = match entries_stream.next_entry().await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break,
                    Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory entry: {}", e)))),
                };
                
                let file_name = entry.file_name().to_string_lossy().to_string();
                
                // Skip hidden files if not requested
                if !self.show_hidden && file_name.starts_with('.') {
                    continue;
                }

                let metadata = entry.metadata().await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata for '{}': {}", file_name, e))))?;

                let entry_path = entry.path();
                
                // For recursive listing, we want to show relative paths from the starting directory
                let relative_path = entry_path.strip_prefix(path)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .to_string();

                // Apply filter to the relative path for recursive listings
                let should_include = if let Some(pattern) = glob_pattern {
                    let match_options = MatchOptions {
                        case_sensitive: true,
                        require_literal_separator: false,
                        require_literal_leading_dot: !self.show_hidden,
                    };
                    pattern.matches_with(&relative_path, match_options) || 
                    pattern.matches_with(&file_name, match_options)
                } else {
                    true
                };

                if metadata.is_dir() {
                    // Always include directories in the listing
                    all_entries.push(FileEntry {
                        name: relative_path,
                        _path: entry_path.clone(),
                        is_dir: true,
                        size: 0, // Directories don't have meaningful size
                        modified: metadata.modified()
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get modified time: {}", e))))?,
                        #[cfg(unix)]
                        mode: {
                            use std::os::unix::fs::MetadataExt;
                            metadata.mode()
                        },
                    });
                    
                    // Add to dirs to process for recursion
                    dirs_to_process.push(entry_path);
                } else if should_include {
                    all_entries.push(FileEntry {
                        name: relative_path,
                        _path: entry_path,
                        is_dir: false,
                        size: metadata.len(),
                        modified: metadata.modified()
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get modified time: {}", e))))?,
                        #[cfg(unix)]
                        mode: {
                            use std::os::unix::fs::MetadataExt;
                            metadata.mode()
                        },
                    });
                }
            }
        }

        Ok(all_entries)
    }

    fn format_simple(&self, entry: &FileEntry) -> String {
        let type_indicator = if entry.is_dir { "[DIR]" } else { "[FILE]" };
        format!("{} {}", type_indicator, entry.name)
    }

    fn format_with_metadata(&self, entry: &FileEntry) -> Result<String, CallToolError> {
        let type_indicator = if entry.is_dir { "[DIR]" } else { "[FILE]" };
        
        // Format size
        let size_str = if entry.is_dir {
            "-".to_string()
        } else {
            format_size(entry.size)
        };

        // Format modified time
        let modified_datetime: DateTime<Local> = entry.modified.into();
        let modified_str = modified_datetime.format("%Y-%m-%d %H:%M:%S").to_string();

        // Format permissions (Unix only)
        #[cfg(unix)]
        let perms_str = format_permissions(entry.mode);
        #[cfg(not(unix))]
        let perms_str = "-".to_string();

        Ok(format!(
            "{} {:>10} {} {} {}",
            type_indicator,
            size_str,
            perms_str,
            modified_str,
            entry.name
        ))
    }
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size_f = size as f64;
    let mut unit_idx = 0;

    while size_f >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size_f /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size_f, UNITS[unit_idx])
    }
}

#[cfg(unix)]
fn format_permissions(mode: u32) -> String {
    let file_type = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };

    let user = format!(
        "{}{}{}",
        if mode & 0o400 != 0 { 'r' } else { '-' },
        if mode & 0o200 != 0 { 'w' } else { '-' },
        if mode & 0o100 != 0 { 'x' } else { '-' }
    );

    let group = format!(
        "{}{}{}",
        if mode & 0o040 != 0 { 'r' } else { '-' },
        if mode & 0o020 != 0 { 'w' } else { '-' },
        if mode & 0o010 != 0 { 'x' } else { '-' }
    );

    let other = format!(
        "{}{}{}",
        if mode & 0o004 != 0 { 'r' } else { '-' },
        if mode & 0o002 != 0 { 'w' } else { '-' },
        if mode & 0o001 != 0 { 'x' } else { '-' }
    );

    format!("{}{}{}{}", file_type, user, group, other)
}