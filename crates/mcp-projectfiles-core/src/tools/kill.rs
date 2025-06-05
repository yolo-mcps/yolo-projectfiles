use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::format_count;
use async_trait::async_trait;
use colored::*;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

const TOOL_NAME: &str = "kill";

#[mcp_tool(
    name = "kill",
    description = "Terminate processes in project directory. Signals, patterns, dry-run preview.
Examples: {\"pid\": 12345} or {\"name_pattern\": \"*webpack*\", \"dry_run\": true}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct KillTool {
    /// Process ID to kill (optional)
    pub pid: Option<u32>,

    /// Process name pattern to match (optional, supports wildcards like '*node*' or 'webpack')
    pub name_pattern: Option<String>,

    /// Signal to send (default: TERM). Valid values: TERM, KILL, INT, QUIT, USR1, USR2
    pub signal: Option<String>,

    /// Perform a dry run - show what would be killed without actually terminating (default: false)
    #[serde(default)]
    pub dry_run: bool,

    /// Maximum number of processes to kill when using name_pattern (default: 10)
    pub max_processes: Option<u32>,

    /// Show matching processes without detailed dry run output (default: false)
    #[serde(default)]
    pub preview_only: bool,

    /// Require explicit confirmation for dangerous operations (default: false)
    #[serde(default)]
    pub force_confirmation: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProcessKillResult {
    pid: u32,
    name: String,
    working_directory: Option<String>,
    signal_sent: String,
    success: bool,
    error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_line: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct KillSummary {
    processes_targeted: usize,
    processes_killed: usize,
    processes_failed: usize,
    signal_used: String,
    dry_run: bool,
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
        // Only require dry_run when user wants to be careful
        // By default, allow killing processes in the project directory
        // (Safety is already enforced by the project directory check)

        // Enhanced parameter validation
        if self.pid.is_none() && self.name_pattern.is_none() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Either 'pid' or 'name_pattern' must be specified. Example: {\"pid\": 12345} or {\"name_pattern\": \"*python*\"}",
            )));
        }

        // Validate mutual exclusivity of certain options
        if self.pid.is_some() && self.name_pattern.is_some() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Cannot specify both 'pid' and 'name_pattern'. Use one or the other.",
            )));
        }

        // Validate preview_only and dry_run combination
        if self.preview_only && self.dry_run {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "Cannot use both 'preview_only' and 'dry_run'. Use 'preview_only' for simple listing or 'dry_run' for detailed preview.",
            )));
        }

        let project_root = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        let signal = self.signal.as_deref().unwrap_or("TERM");
        let max_processes = self.max_processes.unwrap_or(10);

        // Enhanced signal validation
        let valid_signals = ["TERM", "KILL", "INT", "QUIT", "USR1", "USR2"];
        if !valid_signals.contains(&signal) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!(
                    "Invalid signal '{}'. Valid signals: {} - Use TERM for graceful shutdown, KILL for force kill, INT for interrupt",
                    signal,
                    valid_signals.join(", ")
                ),
            )));
        }

        // Validate PID format if provided
        if let Some(pid) = self.pid {
            if pid == 0 {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    "PID cannot be 0. Please provide a valid process ID.",
                )));
            }
        }

        // Validate max_processes
        if max_processes == 0 {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "max_processes must be greater than 0",
            )));
        }

        if max_processes > 50 {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                "max_processes cannot exceed 50 for safety. Use smaller batches for bulk operations.",
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
                        &format!(
                            "Process {} (working directory: {}) is not within project directory ({})",
                            pid,
                            process_info.cwd.unwrap_or_else(|| "unknown".to_string()),
                            project_root.display()
                        ),
                    )));
                }
            } else {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("Process with PID {} not found", pid),
                )));
            }
        } else if let Some(pattern) = &self.name_pattern {
            // Find processes by name pattern
            processes_to_kill = find_processes_by_pattern(pattern, &project_root, max_processes)?;

            if processes_to_kill.is_empty() {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!(
                        "No processes found matching pattern '{}' within project directory",
                        pattern
                    ),
                )));
            }
        }

        // Handle preview_only mode - just show matching processes
        if self.preview_only {
            let preview_result = json!({
                "preview_mode": true,
                "processes_found": processes_to_kill.len(),
                "matches": processes_to_kill.iter().map(|p| {
                    json!({
                        "pid": p.pid,
                        "name": p.name,
                        "working_directory": p.cwd,
                        "command_line": get_process_command_line(p.pid).unwrap_or_else(|_| "N/A".to_string())
                    })
                }).collect::<Vec<_>>(),
                "signal_would_send": signal,
                "note": "This is preview mode - no processes were terminated. Use dry_run=true for detailed execution preview or remove preview_only to execute."
            });

            return Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    serde_json::to_string_pretty(&preview_result)
                        .unwrap_or_else(|_| "Error formatting preview".to_string()),
                    None,
                ))],
                is_error: Some(false),
                meta: None,
            });
        }

        // Kill the processes (or simulate if dry run)
        let mut results = Vec::new();
        let mut killed_count = 0;
        let mut failed_count = 0;

        for process in &processes_to_kill {
            let (success, error_message) = if self.dry_run {
                // Dry run - don't actually kill
                (true, None)
            } else {
                // Actually kill the process
                let kill_result = kill_process(process.pid, signal);
                let success = kill_result.is_ok();

                if success {
                    killed_count += 1;
                } else {
                    failed_count += 1;
                }

                (success, kill_result.err().map(|e| e.to_string()))
            };

            // Get command line if available (for better process identification)
            let command_line = get_process_command_line(process.pid).ok();

            results.push(ProcessKillResult {
                pid: process.pid,
                name: process.name.clone(),
                working_directory: process.cwd.clone(),
                signal_sent: signal.to_string(),
                success,
                error_message,
                command_line,
            });
        }

        // For dry run, we don't actually kill anything
        if self.dry_run {
            killed_count = 0;
            failed_count = 0;
        }

        let summary = KillSummary {
            processes_targeted: processes_to_kill.len(),
            processes_killed: killed_count,
            processes_failed: failed_count,
            signal_used: signal.to_string(),
            dry_run: self.dry_run,
            results,
            query: KillQuery {
                pid: self.pid,
                name_pattern: self.name_pattern.clone(),
                signal: signal.to_string(),
                max_processes,
            },
        };

        // Format the response
        let mut response = String::new();

        if self.dry_run {
            response.push_str(&format!(
                "{} {} matching {}:\n\n",
                "[DRY RUN]".yellow().bold(),
                format_count(processes_to_kill.len(), "process", "processes"),
                if self.pid.is_some() {
                    format!("PID {}", self.pid.unwrap())
                } else {
                    format!("pattern '{}'", self.name_pattern.as_ref().unwrap())
                }
            ));

            // Show detailed process list for dry run
            for process in &summary.results {
                response.push_str(&format!(
                    "  {} {} (PID: {})\n",
                    "•".cyan(),
                    process.name.bold(),
                    process.pid
                ));

                if let Some(cwd) = &process.working_directory {
                    response.push_str(&format!("    Working directory: {}\n", cwd.dimmed()));
                }

                if let Some(cmd) = &process.command_line {
                    response.push_str(&format!("    Command: {}\n", cmd.dimmed()));
                }

                response.push_str(&format!("    Signal to send: {}\n", signal.yellow()));
                response.push('\n');
            }

            response.push_str(&format!(
                "\n{}\n",
                "No processes were terminated (dry run mode).".yellow()
            ));
            response.push_str("To actually terminate these processes, run the command again without dry_run=true.\n");
        } else {
            // Regular mode - show results (removed human-readable text to return pure JSON)
        }

        // Return JSON summary only (no prefix text)
        let result_json = serde_json::to_string_pretty(&summary).map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to serialize result: {}", e),
            ))
        })?;
        
        // For dry run, include the human-readable text before JSON
        if self.dry_run {
            response.push_str(&result_json);
        } else {
            // For regular mode, return only JSON for consistency
            response = result_json;
        }

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                response, None,
            ))],
            is_error: Some(failed_count > 0 && !self.dry_run),
            meta: None,
        })
    }
}

#[allow(dead_code)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cwd: Option<String>,
    command_line: Option<String>,
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
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process information not supported on this platform",
        )))
    }
}

fn get_process_command_line(pid: u32) -> Result<String, CallToolError> {
    #[cfg(target_os = "macos")]
    {
        get_process_command_line_macos(pid)
    }
    #[cfg(target_os = "linux")]
    {
        get_process_command_line_linux(pid)
    }
    #[cfg(target_os = "windows")]
    {
        get_process_command_line_windows(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Command line retrieval not supported on this platform",
        )))
    }
}

#[cfg(target_os = "macos")]
fn get_process_info_macos(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    // Get process name
    let name_output = Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get process name: {}", e),
            ))
        })?;

    if !name_output.status.success() {
        return Ok(None); // Process not found
    }

    let name = String::from_utf8_lossy(&name_output.stdout)
        .trim()
        .to_string();

    // Get working directory using lsof
    let cwd_output = Command::new("lsof")
        .args(&["-a", "-p", &pid.to_string(), "-d", "cwd", "-F", "n"])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get process working directory: {}", e),
            ))
        })?;

    let cwd = if cwd_output.status.success() {
        let output = String::from_utf8_lossy(&cwd_output.stdout);
        // lsof output format with -F flag:
        // p<pid>
        // fcwd
        // n<path>
        // We want the line starting with 'n' that follows our PID
        output
            .lines()
            .skip_while(|line| !line.starts_with(&format!("p{}", pid)))
            .find(|line| line.starts_with('n'))
            .map(|line| line[1..].to_string())
    } else {
        None
    };

    // Get command line
    let command_line = get_process_command_line_macos(pid).ok();

    Ok(Some(ProcessInfo {
        pid,
        name,
        cwd,
        command_line,
    }))
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
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read process name: {}", e),
            ))
        })?
        .trim()
        .to_string();

    // Get working directory from /proc/pid/cwd symlink
    let cwd = fs::read_link(format!("/proc/{}/cwd", pid))
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()));

    // Get command line
    let command_line = get_process_command_line_linux(pid).ok();

    Ok(Some(ProcessInfo {
        pid,
        name,
        cwd,
        command_line,
    }))
}

#[cfg(target_os = "windows")]
fn get_process_info_windows(pid: u32) -> Result<Option<ProcessInfo>, CallToolError> {
    // Get process name using wmic
    let name_output = Command::new("wmic")
        .args(&[
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "Name",
            "/format:value",
        ])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get process name: {}", e),
            ))
        })?;

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
        .args(&[
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "ExecutablePath",
            "/format:value",
        ])
        .output();

    let cwd = if let Ok(output) = cwd_output {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find(|line| line.starts_with("ExecutablePath="))
                .and_then(|line| {
                    let exe_path = &line[15..];
                    std::path::Path::new(exe_path)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                })
        } else {
            None
        }
    } else {
        None
    };

    // Get command line
    let command_line = get_process_command_line_windows(pid).ok();

    Ok(Some(ProcessInfo {
        pid,
        name,
        cwd,
        command_line,
    }))
}

#[cfg(target_os = "macos")]
fn get_process_command_line_macos(pid: u32) -> Result<String, CallToolError> {
    let output = Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get process command line: {}", e),
            ))
        })?;

    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process not found",
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn get_process_command_line_linux(pid: u32) -> Result<String, CallToolError> {
    use std::fs;

    // Read command line from /proc/pid/cmdline
    let cmdline = fs::read_to_string(format!("/proc/{}/cmdline", pid)).map_err(|e| {
        CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!("Failed to read process command line: {}", e),
        ))
    })?;

    // Replace null bytes with spaces
    Ok(cmdline.replace('\0', " ").trim().to_string())
}

#[cfg(target_os = "windows")]
fn get_process_command_line_windows(pid: u32) -> Result<String, CallToolError> {
    let output = Command::new("wmic")
        .args(&[
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "CommandLine",
            "/format:value",
        ])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get process command line: {}", e),
            ))
        })?;

    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process not found",
        )));
    }

    let cmdline = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.starts_with("CommandLine="))
        .map(|line| line[12..].to_string())
        .unwrap_or_else(|| "".to_string());

    Ok(cmdline)
}

fn find_processes_by_pattern(
    pattern: &str,
    project_root: &PathBuf,
    max_processes: u32,
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
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process search not supported on this platform",
        )))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn find_processes_by_pattern_unix(
    pattern: &str,
    project_root: &PathBuf,
    max_processes: u32,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    let output = Command::new("ps")
        .args(&["-axo", "pid,comm"])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to list processes: {}", e),
            ))
        })?;

    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!(
                "ps command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )));
    }

    let mut processes = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().skip(1) {
        // Skip header
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
    max_processes: u32,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    find_processes_by_pattern_unix(pattern, project_root, max_processes)
}

#[cfg(target_os = "linux")]
fn find_processes_by_pattern_linux(
    pattern: &str,
    project_root: &PathBuf,
    max_processes: u32,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    find_processes_by_pattern_unix(pattern, project_root, max_processes)
}

#[cfg(target_os = "windows")]
fn find_processes_by_pattern_windows(
    pattern: &str,
    project_root: &PathBuf,
    max_processes: u32,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    let output = Command::new("wmic")
        .args(&["process", "get", "ProcessId,Name", "/format:csv"])
        .output()
        .map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to list processes: {}", e),
            ))
        })?;

    if !output.status.success() {
        return Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!(
                "wmic command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )));
    }

    let mut processes = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().skip(1) {
        // Skip header
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
    project_root: &PathBuf,
) -> Result<bool, CallToolError> {
    match process_cwd {
        Some(cwd) => {
            let cwd_path = PathBuf::from(cwd);

            // Canonicalize both paths to handle symlinks properly
            let canonical_cwd = cwd_path.canonicalize().unwrap_or(cwd_path.clone());
            let canonical_root = project_root.canonicalize().unwrap_or(project_root.clone());

            Ok(canonical_cwd.starts_with(&canonical_root))
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
                format!(
                    "Failed to kill process: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
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

        let output = Command::new("taskkill").args(&args).output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to kill process: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Process termination not supported on this platform",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern() {
        // Exact match (case insensitive)
        assert!(matches_pattern("node", "node"));
        assert!(matches_pattern("Node", "node"));
        assert!(matches_pattern("NODE", "node"));

        // Substring match
        assert!(matches_pattern("node_process", "node"));
        assert!(matches_pattern("my_node_app", "node"));

        // Wildcard patterns
        assert!(matches_pattern("webpack-dev-server", "*webpack*"));
        assert!(matches_pattern("node_modules", "node*"));
        assert!(matches_pattern("test_runner", "*runner"));

        // No match
        assert!(!matches_pattern("python", "node"));
        assert!(!matches_pattern("ruby", "*node*"));
    }
}

