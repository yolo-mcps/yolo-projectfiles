use std::path::{Path, PathBuf};
use std::sync::RwLock;

static PROJECT_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The name of this MCP server
pub const SERVER_NAME: &str = "projectfiles";

/// Format an error message with the server and tool name
pub fn format_tool_error(tool_name: &str, message: &str) -> String {
    format!("{}:{} - {}", SERVER_NAME, tool_name, message)
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
    
    #[test]
    fn test_project_root_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        init_project_root(temp_path.clone());
        
        let root = get_project_root().unwrap();
        assert_eq!(root, temp_path);
    }
    
    #[test]
    fn test_is_within_project_root() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        init_project_root(temp_path.clone());
        
        // Test path within project root
        let inner_path = temp_path.join("subdir");
        assert!(is_within_project_root(&inner_path).unwrap());
        
        // Test path outside project root
        let outside_path = PathBuf::from("/tmp/outside");
        assert!(!is_within_project_root(&outside_path).unwrap());
    }
}