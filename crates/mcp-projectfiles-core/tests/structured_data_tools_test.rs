use mcp_projectfiles_core::tools::{JsonQueryTool, YamlQueryTool, TomlQueryTool};
use mcp_projectfiles_core::context::ToolContext;
use mcp_projectfiles_core::StatefulTool;
use mcp_projectfiles_core::protocol::CallToolResultContentItem;

use tempfile::TempDir;
use std::fs;
use serial_test::serial;

fn extract_text_content(result: &mcp_projectfiles_core::CallToolResult) -> String {
    match &result.content[0] {
        CallToolResultContentItem::TextContent(text) => text.text.clone(),
        _ => panic!("Expected text content"),
    }
}

/// Set up a test with a temporary directory as the project root
fn setup_test_env() -> (TempDir, ToolContext) {
    // Create a new temp directory for this test
    let temp_dir = TempDir::new().unwrap();
    
    // Create context with project root override
    // Canonicalize the path to ensure consistency
    let canonical_path = temp_dir.path().canonicalize().unwrap();
    let context = ToolContext::with_project_root(canonical_path);
    
    (temp_dir, context)
}

// JQ Tool Tests
#[tokio::test]
#[serial]
async fn test_jq_tool_read_basic() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test JSON file
    let json_content = r#"{
        "name": "test",
        "version": "1.0.0",
        "config": {
            "debug": true,
            "database": {
                "host": "localhost",
                "port": 5432
            }
        }
    }"#;
    fs::write(temp_path.join("test.json"), json_content).unwrap();
    
    // Test basic path query
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "test");
    
    // Test nested path query
    let nested_tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".config.database.host".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = nested_tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "localhost");
}

#[tokio::test]
#[serial]
async fn test_jq_tool_complex_paths() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create complex test JSON file
    let json_content = r#"{
        "users": [
            {
                "name": "Alice",
                "roles": ["admin", "user"],
                "profile": {
                    "email": "alice@example.com",
                    "age": 30
                }
            },
            {
                "name": "Bob",
                "roles": ["user"],
                "profile": {
                    "email": "bob@example.com",
                    "age": 25
                }
            }
        ]
    }"#;
    fs::write(temp_path.join("complex.json"), json_content).unwrap();
    
    // Test array index with nested object
    let tool = JsonQueryTool {
        file_path: "complex.json".to_string(),
        query: ".users[0].profile.email".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "alice@example.com");
    
    // Test array within array
    let array_tool = JsonQueryTool {
        file_path: "complex.json".to_string(),
        query: ".users[0].roles[1]".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = array_tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "user");
}

#[tokio::test]
#[serial]
async fn test_jq_tool_write_operations() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test JSON file
    let json_content = r#"{
        "name": "test",
        "version": "1.0.0"
    }"#;
    fs::write(temp_path.join("write_test.json"), json_content).unwrap();
    
    // First read the file to mark it as read
    let read_tool = JsonQueryTool {
        file_path: "write_test.json".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    let _ = read_tool.call_with_context(&context).await;
    
    // Test write operation without quotes
    let tool = JsonQueryTool {
        file_path: "write_test.json".to_string(),
        query: ".description = \"hello world\"".to_string(),
        operation: "write".to_string(),
        output_format: "json".to_string(),
        in_place: true,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_ok());
    
    // Verify the written content
    let written_content = fs::read_to_string(temp_path.join("write_test.json")).unwrap();
    let written_json: serde_json::Value = serde_json::from_str(&written_content).unwrap();
    assert_eq!(written_json["description"], "hello world");
    
    // Verify backup was created
    assert!(temp_path.join("write_test.json.bak").exists());
}

// YQ Tool Tests
#[tokio::test]
#[serial]
async fn test_yq_tool_read_basic() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test YAML file
    let yaml_content = r#"name: test
version: 1.0.0
config:
  debug: true
  database:
    host: localhost
    port: 5432
"#;
    fs::write(temp_path.join("test.yaml"), yaml_content).unwrap();
    
    // Test basic path query
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "test");
    
    // Test nested path query
    let nested_tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".config.database.port".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = nested_tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "5432");
}

// TOMLQ Tool Tests
#[tokio::test]
#[serial]
async fn test_tomlq_tool_read_basic() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file
    let toml_content = r#"name = "test"
version = "1.0.0"

[config]
debug = true

[config.database]
host = "localhost"
port = 5432
"#;
    fs::write(temp_path.join("test.toml"), toml_content).unwrap();
    
    // Test basic path query
    let tool = TomlQueryTool {
        file_path: "test.toml".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "test");
    
    // Test nested path query
    let nested_tool = TomlQueryTool {
        file_path: "test.toml".to_string(),
        query: ".config.database.port".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = nested_tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "5432");
}

#[tokio::test]
#[serial]
async fn test_tomlq_scalar_serialization() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file
    let toml_content = r#"name = "test"
values = [1, 2, 3]
"#;
    fs::write(temp_path.join("scalar_test.toml"), toml_content).unwrap();
    
    // Test scalar output in TOML format (should handle gracefully)
    let tool = TomlQueryTool {
        file_path: "scalar_test.toml".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "toml".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    // Should output the raw value since TOML can't serialize bare scalars
    assert_eq!(output.trim(), "test");
    
    // Test array output in TOML format
    let array_tool = TomlQueryTool {
        file_path: "scalar_test.toml".to_string(),
        query: ".values".to_string(),
        operation: "read".to_string(),
        output_format: "toml".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = array_tool.call_with_context(&context).await;
    
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    // Should output an array with the values 1, 2, 3
    assert!(output.contains('[') && output.contains('1') && output.contains('2') && output.contains('3') && output.contains(']'));
}

#[tokio::test]
#[serial]
async fn test_tomlq_array_access() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file with arrays
    let toml_content = r#"
[[servers]]
name = "web1"
port = 8080

[[servers]]
name = "web2"
port = 8081

[config]
ports = [80, 443, 8080]
"#;
    fs::write(temp_path.join("arrays.toml"), toml_content).unwrap();
    
    // Test array index access
    let tool = TomlQueryTool {
        file_path: "arrays.toml".to_string(),
        query: ".servers[0].name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "web1");
    
    // Test nested array access
    let nested_tool = TomlQueryTool {
        file_path: "arrays.toml".to_string(),
        query: ".config.ports[1]".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = nested_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "443");
}

#[tokio::test]
#[serial]
async fn test_tomlq_write_operations() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file
    let toml_content = r#"name = "original"
version = "1.0.0"

[config]
debug = false
"#;
    let file_path = temp_path.join("write.toml");
    fs::write(&file_path, toml_content).unwrap();
    
    // Read the file first (required for write operations)
    let read_tool = TomlQueryTool {
        file_path: "write.toml".to_string(),
        query: ".".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    read_tool.call_with_context(&context).await.unwrap();
    
    // Test simple write operation
    let write_tool = TomlQueryTool {
        file_path: "write.toml".to_string(),
        query: r#".name = "updated""#.to_string(),
        operation: "write".to_string(),
        output_format: "toml".to_string(),
        in_place: true,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = write_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    
    // Verify the change
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains(r#"name = "updated""#));
    
    // Verify backup was created
    assert!(temp_path.join("write.toml.bak").exists());
    
    // Test nested write
    let nested_write = TomlQueryTool {
        file_path: "write.toml".to_string(),
        query: ".config.debug = true".to_string(),
        operation: "write".to_string(),
        output_format: "toml".to_string(),
        in_place: true,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = nested_write.call_with_context(&context).await;
    assert!(result.is_ok());
    
    // Verify the change
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("debug = true"));
}

#[tokio::test]
#[serial]
async fn test_tomlq_output_formats() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file
    let toml_content = r#"
[package]
name = "test-pkg"
version = "0.1.0"
keywords = ["test", "example"]
"#;
    fs::write(temp_path.join("formats.toml"), toml_content).unwrap();
    
    // Test JSON output format
    let json_tool = TomlQueryTool {
        file_path: "formats.toml".to_string(),
        query: ".package".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = json_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert!(output.contains(r#""name": "test-pkg""#));
    assert!(output.contains(r#""version": "0.1.0""#));
    
    // Test TOML output format
    let toml_tool = TomlQueryTool {
        file_path: "formats.toml".to_string(),
        query: ".package".to_string(),
        operation: "read".to_string(),
        output_format: "toml".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = toml_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert!(output.contains(r#"name = "test-pkg""#));
    assert!(output.contains(r#"version = "0.1.0""#));
}

#[tokio::test]
#[serial]
async fn test_tomlq_error_handling() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Test file not found
    let missing_tool = TomlQueryTool {
        file_path: "nonexistent.toml".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = missing_tool.call_with_context(&context).await;
    assert!(result.is_err());
    
    // Test invalid TOML
    fs::write(temp_path.join("invalid.toml"), "invalid = toml content").unwrap();
    let invalid_tool = TomlQueryTool {
        file_path: "invalid.toml".to_string(),
        query: ".name".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = invalid_tool.call_with_context(&context).await;
    assert!(result.is_err());
    
    // Test valid pipe operation (now supported with QueryEngine!)
    let toml_content = r#"name = "test"
value = 42
enabled = true"#;
    fs::write(temp_path.join("pipe.toml"), toml_content).unwrap();
    
    let pipe_tool = TomlQueryTool {
        file_path: "pipe.toml".to_string(),
        query: ". | keys".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = pipe_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    // Debug: print the actual output
    eprintln!("Actual output: {}", output);
    // Should return array of keys - check for the array structure
    assert!(output.contains("[") && output.contains("]"));
    assert!(output.contains("enabled") && output.contains("name") && output.contains("value"));
}

#[tokio::test]
#[serial]
async fn test_tomlq_advanced_queries() {
    let (temp_dir, context) = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test TOML file with more complex data
    let toml_content = r#"
[package]
name = "test-pkg"
version = "1.0.0"

[[dependencies]]
name = "serde"
version = "1.0"

[[dependencies]]
name = "tokio"
version = "1.0"

[features]
default = ["std"]
std = []
"#;
    fs::write(temp_path.join("advanced.toml"), toml_content).unwrap();
    
    // Test that pipe operations now work
    let pipe_tool = TomlQueryTool {
        file_path: "advanced.toml".to_string(),
        query: ". | keys".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = pipe_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert!(output.contains("dependencies") && output.contains("features") && output.contains("package"));
    
    // Test basic functions work
    let type_tool = TomlQueryTool {
        file_path: "advanced.toml".to_string(),
        query: ".package | type".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = type_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "object");
    
    // Test alternative operator
    let alt_tool = TomlQueryTool {
        file_path: "advanced.toml".to_string(),
        query: ".nonexistent // \"default\"".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = alt_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    let output = extract_text_content(&result.unwrap());
    assert_eq!(output.trim(), "default");
}