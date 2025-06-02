use crate::context::{StatefulTool, ToolContext};
use crate::config::{tool_errors, format_tool_error};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use regex::{Regex, RegexBuilder};
use glob::Pattern;
use tokio::io::AsyncReadExt;

const TOOL_NAME: &str = "grep";

#[mcp_tool(name = "grep", description = "Searches for patterns in text files within the project directory. Returns matching lines with context and file information. Supports regex patterns, file filtering, and customizable output.")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct GrepTool {
    /// Regular expression pattern to search for
    pub pattern: String,
    /// Directory to search in (relative to project root, default: ".")
    #[serde(default = "default_path")]
    pub path: String,
    /// File pattern to include (e.g., "*.rs", "*.txt")
    #[serde(default)]
    pub include: Option<String>,
    /// File pattern to exclude (e.g., "*.log", "*.tmp")
    #[serde(default)]
    pub exclude: Option<String>,
    /// Case insensitive search
    #[serde(default = "default_case_insensitive")]
    pub case_insensitive: bool,
    /// Show line numbers
    #[serde(default = "default_show_line_numbers")]
    pub show_line_numbers: bool,
    /// Lines of context before each match
    #[serde(default)]
    pub context_before: Option<u32>,
    /// Lines of context after each match
    #[serde(default)]
    pub context_after: Option<u32>,
    /// Maximum number of results to return
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_path() -> String {
    ".".to_string()
}

fn default_case_insensitive() -> bool {
    false
}

fn default_show_line_numbers() -> bool {
    true
}

fn default_max_results() -> u32 {
    100
}

#[derive(Debug, Clone)]
struct Match {
    file_path: PathBuf,
    line_number: usize,
    line_content: String,
    context_before: Vec<String>,
    context_after: Vec<String>,
}

#[async_trait]
impl StatefulTool for GrepTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::new(std::io::Error::new(std::io::ErrorKind::Other, format_tool_error(TOOL_NAME, &format!("Failed to get project root: {}", e)))))?;
        
        let canonical_project_root = project_root.canonicalize()
            .map_err(|e| CallToolError::new(std::io::Error::new(std::io::ErrorKind::Other, format_tool_error(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e)))))?;
        
        // Resolve search path
        let search_path = Path::new(&self.path);
        let absolute_search_path = if search_path.is_absolute() {
            search_path.to_path_buf()
        } else {
            project_root.join(search_path)
        };
        
        // Ensure search path exists and is within project directory
        if !absolute_search_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.path)));
        }
        
        let canonical_search_path = absolute_search_path.canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path '{}': {}", self.path, e))))?;
        
        if !canonical_search_path.starts_with(&canonical_project_root) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME, 
                &self.path, 
                "Path is outside the project directory"
            )));
        }
        
        // Compile regex pattern
        let regex = RegexBuilder::new(&self.pattern)
            .case_insensitive(self.case_insensitive)
            .build()
            .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, &self.pattern, &e.to_string())))?;
        
        // Compile glob patterns
        let include_pattern = self.include.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, self.include.as_ref().unwrap_or(&String::new()), &format!("Invalid include pattern: {}", e))))?;
        
        let exclude_pattern = self.exclude.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .map_err(|e| CallToolError::from(tool_errors::pattern_error(TOOL_NAME, self.exclude.as_ref().unwrap_or(&String::new()), &format!("Invalid exclude pattern: {}", e))))?;
        
        // Collect all matches
        let mut all_matches = Vec::new();
        let mut files_searched = 0;
        
        if canonical_search_path.is_file() {
            self.search_file(&canonical_search_path, &regex, &mut all_matches).await?;
            files_searched = 1;
        } else {
            self.search_directory(
                &canonical_search_path,
                &regex,
                &include_pattern,
                &exclude_pattern,
                &mut all_matches,
                &mut files_searched,
            ).await?;
        }
        
        // Check if results were limited
        let was_truncated = self.max_results > 0 && all_matches.len() == self.max_results as usize;
        
        // Format output
        let mut output = String::new();
        if all_matches.is_empty() {
            output.push_str(&format!("No matches found for pattern '{}' in {} files searched.", self.pattern, files_searched));
        } else {
            output.push_str(&format!("Found {} matches for pattern '{}' in {} files:\n\n", all_matches.len(), self.pattern, files_searched));
            
            for (i, m) in all_matches.iter().enumerate() {
                if i > 0 {
                    output.push_str("\n");
                }
                
                let relative_path = m.file_path.strip_prefix(&canonical_project_root)
                    .unwrap_or(&m.file_path);
                
                // Output context before
                for (ctx_idx, ctx_line) in m.context_before.iter().enumerate() {
                    let ctx_line_number = m.line_number - m.context_before.len() + ctx_idx;
                    if self.show_line_numbers {
                        output.push_str(&format!("{}:{}-\t{}\n", relative_path.display(), ctx_line_number, ctx_line));
                    } else {
                        output.push_str(&format!("{}: {}\n", relative_path.display(), ctx_line));
                    }
                }
                
                // Output the match line
                if self.show_line_numbers {
                    output.push_str(&format!("{}:{}:\t{}", relative_path.display(), m.line_number, m.line_content));
                } else {
                    output.push_str(&format!("{}: {}", relative_path.display(), m.line_content));
                }
                
                // Output context after
                for (ctx_idx, ctx_line) in m.context_after.iter().enumerate() {
                    let ctx_line_number = m.line_number + 1 + ctx_idx;
                    if self.show_line_numbers {
                        output.push_str(&format!("\n{}:{}-\t{}", relative_path.display(), ctx_line_number, ctx_line));
                    } else {
                        output.push_str(&format!("\n{}: {}", relative_path.display(), ctx_line));
                    }
                }
                
                if i < all_matches.len() - 1 {
                    output.push('\n');
                }
            }
            
            if was_truncated {
                output.push_str(&format!("\n\n[limited to {} results]", self.max_results));
            }
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                output, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl GrepTool {
    async fn search_directory(
        &self,
        dir_path: &Path,
        regex: &Regex,
        include_pattern: &Option<Pattern>,
        exclude_pattern: &Option<Pattern>,
        all_matches: &mut Vec<Match>,
        files_searched: &mut usize,
    ) -> Result<(), CallToolError> {
        let mut entries = fs::read_dir(dir_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;
        
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read directory entry: {}", e)))),
            };
            
            let entry_path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file type: {}", e)))),
            };
            
            if file_type.is_dir() {
                // Skip hidden directories
                if let Some(name) = entry_path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }
                
                // Recursively search subdirectories
                Box::pin(self.search_directory(
                    &entry_path,
                    regex,
                    include_pattern,
                    exclude_pattern,
                    all_matches,
                    files_searched,
                )).await?;
            } else if file_type.is_file() {
                // Check include/exclude patterns
                if let Some(file_name) = entry_path.file_name() {
                    let file_name_str = file_name.to_string_lossy();
                    
                    if let Some(include) = include_pattern {
                        if !include.matches(&file_name_str) {
                            continue;
                        }
                    }
                    
                    if let Some(exclude) = exclude_pattern {
                        if exclude.matches(&file_name_str) {
                            continue;
                        }
                    }
                }
                
                // Search the file
                self.search_file(&entry_path, regex, all_matches).await?;
                *files_searched += 1;
                
                // Stop if we've hit the max results (0 means no limit)
                if self.max_results > 0 && all_matches.len() >= self.max_results as usize {
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    async fn search_file(
        &self,
        file_path: &Path,
        regex: &Regex,
        all_matches: &mut Vec<Match>,
    ) -> Result<(), CallToolError> {
        // Quick binary file check
        let _file = fs::File::open(file_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        
        // Check if file is binary by reading first 512 bytes
        let mut buffer = [0; 512];
        let mut file_for_check = fs::File::open(file_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        let bytes_read = file_for_check.read(&mut buffer).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        if bytes_read > 0 {
            let non_text_bytes = buffer[..bytes_read].iter()
                .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13) // Allow tab, LF, CR
                .count();
            
            if non_text_bytes > buffer.len() / 10 {
                return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, "Binary file detected")));
            }
        }
        
        // Read all lines at once to support context
        let content = tokio::fs::read_to_string(&file_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        let all_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        
        for (line_idx, line) in all_lines.iter().enumerate() {
            let line_number = line_idx + 1;
            
            if regex.is_match(line) {
                // Collect context before
                let mut context_before = Vec::new();
                if let Some(before_count) = self.context_before {
                    let start_idx = line_idx.saturating_sub(before_count as usize);
                    for i in start_idx..line_idx {
                        context_before.push(all_lines[i].clone());
                    }
                }
                
                // Collect context after
                let mut context_after = Vec::new();
                if let Some(after_count) = self.context_after {
                    let end_idx = std::cmp::min(line_idx + 1 + after_count as usize, all_lines.len());
                    for i in (line_idx + 1)..end_idx {
                        context_after.push(all_lines[i].clone());
                    }
                }
                
                all_matches.push(Match {
                    file_path: file_path.to_path_buf(),
                    line_number,
                    line_content: line.clone(),
                    context_before,
                    context_after,
                });
                
                // Stop if we've hit the max results (0 means no limit)
                if self.max_results > 0 && all_matches.len() >= self.max_results as usize {
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let context = ToolContext::new();
        self.call_with_context(&context).await
    }
}