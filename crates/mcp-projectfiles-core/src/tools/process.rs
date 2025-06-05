use crate::config::tool_errors;

use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};

const TOOL_NAME: &str = "process";

#[mcp_tool(
    name = "process",
    description = "Find processes and check port usage. Wildcards, sorting, full commands.
Examples: {} or {\"name_pattern\": \"*node*\"} or {\"check_ports\": [3000, 8080]}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct ProcessTool {
    /// Process name pattern to search for (optional, supports wildcards like '*node*' or 'postgres')
    pub name_pattern: Option<String>,

    /// Array of port numbers to check if they are in use (optional)
    pub check_ports: Option<Vec<u16>>,

    /// Maximum number of results to return (default: 50)
    pub max_results: Option<u32>,

    /// Include full command line in results (default: false)
    pub include_full_command: Option<bool>,

    /// Sort results by: "name" (default), "pid", "cpu", or "memory"
    pub sort_by: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProcessInfo {
    pid: u32,
    name: String,
    command: Option<String>,
    status: String,
    cpu_percent: Option<f32>,
    memory_mb: Option<f64>,
    user: Option<String>,
    start_time: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PortInfo {
    port: u16,
    protocol: String,
    pid: Option<u32>,
    process_name: Option<String>,
    status: String,
}

impl ProcessTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let max_results = self.max_results.unwrap_or(50) as usize;
        let include_full_command = self.include_full_command.unwrap_or(false);
        let sort_by = self.sort_by.as_deref().unwrap_or("name");

        // Validate sort_by parameter
        if !["name", "pid", "cpu", "memory"].contains(&sort_by) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!(
                    "Invalid sort_by value '{}'. Must be one of: name, pid, cpu, memory",
                    sort_by
                ),
            )));
        }

        let mut processes = Vec::new();
        let mut ports = Vec::new();

        // Get process information if name pattern is provided
        if let Some(pattern) = &self.name_pattern {
            processes = get_processes_by_pattern(pattern, max_results, include_full_command)?;
        }

        // Check port information if ports are provided
        if let Some(port_list) = &self.check_ports {
            ports = check_ports(port_list)?;
        }

        // If neither pattern nor ports provided, get all running processes (limited)
        if self.name_pattern.is_none() && self.check_ports.is_none() {
            processes = get_all_processes(max_results, include_full_command)?;
        }

        // Sort processes based on sort_by parameter
        sort_processes(&mut processes, sort_by);

        let result_json = serde_json::json!({
            "processes": processes,
            "ports": ports,
            "total_processes_found": processes.len(),
            "total_ports_checked": self.check_ports.as_ref().map(|p| p.len()).unwrap_or(0),
            "query": {
                "name_pattern": self.name_pattern,
                "check_ports": self.check_ports,
                "max_results": max_results,
                "include_full_command": include_full_command,
                "sort_by": sort_by
            }
        });

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result_json).map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to serialize result: {}", e),
                    ))
                })?,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn get_processes_by_pattern(
    pattern: &str,
    max_results: usize,
    include_full_command: bool,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    #[cfg(target_os = "macos")]
    {
        get_processes_macos(Some(pattern), max_results, include_full_command)
    }
    #[cfg(target_os = "linux")]
    {
        get_processes_linux(Some(pattern), max_results, include_full_command)
    }
    #[cfg(target_os = "windows")]
    {
        get_processes_windows(Some(pattern), max_results, include_full_command)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process monitoring not supported on this platform",
        )))
    }
}

fn get_all_processes(
    max_results: usize,
    include_full_command: bool,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    #[cfg(target_os = "macos")]
    {
        get_processes_macos(None, max_results, include_full_command)
    }
    #[cfg(target_os = "linux")]
    {
        get_processes_linux(None, max_results, include_full_command)
    }
    #[cfg(target_os = "windows")]
    {
        get_processes_windows(None, max_results, include_full_command)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Process monitoring not supported on this platform",
        )))
    }
}

#[cfg(target_os = "macos")]
fn get_processes_macos(
    pattern: Option<&str>,
    max_results: usize,
    include_full_command: bool,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    use std::process::Command;

    // Use ps command with specific format including user and start time
    let mut cmd = Command::new("ps");
    cmd.args(&["-axo", "pid,user,comm,%cpu,rss,stat,lstart"]);

    let output = cmd.output().map_err(|e| {
        CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!("Failed to execute ps command: {}", e),
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for (i, line) in stdout.lines().skip(1).enumerate() {
        // Skip header
        if i >= max_results {
            break;
        }

        // Parse carefully as lstart contains spaces
        let parts: Vec<&str> = line.trim().splitn(7, ' ').collect();
        if parts.len() >= 7 {
            let pid: u32 = parts[0].trim().parse().unwrap_or(0);
            let user = parts[1].trim().to_string();
            let name = parts[2].trim().to_string();
            let cpu: f32 = parts[3].trim().parse().unwrap_or(0.0);
            let memory_kb: f64 = parts[4].trim().parse().unwrap_or(0.0);
            let status = parts[5].trim().to_string();
            let start_time = parts[6].trim().to_string();

            // Apply pattern filter if provided
            if let Some(p) = pattern {
                if !matches_pattern(&name, p) {
                    continue;
                }
            }

            let command = if include_full_command {
                get_full_command_macos(pid).ok()
            } else {
                None
            };

            processes.push(ProcessInfo {
                pid,
                name,
                command,
                status,
                cpu_percent: Some(cpu),
                memory_mb: Some(memory_kb / 1024.0), // Convert KB to MB
                user: Some(user),
                start_time: Some(start_time),
            });
        }
    }

    Ok(processes)
}

#[cfg(target_os = "macos")]
fn get_full_command_macos(pid: u32) -> Result<String, std::io::Error> {
    use std::process::Command;

    let output = Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "args="])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Process not found",
        ))
    }
}

#[cfg(target_os = "linux")]
fn get_processes_linux(
    pattern: Option<&str>,
    max_results: usize,
    include_full_command: bool,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    use std::process::Command;

    let mut cmd = Command::new("ps");
    cmd.args(&["-axo", "pid,user,comm,%cpu,rss,stat,lstart"]);

    let output = cmd.output().map_err(|e| {
        CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!("Failed to execute ps command: {}", e),
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for (i, line) in stdout.lines().skip(1).enumerate() {
        if i >= max_results {
            break;
        }

        // Parse carefully as lstart contains spaces
        let parts: Vec<&str> = line.trim().splitn(7, ' ').collect();
        if parts.len() >= 7 {
            let pid: u32 = parts[0].trim().parse().unwrap_or(0);
            let user = parts[1].trim().to_string();
            let name = parts[2].trim().to_string();
            let cpu: f32 = parts[3].trim().parse().unwrap_or(0.0);
            let memory_kb: f64 = parts[4].trim().parse().unwrap_or(0.0);
            let status = parts[5].trim().to_string();
            let start_time = parts[6].trim().to_string();

            if let Some(p) = pattern {
                if !matches_pattern(&name, p) {
                    continue;
                }
            }

            let command = if include_full_command {
                get_full_command_linux(pid).ok()
            } else {
                None
            };

            processes.push(ProcessInfo {
                pid,
                name,
                command,
                status,
                cpu_percent: Some(cpu),
                memory_mb: Some(memory_kb / 1024.0),
                user: Some(user),
                start_time: Some(start_time),
            });
        }
    }

    Ok(processes)
}

#[cfg(target_os = "linux")]
fn get_full_command_linux(pid: u32) -> Result<String, std::io::Error> {
    use std::fs;

    let cmdline_path = format!("/proc/{}/cmdline", pid);
    let cmdline = fs::read_to_string(cmdline_path)?;

    // /proc/pid/cmdline uses null bytes as separators
    let command = cmdline.replace('\0', " ").trim().to_string();
    Ok(command)
}

#[cfg(target_os = "windows")]
fn get_processes_windows(
    pattern: Option<&str>,
    max_results: usize,
    include_full_command: bool,
) -> Result<Vec<ProcessInfo>, CallToolError> {
    use std::process::Command;

    let mut cmd = Command::new("wmic");
    cmd.args(&[
        "process",
        "get",
        "ProcessId,Name,PageFileUsage,WorkingSetSize",
        "/format:csv",
    ]);

    let output = cmd.output().map_err(|e| {
        CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            &format!("Failed to execute wmic command: {}", e),
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for (i, line) in stdout.lines().skip(1).enumerate() {
        if i >= max_results {
            break;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 4 {
            let name = parts[1].trim().to_string();
            if name.is_empty() {
                continue;
            }

            let pid: u32 = parts[3].trim().parse().unwrap_or(0);
            let memory_bytes: f64 = parts[4].trim().parse().unwrap_or(0.0);

            if let Some(p) = pattern {
                if !matches_pattern(&name, p) {
                    continue;
                }
            }

            let command = if include_full_command {
                get_full_command_windows(pid).ok()
            } else {
                None
            };

            processes.push(ProcessInfo {
                pid,
                name,
                command,
                status: "running".to_string(), // Windows doesn't easily provide status
                cpu_percent: None,             // Would need more complex WMI queries
                memory_mb: Some(memory_bytes / 1024.0 / 1024.0), // Convert bytes to MB
                user: None,                    // Would need WMI query for user info
                start_time: None,              // Would need WMI query for start time
            });
        }
    }

    Ok(processes)
}

#[cfg(target_os = "windows")]
fn get_full_command_windows(pid: u32) -> Result<String, std::io::Error> {
    use std::process::Command;

    let output = Command::new("wmic")
        .args(&[
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "CommandLine",
            "/format:value",
        ])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("CommandLine=") {
                return Ok(line[12..].to_string());
            }
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Command not found",
    ))
}

fn check_ports(ports: &[u16]) -> Result<Vec<PortInfo>, CallToolError> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        check_ports_unix(ports)
    }
    #[cfg(target_os = "windows")]
    {
        check_ports_windows(ports)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(CallToolError::from(tool_errors::invalid_input(
            TOOL_NAME,
            "Port checking not supported on this platform",
        )))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn check_ports_unix(ports: &[u16]) -> Result<Vec<PortInfo>, CallToolError> {
    use std::process::Command;

    let mut port_info = Vec::new();

    for &port in ports {
        // Check TCP ports
        let tcp_output = Command::new("lsof")
            .args(&["-i", &format!("tcp:{}", port), "-n", "-P"])
            .output();

        // Check UDP ports
        let udp_output = Command::new("lsof")
            .args(&["-i", &format!("udp:{}", port), "-n", "-P"])
            .output();

        let mut found_tcp = false;
        let mut found_udp = false;

        if let Ok(output) = tcp_output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(process_info) = parse_lsof_output(&stdout) {
                    port_info.push(PortInfo {
                        port,
                        protocol: "tcp".to_string(),
                        pid: process_info.0,
                        process_name: process_info.1,
                        status: "listening".to_string(),
                    });
                    found_tcp = true;
                }
            }
        }

        if let Ok(output) = udp_output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(process_info) = parse_lsof_output(&stdout) {
                    port_info.push(PortInfo {
                        port,
                        protocol: "udp".to_string(),
                        pid: process_info.0,
                        process_name: process_info.1,
                        status: "listening".to_string(),
                    });
                    found_udp = true;
                }
            }
        }

        // If port not found in either protocol, mark as available
        if !found_tcp && !found_udp {
            port_info.push(PortInfo {
                port,
                protocol: "none".to_string(),
                pid: None,
                process_name: None,
                status: "available".to_string(),
            });
        }
    }

    Ok(port_info)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_lsof_output(output: &str) -> Option<(Option<u32>, Option<String>)> {
    for line in output.lines().skip(1) {
        // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let process_name = parts[0].to_string();
            let pid: u32 = parts[1].parse().ok()?;
            return Some((Some(pid), Some(process_name)));
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn check_ports_windows(ports: &[u16]) -> Result<Vec<PortInfo>, CallToolError> {
    use std::process::Command;

    let mut port_info = Vec::new();

    for &port in ports {
        let output = Command::new("netstat")
            .args(&["-ano", "-p", "TCP"])
            .output()
            .map_err(|e| {
                CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Failed to execute netstat: {}", e),
                ))
            })?;

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut found = false;

        for line in stdout.lines() {
            if line.contains(&format!(":{}", port)) && line.contains("LISTENING") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(pid_str) = parts.last() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        port_info.push(PortInfo {
                            port,
                            protocol: "tcp".to_string(),
                            pid: Some(pid),
                            process_name: get_process_name_by_pid_windows(pid).ok(),
                            status: "listening".to_string(),
                        });
                        found = true;
                        break;
                    }
                }
            }
        }

        if !found {
            port_info.push(PortInfo {
                port,
                protocol: "none".to_string(),
                pid: None,
                process_name: None,
                status: "available".to_string(),
            });
        }
    }

    Ok(port_info)
}

#[cfg(target_os = "windows")]
fn get_process_name_by_pid_windows(pid: u32) -> Result<String, std::io::Error> {
    use std::process::Command;

    let output = Command::new("tasklist")
        .args(&["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().next() {
            let parts: Vec<&str> = line.split(',').collect();
            if let Some(name) = parts.first() {
                return Ok(name.trim_matches('"').to_string());
            }
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Process name not found",
    ))
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

fn sort_processes(processes: &mut Vec<ProcessInfo>, sort_by: &str) {
    match sort_by {
        "name" => processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        "pid" => processes.sort_by_key(|p| p.pid),
        "cpu" => processes.sort_by(|a, b| {
            let cpu_a = a.cpu_percent.unwrap_or(0.0);
            let cpu_b = b.cpu_percent.unwrap_or(0.0);
            cpu_b
                .partial_cmp(&cpu_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "memory" => processes.sort_by(|a, b| {
            let mem_a = a.memory_mb.unwrap_or(0.0);
            let mem_b = b.memory_mb.unwrap_or(0.0);
            mem_b
                .partial_cmp(&mem_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => {} // Default to no sorting (shouldn't happen due to validation)
    }
}

