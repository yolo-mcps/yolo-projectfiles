use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use std::time::SystemTime;
use chrono::DateTime;
use filetime::{set_file_times, FileTime};

#[mcp_tool(
    name = "touch", 
    description = "Creates empty files or updates timestamps within the project directory only. Creates parent directories automatically if needed. Supports selective timestamp updates (access/modification time). File creation enabled by default (create=true)."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct TouchTool {
    /// Path to the file (relative to project root)
    pub path: String,
    /// Whether to create the file if it doesn't exist (default: true)
    #[serde(default = "default_create")]
    pub create: bool,
    /// Whether to update access time (default: true)
    #[serde(default = "default_update_atime")]
    pub update_atime: bool,
    /// Whether to update modification time (default: true)
    #[serde(default = "default_update_mtime")]
    pub update_mtime: bool,
    /// Specific access time to set (ISO 8601 format: "2023-12-25T10:30:00Z")
    /// If not provided, current time is used when update_atime is true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atime: Option<String>,
    /// Specific modification time to set (ISO 8601 format: "2023-12-25T10:30:00Z")
    /// If not provided, current time is used when update_mtime is true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<String>,
    /// Reference file to copy timestamps from (relative to project root)
    /// If provided, timestamps are copied from this file instead of using atime/mtime
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

fn default_create() -> bool {
    true
}

fn default_update_atime() -> bool {
    true
}

fn default_update_mtime() -> bool {
    true
}

impl TouchTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::unknown_tool(format!("Failed to get current directory: {}", e)))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        // Validate the full path is within project bounds
        if absolute_path.exists() {
            let canonical_path = absolute_path.canonicalize()
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve path: {}", e)))?;
            if !canonical_path.starts_with(&current_dir) {
                return Err(CallToolError::unknown_tool(format!(
                    "Access denied: Path '{}' is outside the project directory",
                    self.path
                )));
            }
        } else {
            // For non-existent paths, validate parent and ensure final path would be within bounds
            if let Some(parent) = absolute_path.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve parent directory: {}", e)))?;
                    if !canonical_parent.starts_with(&current_dir) {
                        return Err(CallToolError::unknown_tool(format!(
                            "Access denied: Path '{}' would be outside the project directory",
                            self.path
                        )));
                    }
                    // Reconstruct path to ensure it stays within bounds
                    if let Some(file_name) = absolute_path.file_name() {
                        let final_path = canonical_parent.join(file_name);
                        if !final_path.starts_with(&current_dir) {
                            return Err(CallToolError::unknown_tool(format!(
                                "Access denied: Path '{}' would be outside the project directory",
                                self.path
                            )));
                        }
                    }
                } else if self.create {
                    // Need to validate the parent path would be within bounds before creating
                    let mut check_path = parent;
                    while let Some(ancestor) = check_path.parent() {
                        if ancestor.exists() {
                            let canonical_ancestor = ancestor.canonicalize()
                                .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve ancestor directory: {}", e)))?;
                            if !canonical_ancestor.starts_with(&current_dir) {
                                return Err(CallToolError::unknown_tool(format!(
                                    "Access denied: Path '{}' would be outside the project directory",
                                    self.path
                                )));
                            }
                            break;
                        }
                        check_path = ancestor;
                    }
                    
                    // Create parent directories
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| CallToolError::unknown_tool(format!("Failed to create parent directory: {}", e)))?;
                }
            }
        }
        
        let mut action = "touched";
        
        if !absolute_path.exists() {
            if self.create {
                // Create empty file
                fs::write(&absolute_path, b"")
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to create file: {}", e)))?;
                action = "created";
            } else {
                return Err(CallToolError::unknown_tool(format!(
                    "File '{}' does not exist and create=false",
                    self.path
                )));
            }
        } else {
            // File exists, check if it's a file
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
            
            if !metadata.is_file() {
                return Err(CallToolError::unknown_tool(format!(
                    "Path '{}' exists but is not a file",
                    self.path
                )));
            }
        }

        // Update timestamps (for both existing and newly created files)
        if self.update_atime || self.update_mtime {
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to read metadata: {}", e)))?;
            
            let current_atime = FileTime::from_last_access_time(&metadata);
            let current_mtime = FileTime::from_last_modification_time(&metadata);
            
            // Handle reference file if provided
            let (ref_atime, ref_mtime) = if let Some(ref reference_path) = self.reference {
                let ref_absolute_path = if Path::new(reference_path).is_absolute() {
                    Path::new(reference_path).to_path_buf()
                } else {
                    current_dir.join(reference_path)
                };
                
                let ref_canonical_path = ref_absolute_path.canonicalize()
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to resolve reference path '{}': {}", reference_path, e)))?;
                
                if !ref_canonical_path.starts_with(&current_dir) {
                    return Err(CallToolError::unknown_tool(format!(
                        "Access denied: Reference path '{}' is outside the project directory",
                        reference_path
                    )));
                }
                
                if !ref_canonical_path.exists() {
                    return Err(CallToolError::unknown_tool(format!(
                        "Reference file not found: {}",
                        reference_path
                    )));
                }
                
                let ref_metadata = fs::metadata(&ref_canonical_path)
                    .await
                    .map_err(|e| CallToolError::unknown_tool(format!("Failed to read reference file metadata: {}", e)))?;
                
                (
                    FileTime::from_last_access_time(&ref_metadata),
                    FileTime::from_last_modification_time(&ref_metadata)
                )
            } else {
                (FileTime::now(), FileTime::now()) // Will be overridden by specific times if provided
            };
            
            let new_atime = if self.update_atime {
                if let Some(ref atime_str) = self.atime {
                    self.parse_timestamp(atime_str, "access time")?
                } else if self.reference.is_some() {
                    ref_atime
                } else {
                    FileTime::now()
                }
            } else {
                current_atime
            };
            
            let new_mtime = if self.update_mtime {
                if let Some(ref mtime_str) = self.mtime {
                    self.parse_timestamp(mtime_str, "modification time")?
                } else if self.reference.is_some() {
                    ref_mtime
                } else {
                    FileTime::now()
                }
            } else {
                current_mtime
            };
            
            set_file_times(&absolute_path, new_atime, new_mtime)
                .map_err(|e| CallToolError::unknown_tool(format!("Failed to set timestamps: {}", e)))?;
            
            if action != "created" {
                action = "updated";
            }
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("Successfully {} file '{}'", action, self.path), None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }

    fn parse_timestamp(&self, timestamp_str: &str, field_name: &str) -> Result<FileTime, CallToolError> {
        // Try to parse as ISO 8601 format
        let dt = DateTime::parse_from_rfc3339(timestamp_str)
            .map_err(|e| CallToolError::unknown_tool(format!(
                "Invalid {} format '{}': {}. Expected ISO 8601 format like '2023-12-25T10:30:00Z'",
                field_name, timestamp_str, e
            )))?;
        
        let system_time: SystemTime = dt.into();
        Ok(FileTime::from_system_time(system_time))
    }
}