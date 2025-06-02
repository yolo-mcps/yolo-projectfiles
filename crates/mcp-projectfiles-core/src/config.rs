use std::path::{Path, PathBuf};
use std::sync::RwLock;

static PROJECT_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The name of this MCP server
pub const SERVER_NAME: &str = "projectfiles";

/// Format an error message with the server and tool name
pub fn format_tool_error(tool_name: &str, message: &str) -> String {
    format!("{}:{} - {}", SERVER_NAME, tool_name, message)
}

/// Create tool-specific errors with proper typing
pub mod tool_errors {
    use crate::error::Error;
    use super::SERVER_NAME;

    /// Create a file not found error for a tool
    pub fn file_not_found(tool: &str, path: &str) -> Error {
        Error::file_not_found(SERVER_NAME, tool, path)
    }

    /// Create an access denied error for a tool
    pub fn access_denied(tool: &str, path: &str, reason: &str) -> Error {
        Error::access_denied(SERVER_NAME, tool, path, reason)
    }

    /// Create an invalid input error for a tool
    pub fn invalid_input(tool: &str, message: &str) -> Error {
        Error::invalid_input(SERVER_NAME, tool, message)
    }

    /// Create a binary file error for a tool
    pub fn binary_file(tool: &str, path: &str) -> Error {
        Error::binary_file(SERVER_NAME, tool, path)
    }

    /// Create a pattern error for a tool
    pub fn pattern_error(tool: &str, pattern: &str, message: &str) -> Error {
        Error::pattern_error(SERVER_NAME, tool, pattern, message)
    }

    /// Create an encoding error for a tool
    pub fn encoding_error(tool: &str, path: &str, encoding: &str) -> Error {
        Error::encoding_error(SERVER_NAME, tool, path, encoding)
    }

    /// Create an operation not permitted error for a tool
    pub fn operation_not_permitted(tool: &str, message: &str) -> Error {
        Error::operation_not_permitted(SERVER_NAME, tool, message)
    }

    /// Create a limit exceeded error for a tool
    pub fn limit_exceeded(tool: &str, limit: &str, actual: &str) -> Error {
        Error::limit_exceeded(SERVER_NAME, tool, limit, actual)
    }
}

/// Initialize the project root directory
/// 
/// This should be called once at server startup. If not called,
/// the current working directory will be used as the default.
pub fn init_project_root(root: PathBuf) {
    let mut project_root = PROJECT_ROOT.write().unwrap();
    *project_root = Some(root);
}

/// Reset the project root (for testing purposes)
pub fn reset_project_root() {
    let mut project_root = PROJECT_ROOT.write().unwrap();
    *project_root = None;
}

/// Get the project root directory
/// 
/// Returns the configured project root, or the current working directory
/// if no root has been configured.
pub fn get_project_root() -> Result<PathBuf, String> {
    let project_root = PROJECT_ROOT.read().unwrap();
    if let Some(root) = project_root.as_ref() {
        Ok(root.clone())
    } else {
        drop(project_root); // Release the read lock before potentially writing
        
        // Check if MCP_PROJECT_ROOT environment variable is set
        if let Ok(env_root) = std::env::var("MCP_PROJECT_ROOT") {
            let path = PathBuf::from(env_root);
            if path.exists() && path.is_dir() {
                init_project_root(path.clone());
                return Ok(path);
            }
        }
        
        // Default to current working directory
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))
    }
}

/// Check if a path is within the project root
pub fn is_within_project_root(path: &Path) -> Result<bool, String> {
    let project_root = get_project_root()?;
    let canonical_root = project_root.canonicalize()
        .map_err(|e| format!("Failed to canonicalize project root: {}", e))?;
    
    // If the path doesn't exist yet, check its parent
    let canonical_path = if path.exists() {
        path.canonicalize()
            .map_err(|e| format!("Failed to canonicalize path: {}", e))?
    } else {
        // For non-existent paths, resolve the parent and append the filename
        if let Some(parent) = path.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize()
                    .map_err(|e| format!("Failed to canonicalize parent path: {}", e))?;
                if let Some(file_name) = path.file_name() {
                    canonical_parent.join(file_name)
                } else {
                    canonical_parent
                }
            } else {
                // If parent doesn't exist, just use the path as-is
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        }
    };
    
    Ok(canonical_path.starts_with(&canonical_root))
}

/// Normalize a path relative to the project root
pub fn normalize_path(path: &str) -> Result<PathBuf, String> {
    let project_root = get_project_root()?;
    let requested_path = Path::new(path);
    
    let absolute_path = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        project_root.join(requested_path)
    };
    
    Ok(absolute_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use serial_test::serial;
    
    #[test]
    #[serial]
    fn test_project_root_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        init_project_root(temp_path.clone());
        
        let root = get_project_root().unwrap();
        assert_eq!(root, temp_path);
        
        // Clean up
        reset_project_root();
    }
    
    #[test]
    #[serial]
    fn test_is_within_project_root() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        // Reset project root to ensure clean state and set test root
        reset_project_root();
        init_project_root(temp_path.clone());
        
        // Test path within project root
        let inner_path = temp_path.join("subdir");
        assert!(is_within_project_root(&inner_path).unwrap());
        
        // Test path outside project root - use the temp dir's parent to ensure it exists
        let outside_path = temp_path.parent().unwrap().join("outside");
        assert!(!is_within_project_root(&outside_path).unwrap());
        
        // Clean up
        reset_project_root();
    }
}