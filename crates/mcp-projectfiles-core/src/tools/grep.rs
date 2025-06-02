use crate::context::{StatefulTool, ToolContext};
use crate::config::{get_project_root, format_tool_error};
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
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

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
        _context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = get_project_root()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &e)))?;
        
        let canonical_project_root = project_root.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e))))?;
        
        // Resolve search path
        let search_path = Path::new(&self.path);
        let absolute_search_path = if search_path.is_absolute() {
            search_path.to_path_buf()
        } else {
            project_root.join(search_path)
        };
        
        // Ensure search path exists and is within project directory
        if !absolute_search_path.exists() {
            return Err(CallToolError::unknown_tool(format_tool_error(
                TOOL_NAME, 
                &format!("Search path does not exist: {}", self.path)
            )));
        }
        
        let canonical_search_path = absolute_search_path.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to resolve path '{}': {}", self.path, e))))?;
        
        if !canonical_search_path.starts_with(&canonical_project_root) {
            return Err(CallToolError::unknown_tool(format_tool_error(
                TOOL_NAME,
                &format!("Access denied: Path '{}' is outside the project directory", self.path)
            )));
        }
        
        // Compile regex pattern
        let regex = RegexBuilder::new(&self.pattern)
            .case_insensitive(self.case_insensitive)
            .build()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Invalid regex pattern: {}", e))))?;
        
        // Compile glob patterns
        let include_pattern = self.include.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Invalid include pattern: {}", e))))?;
        
        let exclude_pattern = self.exclude.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Invalid exclude pattern: {}", e))))?;
        
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
        
        // Limit results
        if all_matches.len() > self.max_results as usize {
            all_matches.truncate(self.max_results as usize);
        }
        
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
                
                let relative_path = m.file_path.strip_prefix(&project_root)
                    .unwrap_or(&m.file_path);
                
                if self.show_line_numbers {
                    output.push_str(&format!("{}:{}: {}", relative_path.display(), m.line_number, m.line_content));
                } else {
                    output.push_str(&format!("{}: {}", relative_path.display(), m.line_content));
                }
                
                if i < all_matches.len() - 1 {
                    output.push('\n');
                }
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
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to read directory: {}", e))))?;
        
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => return Err(CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to read directory entry: {}", e)))),
            };
            
            let entry_path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => return Err(CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to get file type: {}", e)))),
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
                
                // Stop if we've hit the max results
                if all_matches.len() >= self.max_results as usize {
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
        let file = fs::File::open(file_path).await
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        
        // Check if file is binary by reading first 512 bytes
        let mut buffer = [0; 512];
        let mut file_for_check = fs::File::open(file_path).await
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        let bytes_read = file_for_check.read(&mut buffer).await
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        if bytes_read > 0 {
            let non_text_bytes = buffer[..bytes_read].iter()
                .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13) // Allow tab, LF, CR
                .count();
            
            if non_text_bytes > buffer.len() / 10 {
                return Err(CallToolError::unknown_tool(format_tool_error(TOOL_NAME, "Binary file detected")));
            }
        }
        
        // Read file line by line
        let file = fs::File::open(file_path).await
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut line_number = 0;
        
        while let Some(line) = lines.next_line().await
            .map_err(|e| CallToolError::unknown_tool(format_tool_error(TOOL_NAME, &format!("Failed to read line: {}", e))))? {
            line_number += 1;
            
            if regex.is_match(&line) {
                all_matches.push(Match {
                    file_path: file_path.to_path_buf(),
                    line_number,
                    line_content: line,
                    context_before: Vec::new(),
                    context_after: Vec::new(),
                });
                
                // Stop if we've hit the max results
                if all_matches.len() >= self.max_results as usize {
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