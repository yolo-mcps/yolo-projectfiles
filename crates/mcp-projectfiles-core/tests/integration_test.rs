use mcp_projectfiles_core::tools::{ListTool, GrepTool};
use mcp_projectfiles_core::context::ToolContext;
use mcp_projectfiles_core::StatefulTool;
use mcp_projectfiles_core::protocol::CallToolResultContentItem;
use mcp_projectfiles_core::config::{init_project_root, reset_project_root};
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
fn setup_test_env() -> TempDir {
    // Create a new temp directory for this test
    let temp_dir = TempDir::new().unwrap();
    
    // Reset and re-initialize project root for this test
    reset_project_root();
    init_project_root(temp_dir.path().to_path_buf());
    
    temp_dir
}

#[tokio::test]
#[serial]
async fn test_list_tool_basic() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create test files
    fs::write(temp_path.join("file1.txt"), "content1").unwrap();
    fs::write(temp_path.join("file2.rs"), "content2").unwrap();
    fs::create_dir(temp_path.join("subdir")).unwrap();
    fs::write(temp_path.join("subdir/file3.py"), "content3").unwrap();
    fs::write(temp_path.join(".hidden"), "hidden").unwrap();
    
    // Test basic listing
    let tool = ListTool {
        path: ".".to_string(),
        recursive: false,
        filter: None,
        sort_by: "name".to_string(),
        show_hidden: false,
        show_metadata: false,
    };
    
    let result = tool.call().await.unwrap();
    let output = extract_text_content(&result);
    assert!(output.contains("[FILE] file1.txt"));
    assert!(output.contains("[FILE] file2.rs"));
    assert!(output.contains("[DIR] subdir"));
    assert!(!output.contains(".hidden")); // Hidden files should not show
    assert!(!output.contains("file3.py")); // Not recursive
}

#[tokio::test]
#[serial]
async fn test_list_tool_recursive() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    
    // Create nested structure
    fs::create_dir_all(temp_path.join("a/b/c")).unwrap();
    fs::write(temp_path.join("a/file1.txt"), "content").unwrap();
    fs::write(temp_path.join("a/b/file2.txt"), "content").unwrap();
    fs::write(temp_path.join("a/b/c/file3.txt"), "content").unwrap();
    
    let tool = ListTool {
        path: ".".to_string(),
        recursive: true,
        filter: None,
        sort_by: "name".to_string(),
        show_hidden: false,
        show_metadata: false,
    };
    
    let result = tool.call().await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("a/file1.txt"));
    assert!(output.contains("a/b/file2.txt"));
    assert!(output.contains("a/b/c/file3.txt"));
}

#[tokio::test]
#[serial]
async fn test_list_tool_filter() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    
    fs::write(temp_path.join("test.rs"), "rust").unwrap();
    fs::write(temp_path.join("test.py"), "python").unwrap();
    fs::write(temp_path.join("test.js"), "javascript").unwrap();
    fs::write(temp_path.join("readme.md"), "docs").unwrap();
    
    let tool = ListTool {
        path: ".".to_string(),
        recursive: false,
        filter: Some("*.rs".to_string()),
        sort_by: "name".to_string(),
        show_hidden: false,
        show_metadata: false,
    };
    
    let result = tool.call().await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("test.rs"));
    assert!(!output.contains("test.py"));
    assert!(!output.contains("test.js"));
    assert!(!output.contains("readme.md"));
}

#[tokio::test]
#[serial]
async fn test_list_tool_sort_by_size() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    
    fs::write(temp_path.join("small.txt"), "a").unwrap();
    fs::write(temp_path.join("medium.txt"), "hello world").unwrap();
    fs::write(temp_path.join("large.txt"), "a".repeat(1000)).unwrap();
    
    let tool = ListTool {
        path: ".".to_string(),
        recursive: false,
        filter: None,
        sort_by: "size".to_string(),
        show_hidden: false,
        show_metadata: false,
    };
    
    let result = tool.call().await.unwrap();
    let output = extract_text_content(&result);
    let lines: Vec<&str> = output.lines().collect();
    
    // Files should be sorted by size
    let small_idx = lines.iter().position(|l| l.contains("small.txt")).unwrap();
    let medium_idx = lines.iter().position(|l| l.contains("medium.txt")).unwrap();
    let large_idx = lines.iter().position(|l| l.contains("large.txt")).unwrap();
    
    assert!(small_idx < medium_idx);
    assert!(medium_idx < large_idx);
}

#[tokio::test]
#[serial]
async fn test_list_invalid_sort() {
    let temp_dir = setup_test_env();
    let _temp_path = temp_dir.path();
    
    let tool = ListTool {
        path: ".".to_string(),
        recursive: false,
        filter: None,
        sort_by: "invalid".to_string(),
        show_hidden: false,
        show_metadata: false,
    };
    
    let result = tool.call().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Invalid sort_by value"));
}

#[tokio::test]
#[serial]
async fn test_grep_tool_basic() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    let context = ToolContext::new();
    
    fs::write(temp_path.join("test.txt"), "hello world\nfoo bar\nhello again").unwrap();
    fs::write(temp_path.join("other.txt"), "no match here").unwrap();
    
    let tool = GrepTool {
        pattern: "hello".to_string(),
        path: ".".to_string(),
        include: None,
        exclude: None,
        case_insensitive: false,
        show_line_numbers: true,
        context_before: 0,
        context_after: 0,
        max_results: 0, // 0 means no limit
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("test.txt"));
    assert!(output.contains("1:\thello world"));
    assert!(output.contains("3:\thello again"));
    assert!(!output.contains("other.txt"));
}

#[tokio::test]
#[serial]
async fn test_grep_tool_case_insensitive() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    let context = ToolContext::new();
    
    fs::write(temp_path.join("test.txt"), "Hello World\nHELLO WORLD\nhello world").unwrap();
    
    let tool = GrepTool {
        pattern: "hello".to_string(),
        path: ".".to_string(),
        include: None,
        exclude: None,
        case_insensitive: true,
        show_line_numbers: true,
        context_before: 0,
        context_after: 0,
        max_results: 0,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("1:\tHello World"));
    assert!(output.contains("2:\tHELLO WORLD"));
    assert!(output.contains("3:\thello world"));
}

#[tokio::test]
#[serial]
async fn test_grep_tool_context() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    let context = ToolContext::new();
    
    fs::write(temp_path.join("test.txt"), "line1\nline2\nmatch\nline4\nline5").unwrap();
    
    let tool = GrepTool {
        pattern: "match".to_string(),
        path: ".".to_string(),
        include: None,
        exclude: None,
        case_insensitive: false,
        show_line_numbers: true,
        context_before: 1,
        context_after: 1,
        max_results: 0,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("2-\tline2"));  // Context before
    assert!(output.contains("3:\tmatch"));   // Match
    assert!(output.contains("4-\tline4"));   // Context after
}

#[tokio::test]
#[serial]
async fn test_grep_tool_file_filter() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    let context = ToolContext::new();
    
    fs::write(temp_path.join("test.rs"), "fn main() { println!(\"match\"); }").unwrap();
    fs::write(temp_path.join("test.py"), "print('match')").unwrap();
    fs::write(temp_path.join("test.txt"), "match").unwrap();
    
    let tool = GrepTool {
        pattern: "match".to_string(),
        path: ".".to_string(),
        include: Some("*.rs".to_string()),
        exclude: None,
        case_insensitive: false,
        show_line_numbers: true,
        context_before: 0,
        context_after: 0,
        max_results: 0,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let output = extract_text_content(&result);
    
    assert!(output.contains("test.rs"));
    assert!(!output.contains("test.py"));
    assert!(!output.contains("test.txt"));
}

#[tokio::test]
#[serial]
async fn test_grep_tool_max_results() {
    let temp_dir = setup_test_env();
    let temp_path = temp_dir.path();
    let context = ToolContext::new();
    
    let content = "match\n".repeat(10);
    fs::write(temp_path.join("test.txt"), content).unwrap();
    
    let tool = GrepTool {
        pattern: "match".to_string(),
        path: ".".to_string(),
        include: None,
        exclude: None,
        case_insensitive: false,
        show_line_numbers: true,
        context_before: 0,
        context_after: 0,
        max_results: 3,
    };
    
    let result = tool.call_with_context(&context).await.unwrap();
    let output = extract_text_content(&result);
    
    // Verify we got exactly 3 match lines
    let lines: Vec<&str> = output.lines().collect();
    let match_lines = lines.iter().filter(|line| line.contains(":\tmatch")).count();
    assert_eq!(match_lines, 3);
    assert!(output.contains("[limited to 3 results]"));
}