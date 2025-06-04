use mcp_projectfiles_core::tools::{
    CopyTool, WriteTool, TouchTool, MkdirTool, DeleteTool, MoveTool, ChmodTool, EditTool,
    TomlQueryTool, YamlQueryTool, JsonQueryTool,
};
use mcp_projectfiles_core::context::{StatefulTool, ToolContext};
use std::os::unix::fs::symlink as unix_symlink;
use tempfile::TempDir;
use tokio::fs;

/// Helper to setup test environment with symlinks
async fn setup_symlink_test_env() -> (TempDir, TempDir, ToolContext) {
    // Create project directory
    let project_dir = TempDir::new().unwrap();
    let project_root = project_dir.path().canonicalize().unwrap();
    
    // Create external directory (outside project)
    let external_dir = TempDir::new().unwrap();
    let external_root = external_dir.path().canonicalize().unwrap();
    
    // Create some files in external directory
    fs::write(external_root.join("external.txt"), "External content").await.unwrap();
    fs::create_dir(external_root.join("subdir")).await.unwrap();
    fs::write(external_root.join("subdir/data.json"), r#"{"key": "value"}"#).await.unwrap();
    fs::write(external_root.join("config.toml"), "[section]\nkey = \"value\"").await.unwrap();
    fs::write(external_root.join("data.yaml"), "section:\n  key: value").await.unwrap();
    
    // Create symlink from project to external directory
    unix_symlink(&external_root, project_root.join("external_link")).unwrap();
    
    // Create context with project root
    let context = ToolContext::with_project_root(project_root);
    
    (project_dir, external_dir, context)
}

#[tokio::test]
async fn test_copy_tool_blocks_symlink_destination() {
    let (project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Create a file in project directory
    let project_root = project_dir.path();
    fs::write(project_root.join("source.txt"), "Source content").await.unwrap();
    
    // Try to copy to symlinked directory
    let copy_tool = CopyTool {
        source: "source.txt".to_string(),
        destination: "external_link/copied.txt".to_string(),
        overwrite: false,
        preserve_metadata: true,
    };
    
    let result = copy_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("outside the project directory"));
}

#[tokio::test]
async fn test_write_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to write through symlink
    let write_tool = WriteTool {
        path: "external_link/new_file.txt".to_string(),
        content: "Should not be written".to_string(),
        append: false,
        backup: false,
        encoding: "utf-8".to_string(),
    };
    
    let result = write_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path would be outside the project directory") || 
            error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_touch_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to touch file through symlink
    let touch_tool = TouchTool {
        path: "external_link/touched.txt".to_string(),
        create: true,
        update_atime: true,
        update_mtime: true,
        atime: None,
        mtime: None,
        reference: None,
    };
    
    let result = touch_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path would be outside the project directory"));
}

#[tokio::test]
async fn test_mkdir_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to create directory through symlink
    let mkdir_tool = MkdirTool {
        path: "external_link/new_dir".to_string(),
        parents: true,
        mode: None,
    };
    
    let result = mkdir_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path would be outside the project directory"));
}

#[tokio::test]
async fn test_delete_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to delete file through symlink
    let delete_tool = DeleteTool {
        path: "external_link/external.txt".to_string(),
        recursive: false,
        confirm: true,
        force: false,
        pattern: false,
    };
    
    let result = delete_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_move_tool_blocks_symlink_source() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to move file from symlinked directory
    let move_tool = MoveTool {
        source: "external_link/external.txt".to_string(),
        destination: "moved.txt".to_string(),
        overwrite: false,
        preserve_metadata: true,
    };
    
    let result = move_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Source path is outside the project directory"));
}

#[tokio::test]
async fn test_chmod_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to chmod file through symlink
    let chmod_tool = ChmodTool {
        path: "external_link/external.txt".to_string(),
        mode: "755".to_string(),
        recursive: false,
        pattern: false,
    };
    
    let result = chmod_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_edit_tool_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to edit file through symlink
    let edit_tool = EditTool {
        path: "external_link/external.txt".to_string(),
        old: Some("External".to_string()),
        new: Some("Modified".to_string()),
        expected: None,
        edits: None,
        show_diff: false,
        dry_run: false,
    };
    
    let result = edit_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_tomlq_write_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to write TOML through symlink
    let tomlq_tool = TomlQueryTool {
        file_path: "external_link/config.toml".to_string(),
        query: ".section.newkey = \"newvalue\"".to_string(),
        operation: "write".to_string(),
        output_format: "toml".to_string(),
        in_place: true,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = tomlq_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_yq_write_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to write YAML through symlink
    let yq_tool = YamlQueryTool {
        file_path: "external_link/data.yaml".to_string(),
        query: ".section.newkey = \"newvalue\"".to_string(),
        operation: "write".to_string(),
        output_format: "yaml".to_string(),
        in_place: true,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = yq_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_jq_write_blocks_symlink_path() {
    let (_project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Try to write JSON through symlink
    let jq_tool = JsonQueryTool {
        file_path: "external_link/subdir/data.json".to_string(),
        query: ".newkey = \"newvalue\"".to_string(),
        operation: "write".to_string(),
        output_format: "json".to_string(),
        in_place: true,
        backup: false,
        follow_symlinks: true,
    };
    
    let result = jq_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_copy_allows_within_project() {
    let (project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Create a file in project directory
    let project_root = project_dir.path();
    fs::write(project_root.join("source.txt"), "Source content").await.unwrap();
    
    // Copy within project should work
    let copy_tool = CopyTool {
        source: "source.txt".to_string(),
        destination: "dest.txt".to_string(),
        overwrite: false,
        preserve_metadata: true,
    };
    
    let result = copy_tool.call_with_context(&context).await;
    assert!(result.is_ok());
    assert!(project_root.join("dest.txt").exists());
}

#[tokio::test]
async fn test_nested_symlink_blocked() {
    let (project_dir, external_dir, context) = setup_symlink_test_env().await;
    
    // Create a subdirectory in project
    let project_root = project_dir.path();
    fs::create_dir(project_root.join("subdir")).await.unwrap();
    
    // Create nested symlink: project/subdir/link -> external
    unix_symlink(external_dir.path(), project_root.join("subdir/nested_link")).unwrap();
    
    // Try to write through nested symlink
    let write_tool = WriteTool {
        path: "subdir/nested_link/file.txt".to_string(),
        content: "Should not be written".to_string(),
        append: false,
        backup: false,
        encoding: "utf-8".to_string(),
    };
    
    let result = write_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path would be outside the project directory") || 
            error_msg.contains("Path is outside the project directory"));
}

#[tokio::test]
async fn test_symlink_to_parent_directory_blocked() {
    let (project_dir, _external_dir, context) = setup_symlink_test_env().await;
    
    // Create symlink to parent directory
    let project_root = project_dir.path();
    let parent_dir = project_root.parent().unwrap();
    unix_symlink(parent_dir, project_root.join("parent_link")).unwrap();
    
    // Try to write through parent link
    let write_tool = WriteTool {
        path: "parent_link/dangerous.txt".to_string(),
        content: "Should not be written".to_string(),
        append: false,
        backup: false,
        encoding: "utf-8".to_string(),
    };
    
    let result = write_tool.call_with_context(&context).await;
    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(error_msg.contains("Path would be outside the project directory") || 
            error_msg.contains("Path is outside the project directory"));
}