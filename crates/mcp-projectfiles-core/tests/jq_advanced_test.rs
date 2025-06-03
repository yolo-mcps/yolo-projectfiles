use mcp_projectfiles_core::tools::JsonQueryTool;
use mcp_projectfiles_core::context::{ToolContext, StatefulTool};
use rust_mcp_schema::CallToolResultContentItem;
use serde_json::json;
use tempfile::TempDir;
use tokio::fs;

async fn setup_test_context() -> (ToolContext, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let context = ToolContext::with_project_root(temp_dir.path().to_path_buf());
    (context, temp_dir)
}

async fn create_test_file(temp_dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let file_path = temp_dir.path().join(name);
    fs::write(&file_path, content).await.unwrap();
    file_path
}

fn extract_text_content(result: &rust_mcp_schema::CallToolResult) -> &str {
    match &result.content[0] {
        CallToolResultContentItem::TextContent(text) => &text.text,
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_array_iteration() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "items": ["apple", "banana", "cherry"]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".items[]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_map_operation() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": [
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25},
            {"name": "Charlie", "age": 35}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".users | map(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(["Alice", "Bob", "Charlie"]));
}

#[tokio::test]
async fn test_select_operation() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": [
            {"name": "Alice", "age": 30, "active": true},
            {"name": "Bob", "age": 25, "active": false},
            {"name": "Charlie", "age": 35, "active": true}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".users | select(.active == true)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["name"], "Alice");
    assert_eq!(arr[1]["name"], "Charlie");
}

#[tokio::test]
async fn test_pipe_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "data": {
            "users": [
                {"name": "Alice", "score": 85},
                {"name": "Bob", "score": 92},
                {"name": "Charlie", "score": 78}
            ]
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".data.users | select(.score > 80) | map(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(["Alice", "Bob"]));
}

#[tokio::test]
async fn test_keys_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "zebra": 1,
        "apple": 2,
        "banana": 3
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: "keys".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    // Keys should be sorted
    assert_eq!(parsed, json!(["apple", "banana", "zebra"]));
}

#[tokio::test]
async fn test_values_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "a": 10,
        "b": 20,
        "c": 30
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: "values".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    // Values may not be in order, but all should be present
    assert!(arr.contains(&json!(10)));
    assert!(arr.contains(&json!(20)));
    assert!(arr.contains(&json!(30)));
}

#[tokio::test]
async fn test_length_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "str": "hello",
        "arr": [1, 2, 3, 4, 5],
        "obj": {"a": 1, "b": 2}
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test string length
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".str | length".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "5");
    
    // Test array length
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".arr | length".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "5");
    
    // Test object length
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".obj | length".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "2");
}

#[tokio::test]
async fn test_type_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "str": "hello",
        "num": 42,
        "bool": true,
        "nil": null,
        "arr": [],
        "obj": {}
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test string type
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".str | type".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "string");
    
    // Test number type
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".num | type".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "number");
}

#[tokio::test]
async fn test_comparison_in_select() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "products": [
            {"name": "Apple", "price": 1.5},
            {"name": "Banana", "price": 0.8},
            {"name": "Orange", "price": 2.0},
            {"name": "Grape", "price": 3.5}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test greater than
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".products | select(.price > 1.5)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["name"], "Orange");
    assert_eq!(arr[1]["name"], "Grape");
    
    // Test less than
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".products | select(.price < 1.5)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "Banana");
}

#[tokio::test]
async fn test_complex_nested_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "departments": [
            {
                "name": "Engineering",
                "employees": [
                    {"name": "Alice", "salary": 120000, "active": true},
                    {"name": "Bob", "salary": 95000, "active": true},
                    {"name": "Charlie", "salary": 85000, "active": false}
                ]
            },
            {
                "name": "Sales",
                "employees": [
                    {"name": "David", "salary": 80000, "active": true},
                    {"name": "Eve", "salary": 110000, "active": true}
                ]
            }
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Get all active employees' names across all departments
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".departments[].employees | select(.active == true) | map(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // We should get nested arrays from each department
    assert!(parsed.is_array());
}

#[tokio::test]
async fn test_recursive_descent() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "name": "root",
        "users": [
            {
                "name": "Alice",
                "profile": {
                    "name": "Alice Profile",
                    "settings": {
                        "name": "Alice Settings"
                    }
                }
            },
            {
                "name": "Bob",
                "data": {
                    "name": "Bob Data"
                }
            }
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: "..name".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // Should find all "name" fields at any depth
    assert!(parsed.is_array());
    let names = parsed.as_array().unwrap();
    assert!(names.len() >= 4); // At least root, Alice, Bob, and one nested
    assert!(names.contains(&json!("root")));
    assert!(names.contains(&json!("Alice")));
    assert!(names.contains(&json!("Bob")));
}

#[tokio::test]
async fn test_wildcard_query() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": {
            "alice": {"age": 30, "city": "NYC"},
            "bob": {"age": 25, "city": "LA"},
            "charlie": {"age": 35, "city": "Chicago"}
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test .users.* to get all user objects
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".users.*".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert!(parsed.is_array());
    let users = parsed.as_array().unwrap();
    assert_eq!(users.len(), 3);
    
    // All users should have age and city fields
    for user in users {
        assert!(user.get("age").is_some());
        assert!(user.get("city").is_some());
    }
}

#[tokio::test]
async fn test_array_wildcard() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "data": {
            "items": [
                {"id": 1, "value": "a"},
                {"id": 2, "value": "b"},
                {"id": 3, "value": "c"}
            ]
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test .data.items[*].value using JSONPath syntax
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".data.items[*].value".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert_eq!(parsed, json!(["a", "b", "c"]));
}

#[tokio::test]
async fn test_complex_recursive_and_filter() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "company": {
            "departments": [
                {
                    "name": "Engineering",
                    "teams": [
                        {
                            "name": "Frontend",
                            "members": [
                                {"name": "Alice", "role": "lead"},
                                {"name": "Bob", "role": "developer"}
                            ]
                        },
                        {
                            "name": "Backend",
                            "members": [
                                {"name": "Charlie", "role": "lead"},
                                {"name": "David", "role": "developer"}
                            ]
                        }
                    ]
                }
            ]
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Find all names recursively
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: "..name".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    let names = parsed.as_array().unwrap();
    assert!(names.contains(&json!("Engineering")));
    assert!(names.contains(&json!("Frontend")));
    assert!(names.contains(&json!("Backend")));
    assert!(names.contains(&json!("Alice")));
    assert!(names.contains(&json!("Bob")));
    assert!(names.contains(&json!("Charlie")));
    assert!(names.contains(&json!("David")));
}

#[tokio::test]
async fn test_object_construction() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "user": {
            "firstName": "John",
            "lastName": "Doe",
            "age": 30,
            "email": "john@example.com"
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test creating a new object with selected fields
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"{name: .user.firstName, email: .user.email, adult: true}"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert_eq!(parsed["name"], "John");
    assert_eq!(parsed["email"], "john@example.com");
    assert_eq!(parsed["adult"], true);
}

#[tokio::test]
async fn test_to_entries() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "a": 1,
        "b": 2,
        "c": 3
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: "to_entries".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert!(parsed.is_array());
    let entries = parsed.as_array().unwrap();
    assert_eq!(entries.len(), 3);
    
    // Each entry should have key and value
    for entry in entries {
        assert!(entry.get("key").is_some());
        assert!(entry.get("value").is_some());
    }
}

#[tokio::test]
async fn test_from_entries() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "data": [
            {"key": "name", "value": "Alice"},
            {"key": "age", "value": 30},
            {"key": "active", "value": true}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".data | from_entries".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert!(parsed.is_object());
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["age"], 30);
    assert_eq!(parsed["active"], true);
}

#[tokio::test]
async fn test_object_construction_with_map() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com"},
            {"id": 2, "name": "Bob", "email": "bob@example.com"}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Transform array of users into simplified format
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".users | map({username: .name, contact: .email})"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    assert!(parsed.is_array());
    let users = parsed.as_array().unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["username"], "Alice");
    assert_eq!(users[0]["contact"], "alice@example.com");
    assert_eq!(users[1]["username"], "Bob");
    assert_eq!(users[1]["contact"], "bob@example.com");
}

#[tokio::test]
async fn test_arithmetic_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "price": 100,
        "tax_rate": 0.08,
        "quantity": 3
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test addition
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".price + 50".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "150.0");
    
    // Test multiplication
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".price * .quantity".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "300.0");
    
    // Test complex expression
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".price * (1 + .tax_rate)".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let value: f64 = extract_text_content(&result).trim().parse().unwrap();
    assert!((value - 108.0).abs() < 0.001);
}

#[tokio::test]
async fn test_string_concatenation() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "first": "Hello",
        "last": "World"
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".first + " " + .last"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "Hello World");
}

#[tokio::test]
async fn test_array_concatenation() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "arr1": [1, 2, 3],
        "arr2": [4, 5, 6]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".arr1 + .arr2".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 2, 3, 4, 5, 6]));
}

#[tokio::test]
async fn test_arithmetic_in_map() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "items": [
            {"name": "A", "price": 10},
            {"name": "B", "price": 20},
            {"name": "C", "price": 30}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Get prices with 10% discount
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".items[0].price * 0.9".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "9.0");
    
    // Test arithmetic with array element
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".items[1].price + .items[2].price".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "50.0");
}

#[tokio::test]
async fn test_string_functions() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "text": "  Hello World  ",
        "email": "user@example.com",
        "csv": "apple,banana,cherry",
        "number_str": "42.5"
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test trim
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".text | trim".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "Hello World");
    
    // Test ascii_downcase
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".text | trim | ascii_downcase".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "hello world");
    
    // Test split
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".csv | split(",")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(["apple", "banana", "cherry"]));
    
    // Test contains
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".email | contains("@")"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "true");
    
    // Test tonumber
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: ".number_str | tonumber".to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "42.5");
}

#[tokio::test]
async fn test_join_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "words": ["Hello", "World", "!"],
        "numbers": [1, 2, 3]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test join with strings
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".words | join(" ")"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "Hello World !");
    
    // Test join with numbers
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".numbers | join("-")"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "1-2-3");
}

#[tokio::test]
async fn test_string_functions_in_map() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": [
            {"name": "alice smith", "email": "alice@example.com"},
            {"name": "bob jones", "email": "bob@gmail.com"}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test string function on single user
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".users[0].name | ascii_upcase"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "ALICE SMITH");
    
    // Test split on email
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".users[1].email | split("@")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(["bob", "gmail.com"]));
}

#[tokio::test]
async fn test_if_then_else_basic() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "age": 25,
        "status": "active",
        "score": 85
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test simple if-then-else
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .age > 18 then "adult" else "minor" end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "adult");
    
    // Test if-then without else
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .status == "active" then .score end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "85");
    
    // Test false condition without else
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .status == "inactive" then .score end"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(null));
}

#[tokio::test]
async fn test_if_then_else_nested() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "score": 75
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test nested if-then-else
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .score > 90 then "A" else if .score > 80 then "B" else if .score > 70 then "C" else "F" end end end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "C");
}

#[tokio::test]
async fn test_if_then_else_with_expressions() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "user": {
            "name": "Alice",
            "premium": true,
            "discount": 0.2
        },
        "price": 100
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test with complex expressions in branches
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .user.premium then .price * (1 - .user.discount) else .price end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "80.0");
    
    // Test with object construction in branches
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .user.premium then {name: .user.name, finalPrice: .price * (1 - .user.discount)} else {name: .user.name, finalPrice: .price} end"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["finalPrice"], 80.0);
}

#[tokio::test]
async fn test_if_then_else_in_map() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "users": [
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 15},
            {"name": "Charlie", "age": 25}
        ]
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test if-then-else inside map
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".users | map(if .age >= 18 then .name + " (adult)" else .name + " (minor)" end)"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!([
        "Alice (adult)",
        "Bob (minor)",
        "Charlie (adult)"
    ]));
}

#[tokio::test]
async fn test_boolean_operators() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "user": {
            "age": 25,
            "active": true,
            "premium": false
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test AND operator
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .user.age > 18 and .user.active then "eligible" else "not eligible" end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "eligible");
    
    // Test OR operator
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if .user.premium or .user.age > 20 then "special" else "regular" end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "special");
    
    // Test NOT operator
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if not .user.premium then "standard" else "premium" end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "standard");
    
    // Test complex boolean expression
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"if (.user.age > 18 and .user.active) or .user.premium then "approved" else "denied" end"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "approved");
}

#[tokio::test]
async fn test_alternative_operator() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "name": "Test",
        "config": {
            "timeout": null,
            "retries": 3
        },
        "optional": null
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test basic alternative operator
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".config.timeout // 30"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "30");
    
    // Test with non-null value
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".config.retries // 5"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "3");
    
    // Test with missing field
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".config.missing // "default""#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "default");
    
    // Test chained alternatives
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".optional // .config.timeout // 100"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "100");
}

#[tokio::test]
async fn test_optional_operator() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "user": {
            "name": "Alice",
            "profile": {
                "email": "alice@example.com"
            }
        }
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test optional field access - existing field
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".user.name?"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "Alice");
    
    // Test optional field access - missing field
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".user.age?"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(null));
    
    // Test chained optional access
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".user.?settings.theme"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(null));
    
    // Test optional with alternative operator
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#".user.age? // 25"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "25");
}

#[tokio::test]
async fn test_try_catch() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "data": {
            "value": 42
        },
        "fallback": 100
    });
    create_test_file(&temp_dir, "test.json", &content.to_string()).await;
    
    // Test try without error
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"try .data.value"#.to_string(),
        operation: "read".to_string(),
        output_format: "raw".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    assert_eq!(extract_text_content(&result).trim(), "42");
    
    // Test try with missing field returns null (not an error in jq)
    let tool = JsonQueryTool {
        file_path: "test.json".to_string(),
        query: r#"try .missing.field.deep"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(extract_text_content(&result)).unwrap();
    assert_eq!(parsed, json!(null));
}

#[tokio::test]
async fn test_array_add_function() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test summing numbers
    let content = json!([1, 2, 3, 4, 5]);
    create_test_file(&temp_dir, "numbers.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "add".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(15.0));
    
    // Test concatenating strings
    let content = json!(["hello", " ", "world"]);
    create_test_file(&temp_dir, "strings.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "strings.json".to_string(),
        query: "add".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!("hello world"));
}

#[tokio::test]
async fn test_array_min_max_functions() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([5, 2, 8, 1, 9, 3]);
    create_test_file(&temp_dir, "numbers.json", &content.to_string()).await;
    
    // Test min
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "min".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(1.0));
    
    // Test max
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "max".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(9.0));
}

#[tokio::test]
async fn test_array_unique_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([1, 2, 2, 3, 1, 4, 3, 5]);
    create_test_file(&temp_dir, "duplicates.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "duplicates.json".to_string(),
        query: "unique".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // unique returns sorted unique values
    assert_eq!(parsed, json!([1, 2, 3, 4, 5]));
}

#[tokio::test]
async fn test_array_reverse_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([1, 2, 3, 4, 5]);
    create_test_file(&temp_dir, "ordered.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "ordered.json".to_string(),
        query: "reverse".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([5, 4, 3, 2, 1]));
}

#[tokio::test]
async fn test_array_sort_function() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test sorting numbers
    let content = json!([5, 2, 8, 1, 9, 3]);
    create_test_file(&temp_dir, "numbers.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "sort".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 2, 3, 5, 8, 9]));
    
    // Test sorting strings
    let content = json!(["zebra", "apple", "mango", "banana"]);
    create_test_file(&temp_dir, "strings.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "strings.json".to_string(),
        query: "sort".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(["apple", "banana", "mango", "zebra"]));
}

#[tokio::test]
async fn test_array_sort_by_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([
        {"name": "alice", "age": 30},
        {"name": "bob", "age": 25},
        {"name": "charlie", "age": 35}
    ]);
    create_test_file(&temp_dir, "users.json", &content.to_string()).await;
    
    // Sort by age
    let tool = JsonQueryTool {
        file_path: "users.json".to_string(),
        query: "sort_by(.age)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(
        parsed,
        json!([
            {"name": "bob", "age": 25},
            {"name": "alice", "age": 30},
            {"name": "charlie", "age": 35}
        ])
    );
    
    // Sort by name
    let tool = JsonQueryTool {
        file_path: "users.json".to_string(),
        query: "sort_by(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(
        parsed,
        json!([
            {"name": "alice", "age": 30},
            {"name": "bob", "age": 25},
            {"name": "charlie", "age": 35}
        ])
    );
}

#[tokio::test]
async fn test_array_flatten_function() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test basic flatten
    let content = json!([[1, 2], [3, 4], 5, [6, [7, 8]]]);
    create_test_file(&temp_dir, "nested.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "nested.json".to_string(),
        query: "flatten".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 2, 3, 4, 5, 6, [7, 8]]));
    
    // Test flatten with depth
    let tool = JsonQueryTool {
        file_path: "nested.json".to_string(),
        query: "flatten(2)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 2, 3, 4, 5, 6, 7, 8]));
}

#[tokio::test]
async fn test_array_group_by_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([
        {"name": "alice", "team": "red", "score": 10},
        {"name": "bob", "team": "blue", "score": 15},
        {"name": "charlie", "team": "red", "score": 20},
        {"name": "dave", "team": "blue", "score": 5},
        {"name": "eve", "team": "green", "score": 12}
    ]);
    create_test_file(&temp_dir, "players.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "players.json".to_string(),
        query: "group_by(.team)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // Should be grouped by team and sorted by key (blue, green, red)
    assert_eq!(
        parsed,
        json!([
            [
                {"name": "bob", "team": "blue", "score": 15},
                {"name": "dave", "team": "blue", "score": 5}
            ],
            [
                {"name": "eve", "team": "green", "score": 12}
            ],
            [
                {"name": "alice", "team": "red", "score": 10},
                {"name": "charlie", "team": "red", "score": 20}
            ]
        ])
    );
}
#[tokio::test]
async fn test_array_slicing() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    create_test_file(&temp_dir, "numbers.json", &content.to_string()).await;
    
    // Test basic slice [2:5]
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "[2:5]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([2, 3, 4]));
    
    // Test slice from start [:3]
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "[:3]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([0, 1, 2]));
    
    // Test slice to end [7:]
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "[7:]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([7, 8, 9]));
    
    // Test slice with path
    let content = json!({
        "items": [0, 1, 2, 3, 4, 5]
    });
    create_test_file(&temp_dir, "object.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "object.json".to_string(),
        query: ".items[1:4]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 2, 3]));
}

#[tokio::test]
async fn test_indices_function() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test indices in array
    let content = json!([1, 2, 3, 2, 4, 2, 5]);
    create_test_file(&temp_dir, "numbers.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "numbers.json".to_string(),
        query: "indices(2)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([1, 3, 5]));
    
    // Test indices in string
    let content = json!("hello world hello");
    create_test_file(&temp_dir, "string.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "string.json".to_string(),
        query: r#"indices("hello")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!([0, 12]));
}

#[tokio::test]
async fn test_has_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "name": "alice",
        "age": 30,
        "city": "wonderland"
    });
    create_test_file(&temp_dir, "user.json", &content.to_string()).await;
    
    // Test has with existing key
    let tool = JsonQueryTool {
        file_path: "user.json".to_string(),
        query: r#"has("name")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(true));
    
    // Test has with non-existing key
    let tool = JsonQueryTool {
        file_path: "user.json".to_string(),
        query: r#"has("email")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(false));
}

#[tokio::test]
async fn test_del_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "name": "alice",
        "age": 30,
        "city": "wonderland",
        "hobbies": ["reading", "chess", "tea"]
    });
    create_test_file(&temp_dir, "user.json", &content.to_string()).await;
    
    // Test del with object key
    let tool = JsonQueryTool {
        file_path: "user.json".to_string(),
        query: "del(.age)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!({
        "name": "alice",
        "city": "wonderland",
        "hobbies": ["reading", "chess", "tea"]
    }));
}

#[tokio::test]
async fn test_with_entries_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "a": 1,
        "b": 2,
        "c": 3
    });
    create_test_file(&temp_dir, "object.json", &content.to_string()).await;
    
    // Test with_entries to double all values
    let tool = JsonQueryTool {
        file_path: "object.json".to_string(),
        query: r#"with_entries(.value = .value * 2)"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // Check the values are doubled (handle both integer and float representations)
    if let serde_json::Value::Object(map) = parsed {
        assert_eq!(map.get("a").and_then(|v| v.as_f64()), Some(2.0));
        assert_eq!(map.get("b").and_then(|v| v.as_f64()), Some(4.0));
        assert_eq!(map.get("c").and_then(|v| v.as_f64()), Some(6.0));
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_paths_functions() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!({
        "user": {
            "name": "alice",
            "contact": {
                "email": "alice@example.com",
                "phone": "123-456"
            }
        },
        "status": "active"
    });
    create_test_file(&temp_dir, "nested.json", &content.to_string()).await;
    
    // Test leaf_paths
    let tool = JsonQueryTool {
        file_path: "nested.json".to_string(),
        query: "leaf_paths".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // Should contain only paths to leaf values
    let expected_paths = vec![
        json!(["user", "name"]),
        json!(["user", "contact", "email"]),
        json!(["user", "contact", "phone"]),
        json!(["status"])
    ];
    
    if let serde_json::Value::Array(paths) = parsed {
        assert_eq!(paths.len(), 4);
        for path in expected_paths {
            assert!(paths.contains(&path));
        }
    } else {
        panic!("Expected array of paths");
    }
}

#[tokio::test]
async fn test_string_test_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!("hello@example.com");
    create_test_file(&temp_dir, "email.json", &content.to_string()).await;
    
    // Test regex match
    let tool = JsonQueryTool {
        file_path: "email.json".to_string(),
        query: r#"test("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(true));
    
    // Test non-match
    let content = json!("not an email");
    create_test_file(&temp_dir, "text.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "text.json".to_string(),
        query: r#"test("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!(false));
}

#[tokio::test]
async fn test_string_match_function() {
    let (context, temp_dir) = setup_test_context().await;
    let content = json!("The year is 2024");
    create_test_file(&temp_dir, "text.json", &content.to_string()).await;
    
    // Test match with captures
    let tool = JsonQueryTool {
        file_path: "text.json".to_string(),
        query: r#"match("(\d{4})")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    // Check the match structure
    assert_eq!(parsed["string"], "2024");
    assert_eq!(parsed["offset"], 12);
    assert_eq!(parsed["length"], 4);
}

#[tokio::test]
async fn test_string_trim_functions() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test ltrimstr
    let content = json!("prefixHelloWorld");
    create_test_file(&temp_dir, "text.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "text.json".to_string(),
        query: r#"ltrimstr("prefix")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!("HelloWorld"));
    
    // Test rtrimstr
    let content = json!("HelloWorldsuffix");
    create_test_file(&temp_dir, "text2.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "text2.json".to_string(),
        query: r#"rtrimstr("suffix")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed, json!("HelloWorld"));
}

#[tokio::test]
async fn test_math_functions() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test floor
    let content = json!(3.7);
    create_test_file(&temp_dir, "number.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "number.json".to_string(),
        query: "floor".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(3.0));
    
    // Test ceil
    let content = json!(3.2);
    create_test_file(&temp_dir, "number2.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "number2.json".to_string(),
        query: "ceil".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(4.0));
    
    // Test round
    let content = json!(3.5);
    create_test_file(&temp_dir, "number3.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "number3.json".to_string(),
        query: "round".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(4.0));
    
    // Test abs
    let content = json!(-5.3);
    create_test_file(&temp_dir, "number4.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "number4.json".to_string(),
        query: "abs".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!((parsed.as_f64().unwrap() - 5.3).abs() < 0.0001);
    
    // Test modulo operator
    let content = json!({"a": 17, "b": 5});
    create_test_file(&temp_dir, "modulo.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "modulo.json".to_string(),
        query: ".a % .b".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed.as_f64(), Some(2.0));
}

#[tokio::test]
async fn test_debugging_functions() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test empty function
    let content = json!([1, 2, 3]);
    create_test_file(&temp_dir, "array.json", &content.to_string()).await;
    
    let tool = JsonQueryTool {
        file_path: "array.json".to_string(),
        query: "empty".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    // empty returns an empty array as a marker
    assert_eq!(parsed, json!([]));
    
    // Test error function
    let tool = JsonQueryTool {
        file_path: "array.json".to_string(),
        query: r#"error("This is an error message")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await;
    assert!(result.is_err());
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(error_msg.contains("This is an error message"));
    }
}