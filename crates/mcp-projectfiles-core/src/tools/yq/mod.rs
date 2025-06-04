mod parser;
mod executor;
mod functions;
mod operators;
mod conditionals;

use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use executor::YamlQueryExecutor;

#[derive(Error, Debug)]
pub enum YamlQueryError {
    #[error("Error: projectfiles:yq - File not found: {0}")]
    FileNotFound(String),
    
    #[error("Error: projectfiles:yq - Invalid YAML in file {file}: {error}")]
    InvalidYaml { file: String, error: String },
    
    #[error("Error: projectfiles:yq - Invalid query syntax: {0}")]
    InvalidQuery(String),
    
    #[error("Error: projectfiles:yq - Query execution failed: {0}")]
    ExecutionError(String),
    
    #[error("Error: projectfiles:yq - IO error: {0}")]
    IoError(String),
}

#[mcp_tool(name = "yq", description = "Query and manipulate YAML files using jq-style syntax. Preferred tool for YAML manipulation in projects.

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
- \"yaml\": YAML format (default)
- \"json\": JSON format
- \"raw\": Plain values for simple types

YAML-Specific Features:
- Preserves YAML structure and types
- Handles multi-document YAML files
- Maintains compatibility with JSON tooling via internal conversion
- Type-aware processing (strings, numbers, booleans, null)

Safety:
- Restricted to project directory only
- Supports symlink handling with follow_symlinks parameter (default: true)
- When follow_symlinks is false, symlinked YAML files cannot be accessed
- Optional backups for write operations (backup: true)
- Atomic writes prevent corruption")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct YamlQueryTool {
    /// Path to the YAML file (relative to project root)
    pub file_path: String,
    /// jq-style query string for YAML data manipulation
    pub query: String,
    /// Operation type: "read" (default) or "write"
    #[serde(default = "default_operation")]
    pub operation: String,
    /// Output format: "yaml" (default), "json", or "raw"
    #[serde(default = "default_output_format")]
    pub output_format: String,
    /// Modify file in-place for write operations (default: false)
    #[serde(default)]
    pub in_place: bool,
    /// Create backup before writing (default: true for write operations)
    #[serde(default = "default_backup")]
    pub backup: bool,
    /// Follow symlinks when reading files (default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

fn default_operation() -> String {
    "read".to_string()
}

fn default_output_format() -> String {
    "yaml".to_string()
}

fn default_backup() -> bool {
    true
}

fn default_follow_symlinks() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YamlQueryResult {
    pub result: serde_json::Value,
    pub output: String,
    pub modified: bool,
}

impl YamlQueryTool {

    fn read_yaml_file(&self, file_path: &Path) -> Result<serde_json::Value, YamlQueryError> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    YamlQueryError::FileNotFound(file_path.display().to_string())
                } else {
                    YamlQueryError::IoError(e.to_string())
                }
            })?;
        
        // Parse YAML and convert to JSON Value for uniform processing
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)
            .map_err(|e| YamlQueryError::InvalidYaml {
                file: file_path.display().to_string(),
                error: e.to_string(),
            })?;
        
        // Convert YAML Value to JSON Value for jq processing
        let json_str = serde_json::to_string(&yaml_value)
            .map_err(|e| YamlQueryError::ExecutionError(format!("YAML to JSON conversion failed: {}", e)))?;
        
        serde_json::from_str(&json_str)
            .map_err(|e| YamlQueryError::ExecutionError(format!("JSON parsing failed: {}", e)))
    }
    
    fn format_output(&self, value: &serde_json::Value, format: &str) -> Result<String, YamlQueryError> {
        match format {
            "yaml" => {
                // Convert JSON Value back to YAML
                serde_yaml::to_string(value)
                    .map_err(|e| YamlQueryError::ExecutionError(format!("YAML serialization failed: {}", e)))
            }
            "json" => serde_json::to_string_pretty(value)
                .map_err(|e| YamlQueryError::ExecutionError(format!("JSON serialization failed: {}", e))),
            "raw" => {
                match value {
                    serde_json::Value::String(s) => Ok(s.clone()),
                    serde_json::Value::Number(n) => Ok(n.to_string()),
                    serde_json::Value::Bool(b) => Ok(b.to_string()),
                    serde_json::Value::Null => Ok("null".to_string()),
                    _ => serde_yaml::to_string(value)
                        .map_err(|e| YamlQueryError::ExecutionError(format!("YAML serialization failed: {}", e))),
                }
            }
            _ => Err(YamlQueryError::ExecutionError(format!("Invalid output format: {}", format))),
        }
    }
    
    fn write_yaml_file(&self, file_path: &Path, data: &serde_json::Value, backup: bool) -> Result<(), YamlQueryError> {
        if backup && file_path.exists() {
            let backup_path = format!("{}.bak", file_path.display());
            std::fs::copy(file_path, &backup_path)
                .map_err(|e| YamlQueryError::IoError(format!("Failed to create backup: {}", e)))?;
        }
        
        let yaml_str = serde_yaml::to_string(data)
            .map_err(|e| YamlQueryError::ExecutionError(format!("YAML serialization failed: {}", e)))?;
        
        // Atomic write using temporary file
        let temp_path = format!("{}.tmp", file_path.display());
        std::fs::write(&temp_path, yaml_str)
            .map_err(|e| YamlQueryError::IoError(format!("Failed to write temporary file: {}", e)))?;
        
        std::fs::rename(&temp_path, file_path)
            .map_err(|e| YamlQueryError::IoError(format!("Failed to move temporary file: {}", e)))?;
        
        Ok(())
    }
}

#[async_trait]
impl StatefulTool for YamlQueryTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        use std::collections::HashSet;
        
        // Get project root
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &format!("Failed to get project root: {}", e))))?;
        
        // For read operations, use symlink-aware path resolution
        let canonical_path = if self.operation == "read" {
            resolve_path_for_read(&self.file_path, &project_root, self.follow_symlinks, "yq")
                .map_err(|e| CallToolError::from(e))?
        } else {
            // For write operations, ensure we don't write through symlinks
            let requested_path = Path::new(&self.file_path);
            let absolute_path = if requested_path.is_absolute() {
                requested_path.to_path_buf()
            } else {
                project_root.join(requested_path)
            };
            
            // For existing files, canonicalize the path to resolve all symlinks
            // For new files, canonicalize the parent directory and ensure the full path is within bounds
            if absolute_path.exists() {
                let canonical = absolute_path.canonicalize()
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &format!("Failed to resolve path '{}': {}", self.file_path, e))))?;
                
                // Ensure the canonicalized path is within the project directory
                if !canonical.starts_with(&project_root) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        "yq",
                        &self.file_path,
                        "Path is outside the project directory"
                    )));
                }
                
                canonical
            } else {
                // For new files, canonicalize the parent directory
                if let Some(parent) = absolute_path.parent() {
                    if parent.exists() {
                        let canonical_parent = parent.canonicalize()
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &format!("Failed to resolve parent directory: {}", e))))?;
                        
                        // Ensure the parent is within the project directory
                        if !canonical_parent.starts_with(&project_root) {
                            return Err(CallToolError::from(tool_errors::access_denied(
                                "yq",
                                &self.file_path,
                                "Path would be outside the project directory"
                            )));
                        }
                        
                        // Reconstruct the path with the canonical parent
                        if let Some(file_name) = absolute_path.file_name() {
                            canonical_parent.join(file_name)
                        } else {
                            return Err(CallToolError::from(tool_errors::invalid_input(
                                "yq",
                                &format!("Invalid file path: '{}'", self.file_path)
                            )));
                        }
                    } else {
                        // Parent doesn't exist yet, check path components
                        // Make sure the path would be within project bounds when created
                        if !absolute_path.starts_with(&project_root) {
                            return Err(CallToolError::from(tool_errors::access_denied(
                                "yq",
                                &self.file_path,
                                "Path would be outside the project directory"
                            )));
                        }
                        absolute_path
                    }
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        "yq",
                        &format!("Invalid file path: '{}'", self.file_path)
                    )));
                }
            }
        };
        
        // Read the YAML file
        let mut data = self.read_yaml_file(&canonical_path).map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &e.to_string())))?;
        
        let mut modified = false;
        
        // Execute the query using the YamlQueryExecutor
        let result = match self.operation.as_str() {
            "read" => {
                // Track file as read
                let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
                    .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
                let mut read_files_clone = (*read_files).clone();
                read_files_clone.insert(canonical_path.clone());
                context.set_custom_state(read_files_clone).await;
                
                let executor = YamlQueryExecutor::new();
                executor.execute(&data, &self.query).map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &e.to_string())))?
            }
            "write" => {
                // Check if file has been read
                let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
                    .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
                
                // Check if this is a new file (doesn't exist)
                let is_new_file = !canonical_path.exists();
                
                if !is_new_file && !read_files.contains(&canonical_path) {
                    return Err(CallToolError::from(tool_errors::operation_not_permitted(
                        "yq", 
                        &format!("File must be read before editing: {}", self.file_path)
                    )));
                }
                
                if self.in_place {
                    let executor = YamlQueryExecutor::new();
                    let result = executor.execute_write(&mut data, &self.query).map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &e.to_string())))?;
                    modified = true;
                    
                    // Write the modified data back to file
                    self.write_yaml_file(&canonical_path, &data, self.backup).map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &e.to_string())))?;
                    result
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input("yq",
                        "Write operations require in_place=true"
                    )));
                }
            }
            _ => return Err(CallToolError::from(tool_errors::invalid_input("yq", 
                &format!("Invalid operation: {}. Must be 'read' or 'write'", self.operation)
            ))),
        };
        
        // Format the output
        let output = self.format_output(&result, &self.output_format).map_err(|e| CallToolError::from(tool_errors::invalid_input("yq", &e.to_string())))?;
        
        // For write operations, return a summary of the operation
        let content = if self.operation == "write" && modified {
            serde_json::json!({
                "modified": true,
                "file": self.file_path,
                "query": self.query
            }).to_string()
        } else {
            // For read operations, just return the output
            output
        };
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::text_content(
                content,
                None,
            )],
            is_error: Some(false),
            meta: None,
        })
    }
}