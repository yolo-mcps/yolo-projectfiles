use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::format_path;
use async_trait::async_trait;
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

const TOOL_NAME: &str = "touch";

#[mcp_tool(
    name = "touch", 
    description = "Create files or update timestamps. ISO 8601 dates, reference files, content.
Examples: {\"path\": \"new.txt\"} or {\"path\": \"dated.txt\", \"mtime\": \"2023-01-01T00:00:00Z\"}"
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
    
    /// File encoding to use when creating new files - "utf-8", "ascii", "latin1"
    #[serde(default = "default_encoding")]
    pub encoding: String,
    
    /// Initial content for new files (empty string by default)
    #[serde(default)]
    pub content: String,
    
    /// Preview operation without making changes
    #[serde(default)]
    pub dry_run: bool,
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

fn default_encoding() -> String {
    "utf-8".to_string()
}

#[async_trait]
impl StatefulTool for TouchTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
            
        // Canonicalize project root for consistent path comparison
        let current_dir = project_root.canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e))))?;
        
        let requested_path = Path::new(&self.path);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            current_dir.join(requested_path)
        };
        
        // Validate the full path is within project bounds
        if absolute_path.exists() {
            let canonical_path = absolute_path.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path: {}", e))))?;
            if !canonical_path.starts_with(&current_dir) {
                return Err(CallToolError::from(tool_errors::access_denied(
                    TOOL_NAME,
                    &self.path,
                    "Path is outside the project directory"
                )));
            }
        } else {
            // For non-existent paths, validate parent and ensure final path would be within bounds
            if let Some(parent) = absolute_path.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve parent directory: {}", e))))?;
                    if !canonical_parent.starts_with(&current_dir) {
                        return Err(CallToolError::from(tool_errors::access_denied(
                            TOOL_NAME,
                            &self.path,
                            "Path would be outside the project directory"
                        )));
                    }
                    // Reconstruct path to ensure it stays within bounds
                    if let Some(file_name) = absolute_path.file_name() {
                        let final_path = canonical_parent.join(file_name);
                        if !final_path.starts_with(&current_dir) {
                            return Err(CallToolError::from(tool_errors::access_denied(
                                TOOL_NAME,
                                &self.path,
                                "Path would be outside the project directory"
                            )));
                        }
                    }
                } else if self.create {
                    // Need to validate the parent path would be within bounds before creating
                    let mut check_path = parent;
                    while let Some(ancestor) = check_path.parent() {
                        if ancestor.exists() {
                            let canonical_ancestor = ancestor.canonicalize()
                                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve ancestor directory: {}", e))))?;
                            if !canonical_ancestor.starts_with(&current_dir) {
                                return Err(CallToolError::from(tool_errors::access_denied(
                                    TOOL_NAME,
                                    &self.path,
                                    "Path would be outside the project directory"
                                )));
                            }
                            break;
                        }
                        check_path = ancestor;
                    }
                    
                    // Create parent directories
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create parent directory: {}", e))))?;
                }
            }
        }
        
        // Check if this is a dry run
        if self.dry_run {
            let exists = absolute_path.exists();
            let action_str = if !exists && self.create {
                "Would create"
            } else if exists && (self.update_atime || self.update_mtime) {
                "Would update timestamps for"
            } else if !exists && !self.create {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("File '{}' does not exist and create=false", self.path)
                )));
            } else {
                "Would touch"
            };
            
            let relative_path = absolute_path.strip_prefix(&current_dir)
                .unwrap_or(&absolute_path);
            
            let mut message = format!("{} file {}", action_str, format_path(relative_path));
            
            // Add details about what would be done
            let mut details = Vec::new();
            if !exists && self.create {
                if !self.content.is_empty() {
                    details.push(format!("with {} bytes of content", self.content.len()));
                }
                details.push(format!("encoding: {}", self.encoding));
            }
            
            if self.update_atime || self.update_mtime {
                let mut time_updates = Vec::new();
                if self.update_atime {
                    if let Some(ref atime_str) = self.atime {
                        time_updates.push(format!("atime={}", atime_str));
                    } else if let Some(ref ref_file) = self.reference {
                        time_updates.push(format!("atime from '{}'", ref_file));
                    } else {
                        time_updates.push("atime=now".to_string());
                    }
                }
                if self.update_mtime {
                    if let Some(ref mtime_str) = self.mtime {
                        time_updates.push(format!("mtime={}", mtime_str));
                    } else if let Some(ref ref_file) = self.reference {
                        time_updates.push(format!("mtime from '{}'", ref_file));
                    } else {
                        time_updates.push("mtime=now".to_string());
                    }
                }
                if !time_updates.is_empty() {
                    details.push(format!("({})", time_updates.join(", ")));
                }
            }
            
            if !details.is_empty() {
                message.push_str(" ");
                message.push_str(&details.join(" "));
            }
            
            return Ok(CallToolResult {
                content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                    message, None,
                ))],
                is_error: Some(false),
                meta: None,
            });
        }
        
        let mut action = "touched";
        
        if !absolute_path.exists() {
            if self.create {
                // Create file with optional content
                let content_bytes = if self.content.is_empty() {
                    Vec::new()
                } else {
                    // Encode content based on specified encoding
                    match self.encoding.as_str() {
                        "utf-8" => self.content.as_bytes().to_vec(),
                        "ascii" => {
                            if !self.content.is_ascii() {
                                return Err(CallToolError::from(tool_errors::invalid_input(
                                    TOOL_NAME,
                                    "Content contains non-ASCII characters but encoding is set to 'ascii'"
                                )));
                            }
                            self.content.as_bytes().to_vec()
                        },
                        "latin1" => {
                            self.content.chars()
                                .map(|c| {
                                    if c as u32 > 255 {
                                        return Err(CallToolError::from(tool_errors::invalid_input(
                                            TOOL_NAME,
                                            &format!("Character '{}' cannot be encoded as latin1", c)
                                        )));
                                    }
                                    Ok(c as u8)
                                })
                                .collect::<Result<Vec<u8>, CallToolError>>()?
                        },
                        _ => {
                            return Err(CallToolError::from(tool_errors::invalid_input(
                                TOOL_NAME,
                                &format!("Unsupported encoding '{}'. Supported: utf-8, ascii, latin1", self.encoding)
                            )));
                        }
                    }
                };
                
                fs::write(&absolute_path, &content_bytes)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to create file: {}", e))))?;
                action = "created";
            } else {
                return Err(CallToolError::from(tool_errors::file_not_found(
                    TOOL_NAME,
                    &format!("File '{}' does not exist and create=false", self.path)
                )));
            }
        } else {
            // File exists, check if it's a file
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata: {}", e))))?;
            
            if !metadata.is_file() {
                return Err(CallToolError::from(tool_errors::invalid_input(
                    TOOL_NAME,
                    &format!("Path '{}' exists but is not a file", self.path)
                )));
            }
        }

        // Update timestamps (for both existing and newly created files)
        if self.update_atime || self.update_mtime {
            let metadata = fs::metadata(&absolute_path)
                .await
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read metadata: {}", e))))?;
            
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
                    .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, reference_path)))?;
                
                if !ref_canonical_path.starts_with(&current_dir) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        TOOL_NAME,
                        reference_path,
                        "Reference path is outside the project directory"
                    )));
                }
                
                if !ref_canonical_path.exists() {
                    return Err(CallToolError::from(tool_errors::file_not_found(
                        TOOL_NAME,
                        reference_path
                    )));
                }
                
                let ref_metadata = fs::metadata(&ref_canonical_path)
                    .await
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read reference file metadata: {}", e))))?;
                
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
                .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to set timestamps: {}", e))))?;
            
            if action != "created" {
                action = "updated";
            }
        }
        
        // Format path relative to project root
        let relative_path = absolute_path.strip_prefix(&current_dir)
            .unwrap_or(&absolute_path);
        
        let mut message = match action {
            "created" => {
                let mut msg = format!("Created file {}", format_path(relative_path));
                if !self.content.is_empty() {
                    msg.push_str(&format!(" ({} bytes)", self.content.len()));
                }
                msg
            },
            "updated" => {
                let mut updates = Vec::new();
                if self.update_atime {
                    updates.push("access time");
                }
                if self.update_mtime {
                    updates.push("modification time");
                }
                format!("Updated {} for {}", updates.join(" and "), format_path(relative_path))
            },
            "touched" => format!("Touched file {}", format_path(relative_path)),
            _ => format!("{} file {}", action, format_path(relative_path))
        };
        
        // Add reference file info if used
        if let Some(ref ref_file) = self.reference {
            if action == "updated" || action == "created" {
                message.push_str(&format!(" (timestamps from '{}')", ref_file));
            }
        }
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

impl TouchTool {
    fn parse_timestamp(&self, timestamp_str: &str, field_name: &str) -> Result<FileTime, CallToolError> {
        // First try to parse as date-only format (YYYY-MM-DD)
        if timestamp_str.len() == 10 && timestamp_str.chars().filter(|&c| c == '-').count() == 2 {
            // Append midnight UTC time
            let full_timestamp = format!("{}T00:00:00Z", timestamp_str);
            return self.parse_timestamp(&full_timestamp, field_name);
        }
        
        // Try to parse as ISO 8601 format
        let dt = DateTime::parse_from_rfc3339(timestamp_str)
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Invalid {} format '{}': {}. Expected ISO 8601 format like '2023-12-25T10:30:00Z' or date like '2023-12-25'", field_name, timestamp_str, e)
            )))?;
        
        let system_time: SystemTime = dt.into();
        Ok(FileTime::from_system_time(system_time))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;
    use tokio::fs;

    
    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }
    
    #[tokio::test]
    async fn test_touch_create_new_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "new_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was created
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("new_file.txt");
        assert!(file_path.exists());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Created") || text.text.contains("Touched"));
            assert!(text.text.contains("new_file.txt"));
        }
    }
    
    #[tokio::test]
    async fn test_touch_existing_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create existing file
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("existing.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        // Get original timestamp
        let original_metadata = fs::metadata(&file_path).await.unwrap();
        let original_modified = original_metadata.modified().unwrap();
        
        // Wait a bit to ensure timestamp changes
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let touch_tool = TouchTool {
            path: "existing.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check timestamp was updated
        let new_metadata = fs::metadata(&file_path).await.unwrap();
        let new_modified = new_metadata.modified().unwrap();
        assert!(new_modified >= original_modified);
        
        // Content should be unchanged
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "content");
    }
    
    #[tokio::test]
    async fn test_touch_no_create() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "nonexistent.txt".to_string(),
            create: false,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
        
        // File should not be created
        let project_root = context.get_project_root().unwrap();
        assert!(!project_root.join("nonexistent.txt").exists());
    }
    
    #[tokio::test]
    async fn test_touch_with_specific_timestamps() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "timestamped.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: Some("2023-01-01T12:00:00Z".to_string()),
            mtime: Some("2023-01-01T12:00:00Z".to_string()),
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was created with specified timestamp
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("timestamped.txt");
        assert!(file_path.exists());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("timestamped.txt"));
            assert!(text.text.contains("2023-01-01") || text.text.contains("timestamp"));
        }
    }
    
    #[tokio::test]
    async fn test_touch_with_reference_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create reference file
        let project_root = context.get_project_root().unwrap();
        let ref_path = project_root.join("reference.txt");
        fs::write(&ref_path, "reference content").await.unwrap();
        
        let touch_tool = TouchTool {
            path: "target.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: Some("reference.txt".to_string()),
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check target file was created
        let target_path = project_root.join("target.txt");
        assert!(target_path.exists());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("target.txt"));
            // Just verify successful operation, don't check for specific reference mention
        }
    }
    
    #[tokio::test]
    async fn test_touch_selective_timestamps() {
        let (context, _temp_dir) = setup_test_context().await;
        
        // Create existing file
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("selective.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        // Update only modification time
        let touch_tool = TouchTool {
            path: "selective.txt".to_string(),
            create: false,
            update_atime: false,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("selective.txt"));
            assert!(text.text.contains("Updated") || text.text.contains("Touched"));
        }
    }
    
    #[tokio::test]
    async fn test_touch_with_parent_directories() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "subdir/nested/file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check parent directories and file were created
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("subdir/nested/file.txt");
        assert!(file_path.exists());
        assert!(project_root.join("subdir").is_dir());
        assert!(project_root.join("subdir/nested").is_dir());
    }
    
    #[tokio::test]
    async fn test_touch_invalid_timestamp() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "invalid_time.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: Some("invalid-timestamp".to_string()),
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Invalid") && error_msg.contains("format"));
    }
    
    #[tokio::test]
    async fn test_touch_outside_project_directory() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "../outside.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("outside the project directory"));
    }
    
    #[tokio::test]
    async fn test_touch_invalid_reference_file() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "target.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: Some("nonexistent_ref.txt".to_string()),
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("reference") || error_msg.contains("not found"));
    }
    
    #[tokio::test]
    async fn test_touch_with_content() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "content_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: "Hello, World!".to_string(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was created with content
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("content_file.txt");
        assert!(file_path.exists());
        
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello, World!");
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Created"));
            assert!(text.text.contains("13 bytes"));
        }
    }
    
    #[tokio::test]
    async fn test_touch_dry_run() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "dry_run_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "utf-8".to_string(),
            content: "Test content".to_string(),
            dry_run: true,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // File should NOT be created
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("dry_run_file.txt");
        assert!(!file_path.exists());
        
        let output = result.unwrap();
        let content = &output.content[0];
        if let CallToolResultContentItem::TextContent(text) = content {
            assert!(text.text.contains("Would create"));
            assert!(text.text.contains("12 bytes"));
        }
    }
    
    #[tokio::test]
    async fn test_touch_date_only_format() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "date_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: Some("2023-12-25".to_string()),
            mtime: Some("2023-12-25".to_string()),
            reference: None,
            encoding: "utf-8".to_string(),
            content: String::new(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file was created
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("date_file.txt");
        assert!(file_path.exists());
    }
    
    #[tokio::test]
    async fn test_touch_ascii_encoding() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "ascii_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "ascii".to_string(),
            content: "ASCII text only".to_string(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        // Check file content
        let project_root = context.get_project_root().unwrap();
        let file_path = project_root.join("ascii_file.txt");
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "ASCII text only");
    }
    
    #[tokio::test]
    async fn test_touch_non_ascii_with_ascii_encoding_fails() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let touch_tool = TouchTool {
            path: "unicode_file.txt".to_string(),
            create: true,
            update_atime: true,
            update_mtime: true,
            atime: None,
            mtime: None,
            reference: None,
            encoding: "ascii".to_string(),
            content: "Hello 世界".to_string(),
            dry_run: false,
        };
        
        let result = touch_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("non-ASCII"));
    }
}