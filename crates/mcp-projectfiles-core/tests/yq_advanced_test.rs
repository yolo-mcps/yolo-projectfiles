use mcp_projectfiles_core::tools::YamlQueryTool;
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

async fn create_test_yaml_file(temp_dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
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
    let yaml_content = r#"
items:
  - apple
  - banana
  - cherry
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".items[]".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
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
    let yaml_content = r#"
users:
  - name: Alice
    age: 30
  - name: Bob
    age: 25
  - name: Carol
    age: 35
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".users | map(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "yaml".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert!(content.contains("Alice"));
    assert!(content.contains("Bob"));
    assert!(content.contains("Carol"));
}

#[tokio::test]
async fn test_select_operation() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
users:
  - name: Alice
    age: 30
    active: true
  - name: Bob
    age: 25
    active: false
  - name: Carol
    age: 35
    active: true
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".users | map(select(.active))".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let active_users = parsed.as_array().unwrap();
    assert_eq!(active_users.len(), 2);
    
    // Check that only active users are included
    for user in active_users {
        assert_eq!(user["active"], true);
    }
}

#[tokio::test]
async fn test_pipe_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
data:
  items:
    - category: fruit
      name: apple
      price: 1.20
    - category: fruit
      name: banana
      price: 0.80
    - category: vegetable
      name: carrot
      price: 0.50
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".data.items | map(select(.category == \"fruit\")) | map(.name)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let fruit_names = parsed.as_array().unwrap();
    assert_eq!(fruit_names.len(), 2);
    assert!(fruit_names.contains(&json!("apple")));
    assert!(fruit_names.contains(&json!("banana")));
}

#[tokio::test]
async fn test_keys_function() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
name: Alice
age: 30
city: New York
active: true
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: "keys".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let keys = parsed.as_array().unwrap();
    assert_eq!(keys.len(), 4);
    assert!(keys.contains(&json!("name")));
    assert!(keys.contains(&json!("age")));
    assert!(keys.contains(&json!("city")));
    assert!(keys.contains(&json!("active")));
}

#[tokio::test]
async fn test_values_function() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
numbers:
  first: 10
  second: 20
  third: 30
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers | values".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let values = parsed.as_array().unwrap();
    assert_eq!(values.len(), 3);
    assert!(values.contains(&json!(10)));
    assert!(values.contains(&json!(20)));
    assert!(values.contains(&json!(30)));
}

#[tokio::test]
async fn test_length_function() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
arrays:
  short: [1, 2, 3]
  long: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  empty: []
objects:
  small: {a: 1, b: 2}
  large: {a: 1, b: 2, c: 3, d: 4, e: 5}
  empty: {}
strings:
  short: "hello"
  long: "this is a longer string"
  empty: ""
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test array lengths
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".arrays.short | length".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "3");
    
    // Test object lengths
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".objects.large | length".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "5");
    
    // Test string lengths
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".strings.short | length".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "5");
}

#[tokio::test]
async fn test_type_function() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
data:
  text: "hello"
  number: 42
  float: 3.14
  boolean: true
  array: [1, 2, 3]
  object: {key: "value"}
  null_value: null
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let test_cases = vec![
        (".data.text | type", "string"),
        (".data.number | type", "number"),
        (".data.float | type", "number"),
        (".data.boolean | type", "boolean"),
        (".data.array | type", "array"),
        (".data.object | type", "object"),
        (".data.null_value | type", "null"),
    ];
    
    for (query, expected) in test_cases {
        let tool = YamlQueryTool {
            file_path: "test.yaml".to_string(),
            query: query.to_string(),
            operation: "read".to_string(),
            output_format: "json".to_string(),
            in_place: false,
            backup: true,
            follow_symlinks: true,
        };
        
        let result = tool.call_with_context(&context).await.unwrap();
        let content = extract_text_content(&result);
        assert_eq!(content.trim(), format!("\"{}\"", expected));
    }
}

#[tokio::test]
async fn test_arithmetic_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
math:
  a: 10
  b: 3
  c: 2.5
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    let test_cases = vec![
        (".math.a + .math.b", "13"),
        (".math.a - .math.b", "7"),
        (".math.a * .math.b", "30"),
        (".math.a / .math.b", "3.3333333333333335"),
        (".math.a % .math.b", "1"),
        (".math.c * 2", "5"),
    ];
    
    for (query, expected) in test_cases {
        let tool = YamlQueryTool {
            file_path: "test.yaml".to_string(),
            query: query.to_string(),
            operation: "read".to_string(),
            output_format: "json".to_string(),
            in_place: false,
            backup: true,
            follow_symlinks: true,
        };
        
        let result = tool.call_with_context(&context).await.unwrap();
        let content = extract_text_content(&result);
        assert_eq!(content.trim(), expected);
    }
}

#[tokio::test]
async fn test_string_functions() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
text:
  message: "Hello, World!"
  csv: "apple,banana,cherry"
  padded: "  spaced  "
  email: "user@example.com"
  filename: "document.pdf"
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test split function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".text.csv | split(",")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let parts = parsed.as_array().unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], "apple");
    assert_eq!(parts[1], "banana");
    assert_eq!(parts[2], "cherry");
    
    // Test trim function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".text.padded | trim".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"spaced\"");
    
    // Test contains function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".text.email | contains("@")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "true");
    
    // Test startswith function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".text.message | startswith("Hello")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "true");
    
    // Test endswith function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".text.filename | endswith(".pdf")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "true");
}

#[tokio::test]
async fn test_array_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
numbers: [5, 2, 8, 1, 9, 3]
strings: ["apple", "banana", "apple", "cherry"]
nested: [[1, 2], [3, 4], [5, 6]]
users:
  - name: Alice
    category: admin
    score: 95
  - name: Bob
    category: user
    score: 87
  - name: Carol
    category: admin
    score: 92
  - name: David
    category: user
    score: 78
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test sort
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers | sort".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let sorted = parsed.as_array().unwrap();
    assert_eq!(sorted, &vec![json!(1), json!(2), json!(3), json!(5), json!(8), json!(9)]);
    
    // Test sort_by
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".users | sort_by(.score)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let sorted_users = parsed.as_array().unwrap();
    assert_eq!(sorted_users[0]["name"], "David"); // Lowest score
    assert_eq!(sorted_users[3]["name"], "Alice"); // Highest score
    
    // Test unique
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".strings | unique".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let unique_strings = parsed.as_array().unwrap();
    assert_eq!(unique_strings.len(), 3); // "apple", "banana", "cherry"
    
    // Test flatten
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".nested | flatten".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let flattened = parsed.as_array().unwrap();
    assert_eq!(flattened, &vec![json!(1), json!(2), json!(3), json!(4), json!(5), json!(6)]);
    
    // Test group_by
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".users | group_by(.category)".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let groups = parsed.as_array().unwrap();
    assert_eq!(groups.len(), 2); // Two categories: admin and user
}

#[tokio::test]
async fn test_conditional_expressions() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
users:
  - name: Alice
    age: 30
    active: true
  - name: Bob
    age: 17
    active: false
  - name: Carol
    age: 25
    active: true
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test simple if-then-else
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".users | map(if .age >= 18 then "adult" else "minor" end)"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let age_categories = parsed.as_array().unwrap();
    assert_eq!(age_categories[0], "adult");  // Alice
    assert_eq!(age_categories[1], "minor");  // Bob
    assert_eq!(age_categories[2], "adult");  // Carol
    
    // Test complex conditional with boolean operators
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".users | map(select(.age >= 18 and .active))"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let filtered_users = parsed.as_array().unwrap();
    assert_eq!(filtered_users.len(), 2); // Alice and Carol
    
    for user in filtered_users {
        assert!(user["age"].as_u64().unwrap() >= 18);
        assert_eq!(user["active"], true);
    }
}

#[tokio::test]
async fn test_alternative_operator() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
data:
  complete:
    name: "Alice"
    email: "alice@example.com"
  incomplete:
    name: "Bob"
    # email is missing
  empty: {}
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test alternative operator with existing field
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".data.complete.email // "no-email""#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"alice@example.com\"");
    
    // Test alternative operator with missing field
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".data.incomplete.email // "no-email""#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"no-email\"");
}

#[tokio::test]
async fn test_try_catch() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
data:
  valid: "123"
  invalid: "not-a-number"
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test try-catch with successful operation
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#"try .data.valid catch "failed""#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"123\"");
    
    // Test try-catch with failing operation
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#"try .data.nonexistent.field catch "default""#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"default\"");
}

#[tokio::test]
async fn test_object_manipulation() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
person:
  name: "Alice"
  age: 30
  city: "New York"
config:
  - key: "timeout"
    value: 30
  - key: "retries"
    value: 3
  - key: "debug"
    value: true
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test to_entries
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".person | to_entries".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    let entries = parsed.as_array().unwrap();
    assert_eq!(entries.len(), 3);
    
    // Check that each entry has key and value
    for entry in entries {
        assert!(entry["key"].is_string());
        assert!(!entry["value"].is_null());
    }
    
    // Test from_entries
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".config | from_entries".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["timeout"], 30);
    assert_eq!(parsed["retries"], 3);
    assert_eq!(parsed["debug"], true);
    
    // Test has function
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".person | has("age")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "true");
    
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: r#".person | has("email")"#.to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "false");
}

#[tokio::test]
async fn test_write_operations() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
user:
  name: "Alice"
  age: 30
  settings:
    theme: "light"
    notifications: false
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // First, read the file to enable writing
    let read_tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".".to_string(),
        operation: "read".to_string(),
        output_format: "yaml".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let _result = read_tool.call_with_context(&context).await.unwrap();
    
    // Test simple field update
    let write_tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".user.age = 31".to_string(),
        operation: "write".to_string(),
        output_format: "yaml".to_string(),
        in_place: true,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = write_tool.call_with_context(&context).await.unwrap();
    assert!(!result.is_error.unwrap_or(true));
    
    // Verify the change
    let verify_tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".user.age".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = verify_tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "31");
}

#[tokio::test]
async fn test_yaml_specific_features() {
    let (context, temp_dir) = setup_test_context().await;
    
    // Test with YAML-specific features like multi-line strings
    let yaml_content = r#"
description: |
  This is a multi-line
  string in YAML format
  that should be preserved.
config:
  enabled: true
  count: 42
  ratio: 3.14159
  tags:
    - production
    - important
  metadata:
    created: 2023-01-01
    updated: null
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test that YAML types are preserved
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".config.enabled | type".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "\"boolean\"");
    
    // Test multi-line string handling
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".description".to_string(),
        operation: "read".to_string(),
        output_format: "yaml".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert!(content.contains("multi-line"));
    assert!(content.contains("preserved"));
    
    // Test null handling in YAML
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".config.metadata.updated".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "null");
}

#[tokio::test]
async fn test_recursive_descent() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
data:
  users:
    - email: "alice@example.com"
      profile:
        email: "alice.profile@example.com"
    - email: "bob@example.com"
      profile:
        email: "bob.profile@example.com"
  config:
    admin:
      email: "admin@example.com"
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test recursive search for all email fields
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: "..email".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
    
    if parsed.is_array() {
        let emails = parsed.as_array().unwrap();
        assert!(emails.len() >= 5); // Should find all email fields
        
        // Check that we found the expected emails
        let email_strings: Vec<String> = emails.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        
        assert!(email_strings.contains(&"alice@example.com".to_string()));
        assert!(email_strings.contains(&"bob@example.com".to_string()));
        assert!(email_strings.contains(&"admin@example.com".to_string()));
    } else {
        // Single result case
        assert!(parsed.is_string());
    }
}

#[tokio::test]
async fn test_math_functions() {
    let (context, temp_dir) = setup_test_context().await;
    let yaml_content = r#"
numbers:
  positive: 7.8
  negative: -3.2
  integer: 42
  zero: 0
  decimal: 2.7
"#;
    create_test_yaml_file(&temp_dir, "test.yaml", yaml_content).await;
    
    // Test floor
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers.positive | floor".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "7");
    
    // Test ceil
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers.decimal | ceil".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "3");
    
    // Test round
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers.positive | round".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "8");
    
    // Test abs
    let tool = YamlQueryTool {
        file_path: "test.yaml".to_string(),
        query: ".numbers.negative | abs".to_string(),
        operation: "read".to_string(),
        output_format: "json".to_string(),
        in_place: false,
        backup: true,
        follow_symlinks: true,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let content = extract_text_content(&result);
    assert_eq!(content.trim(), "3.2");
}