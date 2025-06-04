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