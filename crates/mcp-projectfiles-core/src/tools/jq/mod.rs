use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use crate::tools::query_engine::{QueryEngine, QueryError};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JsonQueryError {
    #[error("Error: projectfiles:jq - File not found: {0}")]
    FileNotFound(String),
    
    #[error("Error: projectfiles:jq - Invalid JSON in file {file}: {error}")]
    InvalidJson { file: String, error: String },
    
    #[error("Error: projectfiles:jq - IO error: {0}")]
    IoError(String),
    
    #[error("Error: projectfiles:jq - {0}")]
    QueryEngine(#[from] QueryError),
}

#[mcp_tool(name = "jq", description = "Query and manipulate JSON files with jq syntax. Full jq features, read/write operations.
Examples: \".users | map(.email)\" or \".active = true\" or \"group_by(.category)\"")]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JsonQueryTool {
    /// Path to the JSON file (relative to project root)
    pub file_path: String,
    /// JSONPath or simple jq-style query string
    pub query: String,
    /// Operation type: "read" (default) or "write"
    #[serde(default = "default_operation")]
    pub operation: String,
    /// Output format: "json" (default), "raw", or "compact"
    #[serde(default = "default_output_format")]
    pub output_format: String,
    /// Modify file in-place for write operations (default: false)
    #[serde(default)]
    pub in_place: bool,
    /// Create backup before writing (default: false)
    #[serde(default)]
    pub backup: bool,
    /// Follow symlinks when reading files (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

fn default_operation() -> String {
    "read".to_string()
}

fn default_output_format() -> String {
    "json".to_string()
}

fn default_follow_symlinks() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonQueryResult {
    pub result: serde_json::Value,
    pub output: String,
    pub modified: bool,
}

impl JsonQueryTool {
    fn read_json_file(&self, file_path: &Path) -> Result<serde_json::Value, JsonQueryError> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    JsonQueryError::FileNotFound(file_path.display().to_string())
                } else {
                    JsonQueryError::IoError(e.to_string())
                }
            })?;
        
        serde_json::from_str(&content).map_err(|e| JsonQueryError::InvalidJson {
            file: file_path.display().to_string(),
            error: e.to_string(),
        })
    }
    
    fn write_json_file(&self, file_path: &Path, data: &serde_json::Value, backup: bool) -> Result<(), JsonQueryError> {
        if backup && file_path.exists() {
            let backup_path = file_path.with_extension(format!("{}.bak", 
                file_path.extension().and_then(|s| s.to_str()).unwrap_or("json")));
            std::fs::copy(file_path, backup_path)
                .map_err(|e| JsonQueryError::IoError(format!("Failed to create backup: {}", e)))?;
        }
        
        let content = match self.output_format.as_str() {
            "compact" => serde_json::to_string(data),
            _ => serde_json::to_string_pretty(data),
        }.map_err(|e| JsonQueryError::IoError(format!("Failed to serialize JSON: {}", e)))?;
        
        std::fs::write(file_path, content)
            .map_err(|e| JsonQueryError::IoError(e.to_string()))
    }
    
    fn format_output(&self, result: &serde_json::Value) -> String {
        match self.output_format.as_str() {
            "raw" => {
                match result {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".to_string(),
                    _ => serde_json::to_string_pretty(result).unwrap_or_else(|_| "null".to_string()),
                }
            }
            "compact" => {
                serde_json::to_string(result).unwrap_or_else(|_| "null".to_string())
            }
            _ => {
                serde_json::to_string_pretty(result).unwrap_or_else(|_| "null".to_string())
            }
        }
    }
}

#[async_trait]
impl StatefulTool for JsonQueryTool {
    async fn call_with_context(self, context: &ToolContext) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
        
        // For read operations, use symlink-aware path resolution
        let file_path = if self.operation == "read" {
            resolve_path_for_read(&self.file_path, &project_root, self.follow_symlinks, "jq")
                .map_err(|e| CallToolError::from(e))?
        } else {
            // For write operations, ensure we don't write through symlinks
            let requested_path = Path::new(&self.file_path);
            let absolute_path = if requested_path.is_absolute() {
                requested_path.to_path_buf()
            } else {
                project_root.join(requested_path)
            };
            
            // Canonicalize the path to resolve all symlinks
            let canonical = absolute_path.canonicalize()
                .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &format!("Failed to resolve path '{}': {}", self.file_path, e))))?;
            
            // Ensure the canonicalized path is within the project directory
            if !canonical.starts_with(&project_root) {
                return Err(CallToolError::from(tool_errors::access_denied(
                    "jq",
                    &self.file_path,
                    "Path is outside the project directory"
                )));
            }
            
            canonical
        };
        
        // Read JSON file
        let mut data = self.read_json_file(&file_path).map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
        
        let engine = QueryEngine::new();
        let result = match self.operation.as_str() {
            "write" => {
                // For write operations, use the query engine
                engine.execute_write(&mut data, &self.query)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
                
                if self.in_place {
                    self.write_json_file(&file_path, &data, self.backup)
                        .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
                }
                
                JsonQueryResult {
                    result: data.clone(),
                    output: self.format_output(&data),
                    modified: true,
                }
            }
            _ => {
                // Read operation
                let result = engine.execute(&data, &self.query)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
                
                JsonQueryResult {
                    result: result.clone(),
                    output: self.format_output(&result),
                    modified: false,
                }
            }
        };
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                result.output, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}