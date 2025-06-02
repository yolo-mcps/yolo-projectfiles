use crate::config::tool_errors;
use std::path::{Path, PathBuf};
use rust_mcp_schema::schema_utils::CallToolError;
use crate::config::{get_project_root, is_within_project_root, normalize_path};

const TOOL_NAME: &str = "utils";

/// Get the project root with proper error handling for CallToolError
pub fn get_project_root_validated() -> Result<PathBuf, CallToolError> {
    get_project_root().map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &e)))
}

// The following functions are available for future use:

/// Validate that a path is within the project root and return the normalized absolute path
#[allow(dead_code)]
pub fn validate_path(path: &str) -> Result<PathBuf, CallToolError> {
    let absolute_path = normalize_path(path)
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &e)))?;
    
    if !is_within_project_root(&absolute_path)
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &e)))? {
        return Err(CallToolError::from(tool_errors::access_denied(
            TOOL_NAME,
            path,
            "Path is outside the project directory"
        )));
    }
    
    Ok(absolute_path)
}

/// Validate a path and ensure it exists
#[allow(dead_code)]
pub fn validate_existing_path(path: &str) -> Result<PathBuf, CallToolError> {
    let absolute_path = validate_path(path)?;
    
    if !absolute_path.exists() {
        return Err(CallToolError::from(tool_errors::file_not_found(
            TOOL_NAME,
            path
        )));
    }
    
    Ok(absolute_path)
}

/// Convert an absolute path to a relative path from the project root
#[allow(dead_code)]
pub fn to_relative_path(path: &Path) -> Result<PathBuf, CallToolError> {
    let project_root = get_project_root_validated()?;
    path.strip_prefix(&project_root)
        .map(|p| p.to_path_buf())
        .or_else(|_| Ok(path.to_path_buf()))
}

/// Format file size in human-readable format using binary units (KiB, MiB, GiB)
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    const THRESHOLD: f64 = 1024.0;
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        // For bytes, show as integer
        format!("{} B", bytes)
    } else {
        // For larger units, show with 1 decimal place
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format a count with proper singular/plural form
pub fn format_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {}", singular)
    } else {
        format!("{} {}", count, plural)
    }
}

/// Format multiple counts into a comma-separated string
pub fn format_counts(items: &[(usize, &str, &str)]) -> String {
    items
        .iter()
        .filter(|(count, _, _)| *count > 0)
        .map(|(count, singular, plural)| format_count(*count, singular, plural))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a path with single quotes, handling paths that contain quotes
pub fn format_path(path: &Path) -> String {
    let path_str = path.display().to_string();
    if path_str.contains('\'') || path_str.contains('"') {
        // If path contains quotes, use single quotes and escape them
        format!("'{}'", path_str.replace('\'', "\\'"))
    } else {
        format!("'{}'", path_str)
    }
}

/// Format a duration in human-readable format
pub fn format_duration(millis: u128) -> String {
    if millis < 1000 {
        format!("{}ms", millis)
    } else if millis < 60_000 {
        let seconds = millis as f64 / 1000.0;
        format!("{:.3}s", seconds)
    } else {
        let minutes = millis / 60_000;
        let seconds = (millis % 60_000) / 1000;
        format!("{}m {}s", minutes, seconds)
    }
}

/// Format a large number with comma separators
pub fn format_number(num: usize) -> String {
    let s = num.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let mut chars = s.chars().rev();
    
    for (i, c) in chars.by_ref().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(1048576), "1.0 MiB");
        assert_eq!(format_size(1073741824), "1.0 GiB");
    }

    #[test]
    fn test_format_count() {
        assert_eq!(format_count(0, "file", "files"), "0 files");
        assert_eq!(format_count(1, "file", "files"), "1 file");
        assert_eq!(format_count(2, "file", "files"), "2 files");
        assert_eq!(format_count(100, "file", "files"), "100 files");
    }

    #[test]
    fn test_format_counts() {
        assert_eq!(
            format_counts(&[(3, "file", "files"), (1, "directory", "directories")]),
            "3 files, 1 directory"
        );
        assert_eq!(
            format_counts(&[(0, "file", "files"), (5, "directory", "directories")]),
            "5 directories"
        );
    }

    #[test]
    fn test_format_path() {
        assert_eq!(format_path(Path::new("test.txt")), "'test.txt'");
        assert_eq!(format_path(Path::new("test's file.txt")), "'test\\'s file.txt'");
        assert_eq!(format_path(Path::new("test\"quote\".txt")), "'test\"quote\".txt'");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0ms");
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(1500), "1.500s");
        assert_eq!(format_duration(65000), "1m 5s");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }
}