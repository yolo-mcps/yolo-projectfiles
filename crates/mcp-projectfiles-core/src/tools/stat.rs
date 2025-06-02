use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use chrono::{DateTime, Local};

#[mcp_tool(
    name = "stat",
    description = "Gets detailed file or directory metadata (size, permissions, timestamps) within the project directory. Replaces the need for 'ls -la' or 'stat' commands."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct StatTool {
    /// Path to get stats for (relative to project root)
    pub path: String,
    
    /// Whether to follow symbolic links (default: false)
    #[serde(default)]
    pub follow_symlinks: bool,
}

impl StatTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        // Canonicalize the path to resolve it
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path '{}': {}", self.path, e)))?;
        
        if !canonical_path.starts_with(&current_dir) {
            return Err(CallToolError::unknown_tool(format!(
                "Access denied: Path '{}' is outside the project directory",
                self.path
            )));
        }
        
        // Get metadata
        let metadata = if self.follow_symlinks {
            fs::metadata(&canonical_path).await
        } else {
            fs::symlink_metadata(&canonical_path).await
        }.map_err(|e| CallToolError::unknown_tool(format!("Failed to get metadata for '{}': {}", self.path, e)))?;
        
        // Build the result
        let mut result = serde_json::json!({
            "path": self.path,
            "absolute_path": canonical_path.display().to_string(),
            "exists": true,
            "type": get_file_type(&metadata),
            "size": metadata.len(),
            "is_file": metadata.is_file(),
            "is_dir": metadata.is_dir(),
            "is_symlink": metadata.is_symlink(),
            "readonly": metadata.permissions().readonly(),
        });
        
        // Add timestamps
        if let Ok(modified) = metadata.modified() {
            let modified_dt: DateTime<Local> = modified.into();
            result["modified"] = serde_json::Value::String(modified_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["modified_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(modified_dt.timestamp())
            );
        }
        
        if let Ok(accessed) = metadata.accessed() {
            let accessed_dt: DateTime<Local> = accessed.into();
            result["accessed"] = serde_json::Value::String(accessed_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["accessed_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(accessed_dt.timestamp())
            );
        }
        
        if let Ok(created) = metadata.created() {
            let created_dt: DateTime<Local> = created.into();
            result["created"] = serde_json::Value::String(created_dt.format("%Y-%m-%d %H:%M:%S").to_string());
            result["created_timestamp"] = serde_json::Value::Number(
                serde_json::Number::from(created_dt.timestamp())
            );
        }
        
        // Add Unix-specific metadata
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            result["mode"] = serde_json::Value::String(format!("{:o}", metadata.mode()));
            result["uid"] = serde_json::Value::Number(serde_json::Number::from(metadata.uid()));
            result["gid"] = serde_json::Value::Number(serde_json::Number::from(metadata.gid()));
            result["nlink"] = serde_json::Value::Number(serde_json::Number::from(metadata.nlink()));
            result["dev"] = serde_json::Value::Number(serde_json::Number::from(metadata.dev()));
            result["ino"] = serde_json::Value::Number(serde_json::Number::from(metadata.ino()));
            
            // Format permissions in human-readable form
            result["permissions"] = serde_json::Value::String(format_permissions(metadata.mode()));
        }
        
        // Format size in human-readable form
        result["size_human"] = serde_json::Value::String(format_size(metadata.len()));
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result)
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to serialize result: {}", e)))?,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

fn get_file_type(metadata: &std::fs::Metadata) -> &'static str {
    if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else if metadata.is_symlink() {
        "symlink"
    } else {
        "other"
    }
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    
    if size == 0 {
        return "0 B".to_string();
    }
    
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(unix)]
fn format_permissions(mode: u32) -> String {
    let mut perms = String::with_capacity(10);
    
    // File type
    perms.push(match mode & 0o170000 {
        0o140000 => 's', // socket
        0o120000 => 'l', // symlink
        0o100000 => '-', // regular file
        0o060000 => 'b', // block device
        0o040000 => 'd', // directory
        0o020000 => 'c', // character device
        0o010000 => 'p', // fifo
        _ => '?',
    });
    
    // Owner permissions
    perms.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o100 != 0 {
        if mode & 0o4000 != 0 { 's' } else { 'x' }
    } else {
        if mode & 0o4000 != 0 { 'S' } else { '-' }
    });
    
    // Group permissions
    perms.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o010 != 0 {
        if mode & 0o2000 != 0 { 's' } else { 'x' }
    } else {
        if mode & 0o2000 != 0 { 'S' } else { '-' }
    });
    
    // Other permissions
    perms.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o001 != 0 {
        if mode & 0o1000 != 0 { 't' } else { 'x' }
    } else {
        if mode & 0o1000 != 0 { 'T' } else { '-' }
    });
    
    perms
}