use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

const TOOL_NAME: &str = "kill";

#[mcp_tool(
    name = "kill",
    description = "Safely terminates processes running within the project directory. IMPORTANT: Only processes with current working directory within the project directory can be killed for safety. Requires explicit confirmation (confirm=true) OR force mode (force=true). Can kill by PID, process name pattern, or combination. Prefer this over system 'kill' command when terminating project-related processes."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct KillTool {
    /// Process ID to kill (optional)
    pub pid: Option<u32>,
    
    /// Process name pattern to match (optional, supports wildcards like '*node*' or 'webpack')
    pub name_pattern: Option<String>,
    
    /// Signal to send (default: TERM). Valid values: TERM, KILL, INT, QUIT, USR1, USR2
    pub signal: Option<String>,
    
    /// Require confirmation by setting to true (default: false for safety)
    #[serde(default)]
    pub confirm: bool,
    
    /// Force termination without confirmation (default: false, overrides confirm)
    #[serde(default)]
    pub force: bool,
    
    /// Maximum number of processes to kill when using name_pattern (default: 10)
    pub max_processes: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProcessKillResult {
    pid: u32,
    name: String,
    working_directory: Option<String>,
    signal_sent: String,
    success: bool,
    error_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct KillSummary {
    processes_targeted: usize,
    processes_killed: usize,
    processes_failed: usize,
    signal_used: String,
    results: Vec<ProcessKillResult>,
    query: KillQuery,
}

#[derive(Serialize, Deserialize, Debug)]
struct KillQuery {
    pid: Option<u32>,
    name_pattern: Option<String>,
    signal: String,
    max_processes: u32,
}

#[async_trait]
impl StatefulTool for KillTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Safety check: require confirmation or force mode
        if !self.confirm && !self.force {
            return Err(CallToolError::from(tool_errors::operation_not_permitted(
                TOOL_NAME,
                "Process termination requires confirmation. Set confirm=true or force=true to proceed."
            )));
        }
        
        // Validate that either PID or name pattern is provided
        if self.pid.is_none() && self.name_pattern.is_none() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Either 'pid' or 'name_pattern' must be specified"
            )));
        }
        
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
        
        let signal = self.signal.as_deref().unwrap_or("TERM");
        let max_processes = self.max_processes.unwrap_or(10);
        
        // Validate signal
        let valid_signals = ["TERM", "KILL", "INT", "QUIT", "USR1", "USR2"];
        if !valid_signals.contains(&signal) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Invalid signal '{}'. Valid signals: {}", signal, valid_signals.join(", "))
            )));
        }
        
        // Find processes to kill
        let mut processes_to_kill = Vec::new();
        
        if let Some(pid) = self.pid {
            // Kill specific PID
            if let Some(process_info) = get_process_info(pid)? {
                if is_process_in_project_directory(&process_info.cwd, &project_root)? {
                    processes_to_kill.push(process_info);
                } else {
                    return Err(CallToolError::from(tool_errors::operation_not_permitted(
                        TOOL_NAME,
                        &format!("Process {} (working directory: {}) is not within project directory ({})", 
                            pid, 
                            process_info.cwd.unwrap_or_else(|| "unknown".to_string()),
                            project_root.display())
                    )));
                }
            } else {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("Process with PID {} not found", pid)
                )));
            }
        } else if let Some(pattern) = &self.name_pattern {
            // Find processes by name pattern
            processes_to_kill = find_processes_by_pattern(pattern, &project_root, max_processes)?;
            
            if processes_to_kill.is_empty() {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("No processes found matching pattern '{}' within project directory", pattern)
                )));
            }
        }
        
        // Kill the processes
        let mut results = Vec::new();
        let mut killed_count = 0;
        let mut failed_count = 0;
        
        for process in &processes_to_kill {
            let kill_result = kill_process(process.pid, signal);
            let success = kill_result.is_ok();
            
            if success {
                killed_count += 1;
            } else {
                failed_count += 1;
            }
            
            results.push(ProcessKillResult {
                pid: process.pid,
                name: process.name.clone(),
                working_directory: process.cwd.clone(),
                signal_sent: signal.to_string(),
                success,
                error_message: kill_result.err().map(|e| e.to_string()),
            });
        }
        
        let summary = KillSummary {
            processes_targeted: processes_to_kill.len(),
            processes_killed: killed_count,
            processes_failed: failed_count,
            signal_used: signal.to_string(),
            results,
            query: KillQuery {
                pid: self.pid,
                name_pattern: self.name_pattern,
                signal: signal.to_string(),
                max_processes,
            },
        };
        
        let result_json = serde_json::to_string_pretty(&summary)
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to serialize result: {}", e))))?;
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                result_json,
                None,
            ))],
            is_error: Some(failed_count > 0),
            meta: None,
        })
    }
}

#[derive(Debug)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cwd: Option<String>,
}

fn get_process_info(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    #[cfg(target_os = "macos")]
    {
        get_process_info_macos(pid)
    }
    #[cfg(target_os = "linux")]
    {
        get_process_info_linux(pid)
    }
    #[cfg(target_os = "windows")]
    {
        get_process_info_windows(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, "Process information not supported on this platform")))
    }
}

#[cfg(target_os = "macos")]
fn get_process_info_macos(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    // Get process name
    let name_output = Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get process name: {}", e))))?;
    
    if !name_output.status.success() {
        return Ok(None); // Process not found
    }
    
    let name = String::from_utf8_lossy(&name_output.stdout).trim().to_string();
    
    // Get working directory using lsof
    let cwd_output = Command::new("lsof")
        .args(&["-p", &pid.to_string(), "-d", "cwd", "-F", "n"])
        .output()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get process working directory: {}", e))))?;
    
    let cwd = if cwd_output.status.success() {
        let output = String::from_utf8_lossy(&cwd_output.stdout);
        // lsof output format: "ncwd_path", we want to extract the path part
        output.lines()
            .find(|line| line.starts_with('n'))
            .map(|line| line[1..].to_string())
    } else {
        None
    };
    
    Ok(Some(ProcessInfo { pid, name, cwd }))
}

#[cfg(target_os = "linux")]
fn get_process_info_linux(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    use std::fs;
    
    // Check if process exists
    let proc_dir = format!("/proc/{}", pid);
    if !std::path::Path::new(&proc_dir).exists() {
        return Ok(None);
    }
    
    // Get process name from /proc/pid/comm
    let name = fs::read_to_string(format!("/proc/{}/comm", pid))
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read process name: {}", e))))?
        .trim()
        .to_string();
    
    // Get working directory from /proc/pid/cwd symlink
    let cwd = fs::read_link(format!("/proc/{}/cwd", pid))
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()));
    
    Ok(Some(ProcessInfo { pid, name, cwd }))
}

#[cfg(target_os = "windows")]
fn get_process_info_windows(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    // Get process name using wmic
    let name_output = Command::new("wmic")
        .args(&["process", "where", &format!("ProcessId={}", pid), "get", "Name", "/format:value"])
        .output()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get process name: {}", e))))?;
    
    if !name_output.status.success() {
        return Ok(None);
    }
    
    let name = String::from_utf8_lossy(&name_output.stdout)
        .lines()
        .find(|line| line.starts_with("Name="))
        .map(|line| line[5..].to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    // Get working directory using wmic (this may not always work)
    let cwd_output = Command::new("wmic")
        .args(&["process", "where", &format!("ProcessId={}", pid), "get", "ExecutablePath", "/format:value"])
        .output();
    
    let cwd = if let Ok(output) = cwd_output {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find(|line| line.starts_with("ExecutablePath="))
                .and_then(|line| {
                    let exe_path = &line[15..];
                    std::path::Path::new(exe_path).parent().map(|p| p.to_string_lossy().to_string())
                })
        } else {
            None
        }
    } else {
        None
    };
    
    Ok(Some(ProcessInfo { pid, name, cwd }))
}

fn find_processes_by_pattern(
    pattern: &str, 
    project_root: &PathBuf, 
    max_processes: u32
) -> Result<Vec<ProcessInfo>, CallToolError> {
    #[cfg(target_os = "macos")]
    {
        find_processes_by_pattern_macos(pattern, project_root, max_processes)
    }
    #[cfg(target_os = "linux")]
    {
        find_processes_by_pattern_linux(pattern, project_root, max_processes)
    }
    #[cfg(target_os = "windows")]
    {
        find_processes_by_pattern_windows(pattern, project_root, max_processes)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME, "Process search not supported on this platform")))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn find_processes_by_pattern_unix(
    pattern: &str, 
    project_root: &PathBuf, 
    max_processes: u32
) -> Result<Vec<ProcessInfo>, CallToolError> {
    let output = Command::new("ps")
        .args(&["-axo", "pid,comm"])
        .output()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to list processes: {}", e))))?;
    
    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME,
            &format!("ps command failed: {}", String::from_utf8_lossy(&output.stderr))
        )));
    }
    
    let mut processes = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines().skip(1) { // Skip header
        if processes.len() >= max_processes as usize {
            break;
        }
        
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            let pid: u32 = parts[0].parse().unwrap_or(0);
            let name = parts[1].to_string();
            
            // Check if name matches pattern
            if matches_pattern(&name, pattern) {
                if let Ok(Some(process_info)) = get_process_info(pid) {
                    if is_process_in_project_directory(&process_info.cwd, project_root)? {
                        processes.push(process_info);
                    }
                }
            }
        }
    }
    
    Ok(processes)
}

#[cfg(target_os = "macos")]
fn find_processes_by_pattern_macos(
    pattern: &str, 
    project_root: &PathBuf, 
    max_processes: u32
) -> Result<Vec<ProcessInfo>, CallToolError> {
    find_processes_by_pattern_unix(pattern, project_root, max_processes)
}

#[cfg(target_os = "linux")]
fn find_processes_by_pattern_linux(
    pattern: &str, 
    project_root: &PathBuf, 
    max_processes: u32
) -> Result<Vec<ProcessInfo>, CallToolError> {
    find_processes_by_pattern_unix(pattern, project_root, max_processes)
}

#[cfg(target_os = "windows")]
fn find_processes_by_pattern_windows(
    pattern: &str, 
    project_root: &PathBuf, 
    max_processes: u32
) -> Result<Vec<ProcessInfo>, CallToolError> {
    let output = Command::new("wmic")
        .args(&["process", "get", "ProcessId,Name", "/format:csv"])
        .output()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to list processes: {}", e))))?;
    
    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(TOOL_NAME,
            &format!("wmic command failed: {}", String::from_utf8_lossy(&output.stderr))
        )));
    }
    
    let mut processes = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines().skip(1) { // Skip header
        if processes.len() >= max_processes as usize {
            break;
        }
        
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            let name = parts[1].trim().to_string();
            let pid: u32 = parts[2].trim().parse().unwrap_or(0);
            
            if !name.is_empty() && matches_pattern(&name, pattern) {
                if let Ok(Some(process_info)) = get_process_info(pid) {
                    if is_process_in_project_directory(&process_info.cwd, project_root)? {
                        processes.push(process_info);
                    }
                }
            }
        }
    }
    
    Ok(processes)
}

fn is_process_in_project_directory(
    process_cwd: &Option<String>, 
    project_root: &PathBuf
) -> Result<bool, CallToolError> {
    match process_cwd {
        Some(cwd) => {
            let cwd_path = PathBuf::from(cwd);
            Ok(cwd_path.starts_with(project_root))
        }
        None => {
            // If we can't determine the working directory, err on the side of caution
            Ok(false)
        }
    }
}

fn kill_process(pid: u32, signal: &str) -> Result<(), std::io::Error> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let signal_arg = match signal {
            "TERM" => "-TERM",
            "KILL" => "-KILL", 
            "INT" => "-INT",
            "QUIT" => "-QUIT",
            "USR1" => "-USR1",
            "USR2" => "-USR2",
            _ => "-TERM", // Default fallback
        };
        
        let output = Command::new("kill")
            .args(&[signal_arg, &pid.to_string()])
            .output()?;
        
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to kill process: {}", String::from_utf8_lossy(&output.stderr))
            ))
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Windows doesn't have the same signal concept, so we use taskkill
        let force_flag = if signal == "KILL" { "/F" } else { "" };
        let mut args = vec!["/PID", &pid.to_string()];
        if !force_flag.is_empty() {
            args.push(force_flag);
        }
        
        let output = Command::new("taskkill")
            .args(&args)
            .output()?;
        
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to kill process: {}", String::from_utf8_lossy(&output.stderr))
            ))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Process termination not supported on this platform"
        ))
    }
}

fn matches_pattern(text: &str, pattern: &str) -> bool {
    // Simple pattern matching - convert * to regex
    if pattern.contains('*') {
        let regex_pattern = pattern.replace('*', ".*");
        if let Ok(regex) = regex::Regex::new(&format!("(?i){}", regex_pattern)) {
            return regex.is_match(text);
        }
    }
    
    // Fall back to case-insensitive substring matching
    text.to_lowercase().contains(&pattern.to_lowercase())
}