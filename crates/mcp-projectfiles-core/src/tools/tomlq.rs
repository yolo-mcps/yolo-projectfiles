use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TomlQueryError {
    #[error("Error: projectfiles:tomlq - File not found: {0}")]
    FileNotFound(String),
    
    #[error("Error: projectfiles:tomlq - Invalid TOML in file {file}: {error}")]
    InvalidToml { file: String, error: String },
    
    #[error("Error: projectfiles:tomlq - Invalid query syntax: {0}")]
    InvalidQuery(String),
    
    #[error("Error: projectfiles:tomlq - Query execution failed: {0}")]
    ExecutionError(String),
    
    #[error("Error: projectfiles:tomlq - IO error: {0}")]
    IoError(String),
}

#[mcp_tool(name = "tomlq", description = "Query and manipulate TOML files using jq-style syntax.

Supports both read and write operations:
- Read operations: Query TOML data and return results in various formats
- Write operations: Apply transformations and save changes back to file

Examples:
- Query a field: {\"file_path\": \"Cargo.toml\", \"query\": \".package.name\"}
- Filter dependencies: {\"file_path\": \"Cargo.toml\", \"query\": \".dependencies\"}
- Simple assignment: {\"file_path\": \"config.toml\", \"query\": \".debug = true\", \"operation\": \"write\", \"in_place\": true}

Output formats:
- \"toml\": TOML format (default)
- \"json\": JSON format
- \"raw\": Raw string values for simple types

Special TOML features:
- Handles TOML-specific data types (integers, floats, dates)
- Preserves TOML structure and formatting where possible
- Converts null values to string \"null\" (TOML doesn't support null)
- Maintains compatibility with JSON tooling via internal conversion

Safety features:
- Restricted to project directory only
- Supports symlink handling with follow_symlinks parameter (default: true)
- When follow_symlinks is false, symlinked TOML files cannot be accessed
- Automatic backup creation for write operations
- Atomic writes to prevent file corruption
- Path validation to prevent directory traversal")]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct TomlQueryTool {
    /// Path to the TOML file (relative to project root)
    pub file_path: String,
    /// Query string using jq-style syntax
    pub query: String,
    /// Operation type: "read" (default) or "write"
    #[serde(default = "default_operation")]
    pub operation: String,
    /// Output format: "toml" (default), "json", or "raw"
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
    "toml".to_string()
}

fn default_backup() -> bool {
    true
}

fn default_follow_symlinks() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TomlQueryResult {
    pub result: serde_json::Value,
    pub output: String,
    pub modified: bool,
}



impl TomlQueryTool {

    fn read_toml_file(&self, file_path: &Path) -> Result<serde_json::Value, TomlQueryError> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    TomlQueryError::FileNotFound(file_path.display().to_string())
                } else {
                    TomlQueryError::IoError(e.to_string())
                }
            })?;
        
        // Parse TOML and convert to JSON Value for uniform processing
        let toml_value: toml::Value = toml::from_str(&content)
            .map_err(|e| TomlQueryError::InvalidToml {
                file: file_path.display().to_string(),
                error: e.to_string(),
            })?;
        
        // Convert TOML Value to JSON Value for jq processing
        let json_str = serde_json::to_string(&toml_value)
            .map_err(|e| TomlQueryError::ExecutionError(format!("TOML to JSON conversion failed: {}", e)))?;
        
        serde_json::from_str(&json_str)
            .map_err(|e| TomlQueryError::ExecutionError(format!("JSON parsing failed: {}", e)))
    }
    
    fn execute_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, TomlQueryError> {
        // Use jsonpath-rust for JSONPath queries (subset of jq functionality)

        
        // Convert jq-style simple queries to JSONPath format
        self.simple_path_query(data, query)
    }
    

    fn json_to_toml_value(&self, json_value: &serde_json::Value) -> Result<toml::Value, TomlQueryError> {
        match json_value {
            serde_json::Value::Null => Ok(toml::Value::String("null".to_string())), // TOML doesn't have null
            serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(toml::Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(toml::Value::Float(f))
                } else {
                    Err(TomlQueryError::ExecutionError("Invalid number format".to_string()))
                }
            }
            serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let mut toml_arr = Vec::new();
                for item in arr {
                    toml_arr.push(self.json_to_toml_value(item)?);
                }
                Ok(toml::Value::Array(toml_arr))
            }
            serde_json::Value::Object(obj) => {
                let mut toml_table = toml::map::Map::new();
                for (key, value) in obj {
                    toml_table.insert(key.clone(), self.json_to_toml_value(value)?);
                }
                Ok(toml::Value::Table(toml_table))
            }
        }
    }
    
    fn simple_path_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, TomlQueryError> {
        let query = query.trim();
        
        // Handle root reference
        if query == "." {
            return Ok(data.clone());
        }
        
        if !query.starts_with('.') {
            return Err(TomlQueryError::InvalidQuery(
                format!("Query must start with '.': {}", query)
            ));
        }
        
        // Check for unsupported operators
        if query.contains('|') {
            return Err(TomlQueryError::InvalidQuery(
                format!("Pipe operations not supported: {}. Use simple paths like '.field', '.field.subfield', '.array[0]', or '.users[0].roles[1]'", query)
            ));
        }
        
        let path = &query[1..]; // Remove leading '.'
        if path.is_empty() {
            return Ok(data.clone());
        }
        
        // Parse the full path with array access support
        self.parse_complex_path(data, path)
    }
    
    fn parse_complex_path(&self, data: &serde_json::Value, path: &str) -> Result<serde_json::Value, TomlQueryError> {
        let mut current = data.clone();
        let mut i = 0;
        let chars: Vec<char> = path.chars().collect();
        
        while i < chars.len() {
            // Parse field name
            let mut field_end = i;
            while field_end < chars.len() && chars[field_end] != '.' && chars[field_end] != '[' {
                field_end += 1;
            }
            
            if field_end > i {
                let field_name: String = chars[i..field_end].iter().collect();
                if let serde_json::Value::Object(obj) = &current {
                    current = obj.get(&field_name).unwrap_or(&serde_json::Value::Null).clone();
                } else {
                    return Ok(serde_json::Value::Null);
                }
                i = field_end;
            }
            
            // Handle array access
            while i < chars.len() && chars[i] == '[' {
                // Find the closing bracket
                let mut bracket_end = i + 1;
                while bracket_end < chars.len() && chars[bracket_end] != ']' {
                    bracket_end += 1;
                }
                
                if bracket_end >= chars.len() {
                    return Err(TomlQueryError::InvalidQuery("Missing closing bracket ]".to_string()));
                }
                
                let index_str: String = chars[i + 1..bracket_end].iter().collect();
                let index: usize = index_str.parse()
                    .map_err(|_| TomlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                
                if let serde_json::Value::Array(arr) = &current {
                    if index < arr.len() {
                        current = arr[index].clone();
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                } else {
                    return Ok(serde_json::Value::Null);
                }
                
                i = bracket_end + 1;
            }
            
            // Skip dot separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
        
        Ok(current)
    }
    
    fn format_output(&self, value: &serde_json::Value, format: &str) -> Result<String, TomlQueryError> {

        match format {
            "toml" => {
                match value {
                    // For scalar values, return raw values since TOML can't serialize bare scalars
                    serde_json::Value::String(s) => Ok(s.clone()),
                    serde_json::Value::Number(n) => Ok(n.to_string()),
                    serde_json::Value::Bool(b) => Ok(b.to_string()),
                    serde_json::Value::Null => Ok("null".to_string()), // TOML doesn't have null
                    serde_json::Value::Array(_) => {
                        // TOML cannot serialize bare arrays at root level
                        // Wrap in a table with "result" key
                        let mut wrapper = serde_json::Map::new();
                        wrapper.insert("result".to_string(), value.clone());
                        let wrapped_value = serde_json::Value::Object(wrapper);
                        let toml_value = self.json_to_toml_value(&wrapped_value)?;
                        let output = toml::to_string_pretty(&toml_value)
                            .map_err(|e| TomlQueryError::ExecutionError(format!("TOML serialization failed: {}", e)))?;
                        
                        // Extract just the array portion
                        if let Some(start) = output.find("result = ") {
                            Ok(output[start + 9..].to_string())
                        } else {
                            Ok(output)
                        }
                    }
                    serde_json::Value::Object(_) => {
                        // Objects can be serialized directly as TOML tables
                        let toml_value = self.json_to_toml_value(value)?;
                        toml::to_string_pretty(&toml_value)
                            .map_err(|e| TomlQueryError::ExecutionError(format!("TOML serialization failed: {}", e)))
                    }
                }
            }
            "json" => serde_json::to_string_pretty(value)
                .map_err(|e| TomlQueryError::ExecutionError(format!("JSON serialization failed: {}", e))),
            "raw" => {
                match value {
                    serde_json::Value::String(s) => Ok(s.clone()),
                    serde_json::Value::Number(n) => Ok(n.to_string()),
                    serde_json::Value::Bool(b) => Ok(b.to_string()),
                    serde_json::Value::Null => Ok("null".to_string()),
                    _ => {
                        let toml_value = self.json_to_toml_value(value)?;
                        toml::to_string_pretty(&toml_value)
                            .map_err(|e| TomlQueryError::ExecutionError(format!("TOML serialization failed: {}", e)))
                    }
                }
            }
            _ => Err(TomlQueryError::ExecutionError(format!("Invalid output format: {}", format))),
        }
    }
    
    fn write_toml_file(&self, file_path: &Path, data: &serde_json::Value, backup: bool) -> Result<(), TomlQueryError> {
        if backup && file_path.exists() {
            let backup_path = format!("{}.bak", file_path.display());
            std::fs::copy(file_path, &backup_path)
                .map_err(|e| TomlQueryError::IoError(format!("Failed to create backup: {}", e)))?;
        }
        
        let toml_value = self.json_to_toml_value(data)?;
        let toml_str = toml::to_string_pretty(&toml_value)
            .map_err(|e| TomlQueryError::ExecutionError(format!("TOML serialization failed: {}", e)))?;
        
        // Atomic write using temporary file
        let temp_path = format!("{}.tmp", file_path.display());
        std::fs::write(&temp_path, toml_str)
            .map_err(|e| TomlQueryError::IoError(format!("Failed to write temporary file: {}", e)))?;
        
        std::fs::rename(&temp_path, file_path)
            .map_err(|e| TomlQueryError::IoError(format!("Failed to move temporary file: {}", e)))?;
        
        Ok(())
    }
    
    fn parse_assignment(&self, query: &str) -> Result<Option<(String, serde_json::Value)>, TomlQueryError> {
        // Parse simple assignment patterns like ".field = value"
        if let Some(eq_pos) = query.find('=') {
            let path = query[..eq_pos].trim();
            let value_str = query[eq_pos + 1..].trim();
            
            // Parse the value as JSON, handling different types properly
            let value = if value_str == "true" {
                serde_json::Value::Bool(true)
            } else if value_str == "false" {
                serde_json::Value::Bool(false)
            } else if value_str == "null" {
                serde_json::Value::Null
            } else if let Ok(num) = value_str.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Ok(num) = value_str.parse::<f64>() {
                serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or(serde_json::Number::from(0)))
            } else if value_str.starts_with('"') && value_str.ends_with('"') {
                // Already quoted string - parse as JSON
                serde_json::from_str(value_str)
                    .map_err(|e| TomlQueryError::InvalidQuery(format!("Invalid JSON string '{}': {}", value_str, e)))?
            } else if value_str.starts_with('[') || value_str.starts_with('{') {
                // JSON array or object
                serde_json::from_str(value_str)
                    .map_err(|e| TomlQueryError::InvalidQuery(format!("Invalid JSON '{}': {}", value_str, e)))?
            } else {
                // Treat as unquoted string
                serde_json::Value::String(value_str.to_string())
            };
            
            Ok(Some((path.to_string(), value)))
        } else {
            Ok(None)
        }
    }
    
    fn apply_assignment(&self, data: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<(), TomlQueryError> {
        // Apply assignment to JSON data using complex path parsing
        if path == "." {
            *data = value;
            return Ok(());
        }
        
        if !path.starts_with('.') {
            return Err(TomlQueryError::InvalidQuery(
                "Assignment path must start with '.'".to_string()
            ));
        }
        
        let path = &path[1..]; // Remove leading '.'
        
        // Use the same complex path parsing logic as read operations
        self.set_complex_path(data, path, value)
    }
    
    fn set_complex_path(&self, data: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<(), TomlQueryError> {
        let mut current = data;
        let mut i = 0;
        let chars: Vec<char> = path.chars().collect();
        
        while i < chars.len() {
            let mut segment = String::new();
            
            // Read until we hit '[', '.', or end of string
            while i < chars.len() && chars[i] != '[' && chars[i] != '.' {
                segment.push(chars[i]);
                i += 1;
            }
            
            // Check if this segment is followed by array access
            let is_array_access = i < chars.len() && chars[i] == '[';
            
            // If we have a segment, navigate to it
            if !segment.is_empty() {
                // Check if this is the final segment (no array access and at end of path)
                if !is_array_access && i >= chars.len() {
                    // This is the final segment - set the value
                    if let serde_json::Value::Object(obj) = current {
                        obj.insert(segment, value);
                        return Ok(());
                    } else {
                        return Err(TomlQueryError::ExecutionError(
                            format!("Cannot set field '{}' on non-object value", segment)
                        ));
                    }
                } else if is_array_access {
                    // This segment refers to an array - just navigate to it
                    if let serde_json::Value::Object(obj) = current {
                        current = obj.get_mut(&segment)
                            .ok_or_else(|| TomlQueryError::ExecutionError(
                                format!("Field '{}' not found", segment)
                            ))?;
                    } else {
                        return Err(TomlQueryError::ExecutionError(
                            format!("Cannot access field '{}' on non-object value", segment)
                        ));
                    }
                } else {
                    // This is an intermediate object field
                    if let serde_json::Value::Object(obj) = current {
                        // Get or create the field
                        current = obj.entry(segment.clone())
                            .or_insert(serde_json::Value::Object(serde_json::Map::new()));
                    } else {
                        return Err(TomlQueryError::ExecutionError(
                            format!("Cannot access field '{}' on non-object value", segment)
                        ));
                    }
                }
            }
            
            // Handle array access
            if is_array_access {
                i += 1; // skip '['
                let mut index_str = String::new();
                while i < chars.len() && chars[i] != ']' {
                    index_str.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(TomlQueryError::InvalidQuery("Unclosed bracket".to_string()));
                }
                i += 1; // skip ']'
                
                let index = index_str.parse::<usize>()
                    .map_err(|_| TomlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                
                // Check if this is the final access
                if i >= chars.len() {
                    // This is the final array access - set the value
                    if let serde_json::Value::Array(arr) = current {
                        if index >= arr.len() {
                            return Err(TomlQueryError::ExecutionError(
                                format!("Array index {} out of bounds", index)
                            ));
                        }
                        arr[index] = value;
                        return Ok(());
                    } else {
                        return Err(TomlQueryError::ExecutionError(
                            "Cannot index non-array value".to_string()
                        ));
                    }
                } else {
                    // Navigate to the array element
                    if let serde_json::Value::Array(arr) = current {
                        if index >= arr.len() {
                            return Err(TomlQueryError::ExecutionError(
                                format!("Array index {} out of bounds", index)
                            ));
                        }
                        current = &mut arr[index];
                    } else {
                        return Err(TomlQueryError::ExecutionError(
                            "Cannot index non-array value".to_string()
                        ));
                    }
                }
            }
            
            // Skip dot separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
        
        // If we've consumed the entire path but haven't set the value yet,
        // it means we ended on a dot or the path was empty
        *current = value;
        Ok(())
    }
}

#[async_trait]
impl StatefulTool for TomlQueryTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Get project root
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &format!("Failed to get project root: {}", e))))?;
        
        // For read operations, use symlink-aware path resolution
        let canonical_path = if self.operation == "read" {
            resolve_path_for_read(&self.file_path, &project_root, self.follow_symlinks, "tomlq")
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
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &format!("Failed to resolve path '{}': {}", self.file_path, e))))?;
                
                // Ensure the canonicalized path is within the project directory
                if !canonical.starts_with(&project_root) {
                    return Err(CallToolError::from(tool_errors::access_denied(
                        "tomlq",
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
                            .map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &format!("Failed to resolve parent directory: {}", e))))?;
                        
                        // Ensure the parent is within the project directory
                        if !canonical_parent.starts_with(&project_root) {
                            return Err(CallToolError::from(tool_errors::access_denied(
                                "tomlq",
                                &self.file_path,
                                "Path would be outside the project directory"
                            )));
                        }
                        
                        // Reconstruct the path with the canonical parent
                        if let Some(file_name) = absolute_path.file_name() {
                            canonical_parent.join(file_name)
                        } else {
                            return Err(CallToolError::from(tool_errors::invalid_input(
                                "tomlq",
                                &format!("Invalid file path: '{}'", self.file_path)
                            )));
                        }
                    } else {
                        // Parent doesn't exist yet, need to check each component
                        // Walk up the path to find the first existing parent
                        let mut current = parent;
                        let mut components_to_create = vec![];
                        
                        while !current.exists() {
                            if let Some(parent_of_current) = current.parent() {
                                components_to_create.push(current);
                                current = parent_of_current;
                            } else {
                                break;
                            }
                        }
                        
                        // Now current is the first existing ancestor
                        if current.exists() {
                            let canonical_existing = current.canonicalize()
                                .map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &format!("Failed to resolve existing parent: {}", e))))?;
                            
                            // Check if the existing parent is within project
                            if !canonical_existing.starts_with(&project_root) {
                                return Err(CallToolError::from(tool_errors::access_denied(
                                    "tomlq",
                                    &self.file_path,
                                    "Path would be outside the project directory"
                                )));
                            }
                            
                            // Reconstruct the full path
                            let mut result = canonical_existing;
                            for component in components_to_create.iter().rev() {
                                if let Some(file_name) = component.file_name() {
                                    result = result.join(file_name);
                                }
                            }
                            if let Some(file_name) = absolute_path.file_name() {
                                result.join(file_name)
                            } else {
                                result
                            }
                        } else {
                            // No existing parent found, check if path is at least within project
                            if !absolute_path.starts_with(&project_root) {
                                return Err(CallToolError::from(tool_errors::access_denied(
                                    "tomlq",
                                    &self.file_path,
                                    "Path would be outside the project directory"
                                )));
                            }
                            absolute_path
                        }
                    }
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input(
                        "tomlq",
                        &format!("Invalid file path: '{}'", self.file_path)
                    )));
                }
            }
        };
        
        // Read the TOML file
        let mut data = self.read_toml_file(&canonical_path).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))?;
        
        let mut modified = false;
        
        // Execute the query
        let result = match self.operation.as_str() {
            "read" => {
                // Track file as read
                let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
                    .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
                let mut read_files_clone = (*read_files).clone();
                read_files_clone.insert(canonical_path.clone());
                context.set_custom_state(read_files_clone).await;
                
                self.execute_query(&data, &self.query).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))?
            }
            "write" => {
                // Check if file has been read
                let read_files = context.get_custom_state::<HashSet<PathBuf>>().await
                    .unwrap_or_else(|| std::sync::Arc::new(HashSet::new()));
                
                // Check if this is a new file (doesn't exist)
                let is_new_file = !canonical_path.exists();
                
                if !is_new_file && !read_files.contains(&canonical_path) {
                    return Err(CallToolError::from(tool_errors::operation_not_permitted(
                        "tomlq", 
                        &format!("File must be read before editing: {}", self.file_path)
                    )));
                }
                // For write operations, apply simple value assignments
                if self.in_place {
                    // Parse simple assignment queries like ".field = value"
                    if let Some((path, value)) = self.parse_assignment(&self.query).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))? {
                        self.apply_assignment(&mut data, &path, value).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))?;
                        modified = true;
                        
                        // Write the modified data back to file
                        self.write_toml_file(&canonical_path, &data, self.backup).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))?;
                        data.clone()
                    } else {
                        return Err(CallToolError::from(tool_errors::invalid_input("tomlq", 
                            "Write operations currently only support simple assignments like '.field = value'"
                        )));
                    }
                } else {
                    return Err(CallToolError::from(tool_errors::invalid_input("tomlq",
                        "Write operations require in_place=true"
                    )));
                }
            }
            _ => return Err(CallToolError::from(tool_errors::invalid_input("tomlq", 
                &format!("Invalid operation: {}. Must be 'read' or 'write'", self.operation)
            ))),
        };
        
        // Format the output
        let output = self.format_output(&result, &self.output_format).map_err(|e| CallToolError::from(tool_errors::invalid_input("tomlq", &e.to_string())))?;
        
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