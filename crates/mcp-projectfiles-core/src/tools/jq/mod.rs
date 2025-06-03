mod parser;
mod executor;
mod functions;
mod operators;
mod conditionals;

use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use executor::JsonQueryExecutor;

#[derive(Error, Debug)]
pub enum JsonQueryError {
    #[error("Error: projectfiles:jq - File not found: {0}")]
    FileNotFound(String),
    
    #[error("Error: projectfiles:jq - Invalid JSON in file {file}: {error}")]
    InvalidJson { file: String, error: String },
    
    #[error("Error: projectfiles:jq - Invalid query syntax: {0}")]
    InvalidQuery(String),
    
    #[error("Error: projectfiles:jq - Query execution failed: {0}")]
    ExecutionError(String),
    
    #[error("Error: projectfiles:jq - IO error: {0}")]
    IoError(String),
    
    #[error("Error: projectfiles:jq - Path outside project directory: {0}")]
    PathOutsideProject(String),
}

#[mcp_tool(name = "jq", description = "Query and manipulate JSON files using jq-style syntax. Preferred tool for JSON manipulation in projects.

Core Features:

Data Access & Filtering:
- Basic access: \".field\", \".nested.field\", \".array[0]\", \".users[*].name\"
- Array iteration: \".users[]\", \"map(.name)\", \"select(.age > 18)\"
- Filtering: \"select(.active)\", \"map(select(.score > 80))\"
- Recursive search: \"..email\" (find all email fields), \"..\" (all values)
- Wildcards: \".users.*\", \".data.items[*].id\"

Array Operations:
- Basic: \"add\" (sum/concat), \"min\", \"max\", \"unique\", \"reverse\", \"sort\", \"sort_by(.field)\"
- Advanced: \"flatten\", \"group_by(.key)\", \"indices(value)\", \"[2:5]\" (slicing)

Object Operations:
- Tools: \"keys\", \"values\", \"has(\\\"field\\\")\", \"del(.field)\", \"to_entries\", \"from_entries\"
- Manipulation: \"with_entries(.value *= 2)\", \"paths\", \"leaf_paths\"

String Processing:
- Functions: \"split(\\\",\\\")\", \"join(\\\" \\\")\", \"trim\", \"ltrimstr(\\\"prefix\\\")\", \"rtrimstr(\\\"suffix\\\")\"
- Case: \"ascii_upcase\", \"ascii_downcase\"
- Testing: \"contains(\\\"@\\\")\", \"startswith(\\\"http\\\")\", \"test(\\\"^[0-9]+$\\\")\", \"match(\\\"(\\\\d+)\\\")\"
- Conversion: \"tostring\", \"tonumber\"

Math & Logic:
- Arithmetic: \".price * 1.1\", \".x + .y\", \".a % .b\"
- Math: \"floor\", \"ceil\", \"round\", \"abs\"
- Conditionals: \"if .age > 18 then \\\"adult\\\" else \\\"minor\\\" end\"
- Boolean: \".age >= 18 and .active\", \".premium or .vip\", \"not .disabled\"
- Null handling: \".timeout // 30\", \".user.profile?\", \"try .risky catch \\\"failed\\\"\"

Common Examples:
- Extract data: \".users | map(.email)\"
- Filter: \".users | map(select(.active))\"
- Calculate: \".orders | map(.items | map(.price * .quantity) | add) | add\"
- Group: \"group_by(.category) | map({category: .[0].category, count: length})\"
- Transform: \"to_entries | map({name: .key, value: .value | ascii_upcase}) | from_entries\"

Write Operations:
- Simple: \".field = value\", \".nested.field = \\\"text\\\"\"
- Array: \".items[0] = \\\"new\\\"\"
- Bulk: \"map(.active = true)\"

Output Formats:
- \"json\": Pretty-printed (default)
- \"compact\": Minified JSON
- \"raw\": Plain values for simple types

Safety:
- Restricted to project directory only
- Optional backups for write operations (backup: true)
- Atomic writes prevent corruption")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
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
}

fn default_operation() -> String {
    "read".to_string()
}

fn default_output_format() -> String {
    "json".to_string()
}



#[derive(Debug, Serialize, Deserialize)]
pub struct JsonQueryResult {
    pub result: serde_json::Value,
    pub output: String,
    pub modified: bool,
}

impl JsonQueryTool {
    fn validate_path(&self, file_path: &str) -> Result<PathBuf, JsonQueryError> {
        let path = Path::new(file_path);
        
        // Ensure path is relative and doesn't escape project directory
        if path.is_absolute() {
            return Err(JsonQueryError::PathOutsideProject(file_path.to_string()));
        }
        
        // Check for path traversal attempts
        for component in path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(JsonQueryError::PathOutsideProject(file_path.to_string()));
            }
        }
        
        Ok(path.to_path_buf())
    }
    
    fn read_json_file(&self, file_path: &Path) -> Result<serde_json::Value, JsonQueryError> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    JsonQueryError::FileNotFound(file_path.display().to_string())
                } else {
                    JsonQueryError::IoError(e.to_string())
                }
            })?;
        
        serde_json::from_str(&content)
            .map_err(|e| JsonQueryError::InvalidJson {
                file: file_path.display().to_string(),
                error: e.to_string(),
            })
    }
    
    fn format_output(&self, result: &serde_json::Value) -> String {
        match self.output_format.as_str() {
            "raw" => {
                match result {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".to_string(),
                    _ => serde_json::to_string_pretty(result).unwrap_or_default(),
                }
            }
            "compact" => serde_json::to_string(result).unwrap_or_default(),
            _ => serde_json::to_string_pretty(result).unwrap_or_default(),
        }
    }
    
    fn write_json_file(&self, file_path: &Path, data: &serde_json::Value, backup: bool) -> Result<(), JsonQueryError> {
        // Create backup if requested
        if backup && file_path.exists() {
            let backup_path = file_path.with_extension("json.bak");
            std::fs::copy(file_path, backup_path)
                .map_err(|e| JsonQueryError::IoError(format!("Failed to create backup: {}", e)))?;
        }
        
        // Write atomically
        let temp_path = file_path.with_extension(".tmp");
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| JsonQueryError::IoError(format!("Failed to serialize JSON: {}", e)))?;
        
        std::fs::write(&temp_path, content)
            .map_err(|e| JsonQueryError::IoError(format!("Failed to write temp file: {}", e)))?;
        
        std::fs::rename(&temp_path, file_path)
            .map_err(|e| JsonQueryError::IoError(format!("Failed to rename file: {}", e)))?;
        
        Ok(())
    }
}

#[async_trait]
impl StatefulTool for JsonQueryTool {
    async fn call_with_context(self, context: &ToolContext) -> Result<CallToolResult, CallToolError> {
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
        let relative_path = self.validate_path(&self.file_path).map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
        let file_path = project_root.join(&relative_path);
        
        // Check if file exists
        if !file_path.exists() {
            return Err(CallToolError::from(tool_errors::invalid_input("jq", &format!("File not found: {}", relative_path.display()))));
        }
        
        // Read JSON file
        let mut data = self.read_json_file(&file_path).map_err(|e| CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())))?;
        
        let executor = JsonQueryExecutor::new();
        let result = match self.operation.as_str() {
            "write" => {
                // For write operations, parse assignment and execute
                match parser::parse_assignment(&self.query) {
                    Ok(Some((path, value))) => {
                        executor.set_path(&mut data, &path, value)
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
                    Ok(None) => {
                        return Err(CallToolError::from(tool_errors::invalid_input("jq", "Write operation requires an assignment expression")));
                    }
                    Err(e) => {
                        return Err(CallToolError::from(tool_errors::invalid_input("jq", &e.to_string())));
                    }
                }
            }
            _ => {
                // Read operation
                let result = executor.execute_query(&data, &self.query)
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