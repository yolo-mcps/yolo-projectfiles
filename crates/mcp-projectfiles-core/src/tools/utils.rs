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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Resolve a path within the project directory, optionally following symlinks
/// for read-only operations. This allows symlinks within the project to point
/// to content outside the project directory for reading purposes only.
pub fn resolve_path_for_read(
    path: &str,
    project_root: &Path,
    follow_symlinks: bool,
    tool_name: &str,
) -> Result<PathBuf, CallToolError> {
    let requested_path = Path::new(path);
    let absolute_path = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        project_root.join(requested_path)
    };

    // For symlink following, we need to check if any component in the path is a symlink
    if follow_symlinks && absolute_path.exists() {
        // Check if the path itself is a symlink
        if absolute_path.is_symlink() {
            match absolute_path.canonicalize() {
                Ok(target_path) => {
                    // Allow reading the symlink target even if it's outside the project
                    return Ok(target_path);
                }
                Err(_e) => {
                    return Err(CallToolError::from(tool_errors::file_not_found(
                        tool_name,
                        path
                    )));
                }
            }
        } else {
            // Check if any parent directory in the path is a symlink
            let mut current_path = PathBuf::new();
            for component in absolute_path.components() {
                current_path.push(component);
                if current_path.is_symlink() {
                    // Found a symlink in the path, canonicalize the full path
                    match absolute_path.canonicalize() {
                        Ok(target_path) => {
                            // Allow reading through symlinked directories
                            return Ok(target_path);
                        }
                        Err(_e) => {
                            return Err(CallToolError::from(tool_errors::file_not_found(
                                tool_name,
                                path
                            )));
                        }
                    }
                }
            }
        }
    }

    // For regular files or when not following symlinks, use standard validation
    
    // First check if the path is a symlink when follow_symlinks is false
    if !follow_symlinks && absolute_path.is_symlink() {
        return Err(CallToolError::from(crate::error::Error::symlink_access_denied(
            "projectfiles",
            tool_name,
            path
        )));
    }
    
    let canonical_path = absolute_path.canonicalize()
        .map_err(|_e| CallToolError::from(tool_errors::file_not_found(
            tool_name,
            path
        )))?;

    // Canonicalize project root for comparison
    let canonical_project_root = project_root.canonicalize()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(
            tool_name,
            &format!("Failed to canonicalize project root: {}", e)
        )))?;

    // Check if the resolved path is within the project directory
    if !canonical_path.starts_with(&canonical_project_root) {
        return Err(CallToolError::from(tool_errors::access_denied(
            tool_name,
            path,
            "Path is outside the project directory"
        )));
    }

    Ok(canonical_path)
}

/// Resolve a path for operations that need to check symlinks without following them
/// (like exists and stat tools). This allows checking if a symlink exists within
/// the project directory without following it to its target.
pub fn resolve_path_allowing_symlinks(
    path: &str,
    project_root: &Path,
    tool_name: &str,
) -> Result<PathBuf, CallToolError> {
    let requested_path = Path::new(path);
    let absolute_path = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        project_root.join(requested_path)
    };

    // For symlink checking, we need to validate the parent directory
    if let Some(parent) = absolute_path.parent() {
        // Check if parent exists and is within project
        if parent.exists() {
            let canonical_parent = parent.canonicalize()
                .map_err(|_e| CallToolError::from(tool_errors::file_not_found(
                    tool_name,
                    path
                )))?;
            
            let canonical_project_root = project_root.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                    tool_name,
                    &format!("Failed to canonicalize project root: {}", e)
                )))?;
            
            if !canonical_parent.starts_with(&canonical_project_root) {
                return Err(CallToolError::from(tool_errors::access_denied(
                    tool_name,
                    path,
                    "Path is outside the project directory"
                )));
            }
            
            // Parent is valid, return the absolute path (which may be a symlink)
            return Ok(absolute_path);
        }
    }
    
    // If parent doesn't exist or path has no parent, fall back to standard validation
    // This handles cases like checking if "." exists
    let canonical_project_root = project_root.canonicalize()
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(
            tool_name,
            &format!("Failed to canonicalize project root: {}", e)
        )))?;
    
    // For paths that might not exist yet, we need manual normalization
    let mut normalized = PathBuf::new();
    for component in absolute_path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => {
                normalized.push(component);
            }
        }
    }
    
    if !normalized.starts_with(&canonical_project_root) {
        return Err(CallToolError::from(tool_errors::access_denied(
            tool_name,
            path,
            "Path is outside the project directory"
        )));
    }
    
    Ok(absolute_path)
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