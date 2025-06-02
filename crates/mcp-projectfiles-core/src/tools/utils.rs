use std::path::{Path, PathBuf};
use rust_mcp_schema::schema_utils::CallToolError;
use crate::config::{get_project_root, is_within_project_root, normalize_path};

/// Get the project root with proper error handling for CallToolError
pub fn get_project_root_validated() -> Result<PathBuf, CallToolError> {
    get_project_root().map_err(|e| CallToolError::unknown_tool(e))
}

// The following functions are available for future use:

/// Validate that a path is within the project root and return the normalized absolute path
#[allow(dead_code)]
pub fn validate_path(path: &str) -> Result<PathBuf, CallToolError> {
    let absolute_path = normalize_path(path)
        .map_err(|e| CallToolError::unknown_tool(e))?;
    
    if !is_within_project_root(&absolute_path)
        .map_err(|e| CallToolError::unknown_tool(e))? {
        return Err(CallToolError::unknown_tool(format!(
            "Access denied: Path '{}' is outside the project directory",
            path
        )));
    }
    
    Ok(absolute_path)
}

/// Validate a path and ensure it exists
#[allow(dead_code)]
pub fn validate_existing_path(path: &str) -> Result<PathBuf, CallToolError> {
    let absolute_path = validate_path(path)?;
    
    if !absolute_path.exists() {
        return Err(CallToolError::unknown_tool(format!(
            "Path '{}' does not exist",
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