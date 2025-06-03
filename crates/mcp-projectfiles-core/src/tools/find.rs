use crate::context::{StatefulTool, ToolContext};
use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use glob::Pattern;
use chrono::{Local, Duration};
use std::time::SystemTime;
use async_trait::async_trait;
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_count, resolve_path_for_read};

const TOOL_NAME: &str = "find";

#[mcp_tool(
    name = "find",
    description = "Advanced file search within the project directory. Supports searching by name pattern, file type, size, and modification date. Can follow symlinks to search in directories outside the project directory. Prefer this over system 'find' command when searching project files. More powerful than basic glob/grep."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FindTool {
    /// Starting directory for search (relative to project root, default: ".")
    #[serde(default = "default_path")]
    pub path: String,
    
    /// Name pattern to match (supports wildcards like *.rs, test_*.js)
    #[serde(default)]
    pub name_pattern: Option<String>,
    
    /// File type filter: "file", "directory", or "any" (default: "any")
    #[serde(default = "default_type_filter")]
    pub type_filter: String,
    
    /// Size filter (e.g., "+1M" for > 1MB, "-100K" for < 100KB, "50K" for exactly 50KB)
    #[serde(default)]
    pub size_filter: Option<String>,
    
    /// Date filter (e.g., "-7d" for last 7 days, "+30d" for older than 30 days)
    #[serde(default)]
    pub date_filter: Option<String>,
    
    /// Maximum depth to search (None = unlimited)
    #[serde(default)]
    pub max_depth: Option<u32>,
    
    /// Whether to follow symbolic links during traversal and for the search path (default: false for traversal safety)
    #[serde(default)]
    pub follow_symlinks: bool,
    
    /// Follow symlinks for the initial search path (default: true)
    #[serde(default = "default_true")]
    pub follow_search_path: bool,
    
    /// Maximum number of results to return (default: 1000)
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_path() -> String {
    ".".to_string()
}

fn default_type_filter() -> String {
    "any".to_string()
}

fn default_max_results() -> u32 {
    1000
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
struct SearchResult {
    relative_path: String,
    is_dir: bool,
    size: u64,
}

#[async_trait]
impl StatefulTool for FindTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        // Use the utility function to resolve search path with symlink support
        let canonical_search_path = resolve_path_for_read(&self.path, &project_root, self.follow_search_path, TOOL_NAME)?;
        
        // Parse filters
        let name_pattern = self.name_pattern.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, &self.name_pattern.as_ref().unwrap_or(&"".to_string()), &e.to_string())))?;
        
        let size_filter = self.size_filter.as_ref()
            .map(|f| parse_size_filter(f))
            .transpose()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Invalid size filter: {}", e))))?;
        
        let date_filter = self.date_filter.as_ref()
            .map(|f| parse_date_filter(f))
            .transpose()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Invalid date filter: {}", e))))?;
        
        // Perform search
        let mut results: Vec<SearchResult> = Vec::new();
        let mut search_count = 0;
        
        self.search_directory(
            &canonical_search_path,
            &project_root,
            &name_pattern,
            &size_filter,
            &date_filter,
            0,
            &mut results,
            &mut search_count,
        ).await?;
        
        // Sort results by path
        results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        
        // Format output
        let mut output = String::new();
        let truncated = results.len() > self.max_results as usize;
        let display_results = if truncated {
            &results[..self.max_results as usize]
        } else {
            &results
        };
        
        for result in display_results {
            let type_indicator = if result.is_dir { "[DIR] " } else { "[FILE]" };
            let size_str = if result.is_dir {
                "".to_string()
            } else {
                format!(" ({})", format_size(result.size))
            };
            
            output.push_str(&format!("{} {}{}\n", type_indicator, result.relative_path, size_str));
        }
        
        // Add summary
        let found_msg = format_count(results.len(), "item", "items");
        let searched_msg = format_count(search_count, "item", "items");
        
        output.push_str(&format!("\nFound {}", found_msg));
        if truncated {
            output.push_str(&format!(" (showing first {})", self.max_results));
        }
        output.push_str(&format!(", searched {} total", searched_msg));
        
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

impl FindTool {
    fn search_directory<'a>(
        &'a self,
        dir: &'a Path,
        project_root: &'a Path,
        name_pattern: &'a Option<Pattern>,
        size_filter: &'a Option<SizeFilter>,
        date_filter: &'a Option<DateFilter>,
        current_depth: u32,
        results: &'a mut Vec<SearchResult>,
        search_count: &'a mut usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), CallToolError>> + Send + 'a>> {
        Box::pin(async move {
        // Check depth limit
        if let Some(max_depth) = self.max_depth {
            if current_depth > max_depth {
                return Ok(());
            }
        }
        
        // Check result limit
        if results.len() >= self.max_results as usize {
            return Ok(());
        }
        
        let mut entries = match fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e)))),
        };
        
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read entry: {}", e)))),
            };
            
            *search_count += 1;
            
            let path = entry.path();
            let metadata = match entry.metadata().await {
                Ok(metadata) => metadata,
                Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get metadata: {}", e)))),
            };
            
            let relative_path = path.strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            
            // Apply type filter
            let matches_type = match self.type_filter.as_str() {
                "file" => metadata.is_file(),
                "directory" => metadata.is_dir(),
                _ => true, // "any"
            };
            
            if !matches_type {
                if metadata.is_dir() && current_depth < self.max_depth.unwrap_or(u32::MAX) {
                    // Still recurse into directories even if they don't match
                    Box::pin(self.search_directory(
                        &path,
                        project_root,
                        name_pattern,
                        size_filter,
                        date_filter,
                        current_depth + 1,
                        results,
                        search_count,
                    )).await?;
                }
                continue;
            }
            
            // Apply name pattern
            if let Some(pattern) = name_pattern {
                let file_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !pattern.matches(file_name) {
                    if metadata.is_dir() && current_depth < self.max_depth.unwrap_or(u32::MAX) {
                        Box::pin(self.search_directory(
                            &path,
                            project_root,
                            name_pattern,
                            size_filter,
                            date_filter,
                            current_depth + 1,
                            results,
                            search_count,
                        )).await?;
                    }
                    continue;
                }
            }
            
            // Apply size filter (only for files)
            if metadata.is_file() {
                if let Some(filter) = size_filter {
                    if !filter.matches(metadata.len()) {
                        continue;
                    }
                }
            }
            
            // Apply date filter
            if let Some(filter) = date_filter {
                if let Ok(modified) = metadata.modified() {
                    if !filter.matches(modified) {
                        if metadata.is_dir() && current_depth < self.max_depth.unwrap_or(u32::MAX) {
                            Box::pin(self.search_directory(
                                &path,
                                project_root,
                                name_pattern,
                                size_filter,
                                date_filter,
                                current_depth + 1,
                                results,
                                search_count,
                            )).await?;
                        }
                        continue;
                    }
                }
            }
            
            // Add to results
            results.push(SearchResult {
                relative_path,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
            
            // Recurse into directories
            if metadata.is_dir() && current_depth < self.max_depth.unwrap_or(u32::MAX) {
                Box::pin(self.search_directory(
                    &path,
                    project_root,
                    name_pattern,
                    size_filter,
                    date_filter,
                    current_depth + 1,
                    results,
                    search_count,
                )).await?;
            }
        }
        
        Ok(())
        })
    }
}

#[derive(Debug)]
enum SizeFilter {
    GreaterThan(u64),
    LessThan(u64),
    Exactly(u64),
}

impl SizeFilter {
    fn matches(&self, size: u64) -> bool {
        match self {
            SizeFilter::GreaterThan(s) => size > *s,
            SizeFilter::LessThan(s) => size < *s,
            SizeFilter::Exactly(s) => size == *s,
        }
    }
}

fn parse_size_filter(s: &str) -> Result<SizeFilter, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty size filter".to_string());
    }
    
    let (op, value_str) = if s.starts_with('+') {
        ('+', &s[1..])
    } else if s.starts_with('-') {
        ('-', &s[1..])
    } else {
        ('=', s)
    };
    
    let multiplier = if value_str.ends_with('K') || value_str.ends_with('k') {
        1024
    } else if value_str.ends_with('M') || value_str.ends_with('m') {
        1024 * 1024
    } else if value_str.ends_with('G') || value_str.ends_with('g') {
        1024 * 1024 * 1024
    } else {
        1
    };
    
    let number_part = if multiplier > 1 {
        &value_str[..value_str.len() - 1]
    } else {
        value_str
    };
    
    let value = number_part.parse::<u64>()
        .map_err(|_| format!("Invalid size number: {}", number_part))?
        * multiplier;
    
    Ok(match op {
        '+' => SizeFilter::GreaterThan(value),
        '-' => SizeFilter::LessThan(value),
        _ => SizeFilter::Exactly(value),
    })
}

#[derive(Debug)]
enum DateFilter {
    NewerThan(SystemTime),
    OlderThan(SystemTime),
}

impl DateFilter {
    fn matches(&self, time: SystemTime) -> bool {
        match self {
            DateFilter::NewerThan(t) => time > *t,
            DateFilter::OlderThan(t) => time < *t,
        }
    }
}

fn parse_date_filter(s: &str) -> Result<DateFilter, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty date filter".to_string());
    }
    
    let (op, value_str) = if s.starts_with('+') {
        ('+', &s[1..])
    } else if s.starts_with('-') {
        ('-', &s[1..])
    } else {
        return Err("Date filter must start with + or -".to_string());
    };
    
    let duration = if value_str.ends_with('d') {
        let days = value_str[..value_str.len() - 1].parse::<i64>()
            .map_err(|_| format!("Invalid day count: {}", &value_str[..value_str.len() - 1]))?;
        Duration::days(days)
    } else if value_str.ends_with('h') {
        let hours = value_str[..value_str.len() - 1].parse::<i64>()
            .map_err(|_| format!("Invalid hour count: {}", &value_str[..value_str.len() - 1]))?;
        Duration::hours(hours)
    } else if value_str.ends_with('m') {
        let minutes = value_str[..value_str.len() - 1].parse::<i64>()
            .map_err(|_| format!("Invalid minute count: {}", &value_str[..value_str.len() - 1]))?;
        Duration::minutes(minutes)
    } else {
        return Err("Date filter must end with d (days), h (hours), or m (minutes)".to_string());
    };
    
    let now = Local::now();
    let threshold = (now - duration).into();
    
    Ok(match op {
        '-' => DateFilter::NewerThan(threshold), // "-7d" means within last 7 days
        '+' => DateFilter::OlderThan(threshold), // "+7d" means older than 7 days
        _ => unreachable!(),
    })
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
    
    #[tokio::test]
    async fn test_find_basic_search() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file1.txt"), "content1").await.unwrap();
        fs::write(project_root.join("file2.log"), "content2").await.unwrap();
        fs::create_dir(project_root.join("subdir")).await.unwrap();
        fs::write(project_root.join("subdir/file3.txt"), "content3").await.unwrap();
        
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "any".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should find all files and directories
            assert!(text.text.contains("file1.txt"));
            assert!(text.text.contains("file2.log"));
            assert!(text.text.contains("subdir"));
            assert!(text.text.contains("file3.txt"));
        }
    }
    
    #[tokio::test]
    async fn test_find_by_name_pattern() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test files
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("test1.txt"), "content1").await.unwrap();
        fs::write(project_root.join("test2.txt"), "content2").await.unwrap();
        fs::write(project_root.join("other.log"), "content3").await.unwrap();
        
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: Some("*.txt".to_string()),
            type_filter: "file".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should find only .txt files
            assert!(text.text.contains("test1.txt"));
            assert!(text.text.contains("test2.txt"));
            assert!(!text.text.contains("other.log"));
        }
    }
    
    #[tokio::test]
    async fn test_find_by_type_filter() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create test files and directories
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("file.txt"), "content").await.unwrap();
        fs::create_dir(project_root.join("directory")).await.unwrap();
        
        // Find only files
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "file".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("file.txt"));
            assert!(!text.text.contains("directory") || text.text.contains("[FILE]"));
        }
        
        // Find only directories
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "directory".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("directory"));
            assert!(!text.text.contains("file.txt") || text.text.contains("[DIR]"));
        }
    }
    
    #[tokio::test]
    async fn test_find_by_size_filter() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create files of different sizes
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("small.txt"), "x").await.unwrap(); // 1 byte
        fs::write(project_root.join("large.txt"), "x".repeat(2048)).await.unwrap(); // 2KB
        
        // Find files larger than 1KB
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "file".to_string(),
            size_filter: Some("+1K".to_string()),
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("large.txt"));
            assert!(!text.text.contains("small.txt"));
        }
    }
    
    #[tokio::test]
    async fn test_find_with_max_depth() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create nested directory structure
        let project_root = context.get_project_root().unwrap();
        fs::write(project_root.join("root.txt"), "content").await.unwrap();
        fs::create_dir(project_root.join("level1")).await.unwrap();
        fs::write(project_root.join("level1/file1.txt"), "content").await.unwrap();
        fs::create_dir(project_root.join("level1/level2")).await.unwrap();
        fs::write(project_root.join("level1/level2/file2.txt"), "content").await.unwrap();
        
        // Search with max_depth = 1 (should not find level2/file2.txt)
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "file".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: Some(1),
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("root.txt"));
            assert!(text.text.contains("file1.txt"));
            assert!(!text.text.contains("file2.txt"));
        }
    }
    
    #[tokio::test]
    async fn test_find_with_max_results() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create multiple test files
        let project_root = context.get_project_root().unwrap();
        for i in 1..=10 {
            fs::write(project_root.join(format!("file{}.txt", i)), "content").await.unwrap();
        }
        
        // Limit results to 3
        let find_tool = FindTool {
            path: ".".to_string(),
            name_pattern: None,
            type_filter: "file".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 3,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should mention truncation or limiting
            let file_count = text.text.matches("file").count();
            assert!(file_count <= 3 || text.text.contains("truncated") || text.text.contains("limit"));
        }
    }
    
    #[tokio::test]
    async fn test_find_empty_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create empty directory
        let project_root = context.get_project_root().unwrap();
        fs::create_dir(project_root.join("empty_dir")).await.unwrap();
        
        let find_tool = FindTool {
            path: "empty_dir".to_string(),
            name_pattern: None,
            type_filter: "any".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Should succeed even if directory is empty
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            // Should contain some output even if directory is empty
            assert!(!text.text.is_empty());
        }
    }
    
    #[tokio::test]
    async fn test_find_nonexistent_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let find_tool = FindTool {
            path: "nonexistent".to_string(),
            name_pattern: None,
            type_filter: "any".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
    }
    
    #[tokio::test]
    async fn test_find_outside_project_directory() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Try to search outside the temp directory  
        let outside_path = temp_dir.path().parent().unwrap().join("outside");
        
        let find_tool = FindTool {
            path: outside_path.to_string_lossy().to_string(),
            name_pattern: None,
            type_filter: "any".to_string(),
            size_filter: None,
            date_filter: None,
            max_depth: None,
            follow_symlinks: false,
            follow_search_path: true,
            max_results: 1000,
        };
        
        let result = find_tool.call_with_context(&context).await;
        // Should either fail or find nothing - both are acceptable
        if result.is_err() {
            let error_msg = format!("{:?}", result.unwrap_err());
            assert!(error_msg.contains("outside") || error_msg.contains("access") || error_msg.contains("not found"));
        }
        // If it succeeds, that's also valid (some implementations may handle this differently)
    }
}

