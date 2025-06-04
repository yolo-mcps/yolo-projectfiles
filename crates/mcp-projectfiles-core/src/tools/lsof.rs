use crate::config::tool_errors;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::process::Command;

use std::collections::HashSet;

const TOOL_NAME: &str = "lsof";

#[mcp_tool(
    name = "lsof",
    description = "List open files within the project directory. Preferred over system 'lsof' command.

IMPORTANT: Only shows files within the project directory for security.
NOTE: Omit optional parameters when not needed, don't pass null.

This tool helps identify:
- Which files are currently open by processes
- Lock files and temporary files in use
- Database files being accessed
- Log files being written to
- Which processes have files open in the project

Parameters:
- file_pattern: File pattern to match like \"*.log\", \"*.db\", \"*.lock\" (optional)
- process_filter: Filter by process name or PID like \"python\", \"13361\" (optional)
- include_all: Include all file types including sockets/pipes (optional, default: false)
- output_format: Output format - \"detailed\" (default), \"compact\", \"json\" (optional)
- sort_by: Sort results - \"path\" (default), \"process\", \"access\" (optional)

Examples:
- List all open files: {}
- Find open log files: {\"file_pattern\": \"*.log\"}
- Filter by process: {\"process_filter\": \"python\"}
- Compact output: {\"output_format\": \"compact\"}
- Sort by process: {\"sort_by\": \"process\"}
- Find database locks: {\"file_pattern\": \"*.db\"}
- Include everything: {\"include_all\": true}

Returns JSON with:
- total_found: Total open files found in project
- files: Array of open file details including:
  - file_path: Path to the open file
  - process_info: Information about the process (PID, name if available)
  - file_type: Type of file (file, directory, pipe, socket)
  - access_info: Access mode information if available

Platform support:
- macOS/Linux: Uses lsof command with enhanced field parsing
- Windows: Uses handle.exe if available, otherwise limited support"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct LsofTool {
    /// File pattern to match (optional, supports wildcards like '*.log', '*.db')
    pub file_pattern: Option<String>,
    
    /// Filter by process name or PID (optional)
    pub process_filter: Option<String>,
    
    /// Include all file types including sockets and pipes (default: false)
    pub include_all: Option<bool>,
    
    /// Output format: "detailed" (default), "compact", "json"
    pub output_format: Option<String>,
    
    /// Sort results by: "path" (default), "process", "access"
    pub sort_by: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct OpenFile {
    file_path: String,
    process_info: String,
    file_type: String,
    access_info: Option<String>,
    pid: u32,
    process_name: String,
}

impl LsofTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Get project root from current directory
        let project_root = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get current directory: {}", e)
            )))?;

        let include_all = self.include_all.unwrap_or(false);
        let output_format = self.output_format.as_deref().unwrap_or("detailed");
        let sort_by = self.sort_by.as_deref().unwrap_or("path");

        // Validate parameters
        if let Some(format) = &self.output_format {
            if !matches!(format.as_str(), "detailed" | "compact" | "json") {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    "output_format must be 'detailed', 'compact', or 'json'"
                )));
            }
        }

        if let Some(sort) = &self.sort_by {
            if !matches!(sort.as_str(), "path" | "process" | "access") {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    "sort_by must be 'path', 'process', or 'access'"
                )));
            }
        }

        // Get open files based on platform
        let mut open_files = {
            #[cfg(target_os = "windows")]
            {
                self.get_open_files_windows(&project_root)?
            }
            #[cfg(not(target_os = "windows"))]
            {
                self.get_open_files_unix(&project_root, include_all)?
            }
        };

        // Filter by file pattern if specified
        if let Some(pattern) = &self.file_pattern {
            open_files.retain(|f| matches_pattern(&f.file_path, pattern));
        }

        // Filter by process if specified
        if let Some(process_filter) = &self.process_filter {
            open_files.retain(|f| {
                f.process_name.to_lowercase().contains(&process_filter.to_lowercase()) ||
                f.pid.to_string() == *process_filter ||
                f.process_info.to_lowercase().contains(&process_filter.to_lowercase())
            });
        }

        // Sort files
        match sort_by {
            "process" => open_files.sort_by(|a, b| a.process_name.cmp(&b.process_name).then(a.pid.cmp(&b.pid))),
            "access" => open_files.sort_by(|a, b| {
                let a_access = a.access_info.as_deref().unwrap_or("");
                let b_access = b.access_info.as_deref().unwrap_or("");
                a_access.cmp(b_access).then(a.file_path.cmp(&b.file_path))
            }),
            _ => open_files.sort_by(|a, b| a.file_path.cmp(&b.file_path)), // Default: path
        }

        let total_found = open_files.len();

        // Format output based on requested format
        let result = match output_format {
            "compact" => self.format_compact_output(&open_files, total_found, &project_root),
            "json" => self.format_json_output(&open_files, total_found, &project_root),
            _ => self.format_detailed_output(&open_files, total_found, &project_root), // Default: detailed
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                result,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn get_open_files_unix(&self, project_root: &Path, include_all: bool) -> Result<Vec<OpenFile>, CallToolError> {
        let mut open_files = Vec::new();
        let mut seen_files = HashSet::new();

        // Use lsof to get all open files with enhanced field output
        let output = Command::new("lsof")
            .arg("+D")  // List all files under directory
            .arg(project_root)
            .arg("-F")  // Field output for easier parsing
            .arg("pctnaf") // Specific fields: pid, command, type, name, access, file descriptor
            .output()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to execute lsof: {}. Make sure lsof is installed.", e)
            )))?;

        // Note: lsof often returns non-zero exit codes even when successful
        // We'll just parse whatever output we get
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("command not found") {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "lsof command not found. Please install lsof."
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_pid = String::new();
        let mut current_command = String::new();
        let mut current_type = String::new();
        let mut current_access = String::new();
        let mut current_fd = String::new();

        for line in stdout.lines() {
            if line.is_empty() {
                continue;
            }

            let field_type = &line[0..1];
            let field_value = &line[1..];

            match field_type {
                "p" => current_pid = field_value.to_string(),
                "c" => current_command = field_value.to_string(),
                "t" => current_type = field_value.to_string(),
                "a" => current_access = field_value.to_string(),
                "f" => current_fd = field_value.to_string(),
                "n" => {
                    let file_path = field_value.to_string();
                    
                    // Skip if we've seen this file
                    if seen_files.contains(&file_path) {
                        continue;
                    }

                    // Skip non-regular files unless include_all
                    if !include_all && !is_regular_file(&current_type) {
                        continue;
                    }

                    // Ensure file is within project
                    if !is_path_in_project(&file_path, project_root) {
                        continue;
                    }

                    seen_files.insert(file_path.clone());
                    
                    let file_type = translate_file_type(&current_type);
                    let process_info = format!("{} (PID: {})", current_command, current_pid);
                    
                    // Determine access mode from file descriptor and access field
                    let access_info = determine_access_mode(&current_access, &current_fd);
                    
                    let pid = current_pid.parse::<u32>().unwrap_or(0);

                    open_files.push(OpenFile {
                        file_path,
                        process_info: process_info.clone(),
                        file_type,
                        access_info,
                        pid,
                        process_name: current_command.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(open_files)
    }

    #[cfg(target_os = "windows")]
    fn get_open_files_windows(&self, project_root: &Path) -> Result<Vec<OpenFile>, CallToolError> {
        // For Windows, we'll use a simpler approach
        // Try to find common lock files and files that might be in use
        let mut open_files = Vec::new();
        
        // Common patterns for files that might be locked/in use
        let patterns = vec![
            "*.lock",
            "*.lck", 
            "*.db",
            "*.db-journal",
            "*.log",
            ".git/index.lock",
            "node_modules/.cache",
            "target/debug/incremental",
        ];

        for pattern in patterns {
            if let Ok(entries) = glob::glob(&project_root.join(pattern).to_string_lossy()) {
                for entry in entries.flatten() {
                    if entry.starts_with(project_root) {
                        open_files.push(OpenFile {
                            file_path: entry.display().to_string(),
                            process_info: "Unknown (Windows limited support)".to_string(),
                            file_type: "file".to_string(),
                            access_info: None,
                            pid: 0,
                            process_name: "unknown".to_string(),
                        });
                    }
                }
            }
        }

        // Add a note about limited Windows support
        if open_files.is_empty() {
            open_files.push(OpenFile {
                file_path: "[No open files detected]".to_string(),
                process_info: "Note: Windows support is limited without handle.exe".to_string(),
                file_type: "info".to_string(),
                access_info: Some("Install handle.exe from Sysinternals for full support".to_string()),
                pid: 0,
                process_name: "system".to_string(),
            });
        }

        Ok(open_files)
    }

    // Output formatting methods
    fn format_detailed_output(&self, files: &[OpenFile], total: usize, project_root: &Path) -> String {
        let result = json!({
            "total_found": total,
            "files": files,
            "project_root": project_root.display().to_string(),
        });
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error formatting output".to_string())
    }

    fn format_compact_output(&self, files: &[OpenFile], total: usize, project_root: &Path) -> String {
        let mut output = format!("Found {} open files in {}\n\n", total, project_root.display());
        
        for file in files {
            let access = file.access_info.as_deref().unwrap_or("unknown");
            output.push_str(&format!("{} [{}] {} ({})\n", 
                file.file_path, 
                access,
                file.process_name,
                file.pid
            ));
        }
        
        output
    }

    fn format_json_output(&self, files: &[OpenFile], total: usize, project_root: &Path) -> String {
        let result = json!({
            "total_found": total,
            "files": files,
            "project_root": project_root.display().to_string(),
        });
        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
    }

}

// Helper functions
fn determine_access_mode(access_field: &str, fd_field: &str) -> Option<String> {
    // First try to use the access field if available
    if !access_field.is_empty() {
        return Some(translate_access_mode(access_field));
    }
    
    // Fall back to inferring from file descriptor
    if !fd_field.is_empty() {
        // Common file descriptor patterns
        match fd_field {
            "0" => Some("read".to_string()),      // stdin
            "1" | "2" => Some("write".to_string()), // stdout/stderr
            fd if fd.ends_with('r') => Some("read".to_string()),
            fd if fd.ends_with('w') => Some("write".to_string()),
            fd if fd.ends_with('u') => Some("read/write".to_string()),
            _ => {
                // Try to parse numeric file descriptors
                if let Ok(num) = fd_field.parse::<i32>() {
                    match num {
                        0 => Some("read".to_string()),
                        1 | 2 => Some("write".to_string()),
                        _ => Some("read/write".to_string()), // Default for other numeric FDs
                    }
                } else {
                    Some("unknown".to_string())
                }
            }
        }
    } else {
        None
    }
}
fn matches_pattern(text: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        // Convert glob pattern to regex
        let regex_pattern = pattern
            .replace('.', r"\.")
            .replace('*', ".*")
            .replace('?', ".");
        
        if let Ok(regex) = regex::Regex::new(&format!("(?i)^{}$", regex_pattern)) {
            return regex.is_match(text);
        }
    }
    
    // Fall back to case-insensitive contains
    text.to_lowercase().contains(&pattern.to_lowercase())
}

fn is_regular_file(file_type: &str) -> bool {
    matches!(file_type, "REG" | "DIR")
}

fn translate_file_type(lsof_type: &str) -> String {
    match lsof_type {
        "REG" => "file",
        "DIR" => "directory", 
        "PIPE" => "pipe",
        "FIFO" => "fifo",
        "SOCK" => "socket",
        "CHR" => "char_device",
        "BLK" => "block_device",
        _ => "other",
    }.to_string()
}

fn translate_access_mode(mode: &str) -> String {
    match mode {
        "r" => "read",
        "w" => "write",
        "u" => "read/write",
        _ => mode,
    }.to_string()
}

fn is_path_in_project(path: &str, project_root: &Path) -> bool {
    if let Ok(path) = Path::new(path).canonicalize() {
        if let Ok(project) = project_root.canonicalize() {
            return path.starts_with(&project);
        }
    }
    // If we can't canonicalize, check if it starts with project root
    Path::new(path).starts_with(project_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern("test.log", "*.log"));
        assert!(matches_pattern("error.log", "*.log"));
        assert!(!matches_pattern("test.txt", "*.log"));
        assert!(matches_pattern("database.db", "*.db"));
        assert!(matches_pattern("my-app.lock", "*.lock"));
        assert!(matches_pattern("/path/to/file.log", "*.log"));
    }

    #[test]
    fn test_file_type_translation() {
        assert_eq!(translate_file_type("REG"), "file");
        assert_eq!(translate_file_type("DIR"), "directory");
        assert_eq!(translate_file_type("PIPE"), "pipe");
        assert_eq!(translate_file_type("UNKNOWN"), "other");
    }
}