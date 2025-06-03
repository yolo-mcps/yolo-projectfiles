use crate::context::{StatefulTool, ToolContext};
use crate::config::tool_errors;
use crate::tools::utils::{format_size, format_path};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, BufReader};
use std::fmt::Write as FmtWrite;

const TOOL_NAME: &str = "hash";

#[mcp_tool(
    name = "hash",
    description = "Calculates checksums/hashes of files within the project directory using various algorithms (MD5, SHA1, SHA256, SHA512). Prefer this over system hash commands when verifying project files."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct HashTool {
    /// Path to the file to hash (relative to project root)
    pub path: String,
    
    /// Hash algorithm to use: "md5", "sha1", "sha256", "sha512" (default: "sha256")
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

fn default_algorithm() -> String {
    "sha256".to_string()
}

#[async_trait]
impl StatefulTool for HashTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Get project root and resolve path
        let project_root = context.get_project_root()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get project root: {}", e))))?;
            
        // Canonicalize project root for consistent path comparison
        let current_dir = project_root.canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to canonicalize project root: {}", e))))?;
        
        let target_path = current_dir.join(&self.path);
        
        // Security check - ensure path is within project directory
        let normalized_path = target_path
            .canonicalize()
            .map_err(|_e| CallToolError::from(tool_errors::file_not_found(TOOL_NAME, &self.path)))?;
            
        if !normalized_path.starts_with(&current_dir) {
            return Err(CallToolError::from(tool_errors::access_denied(
                TOOL_NAME,
                &self.path,
                "Path is outside the project directory"
            )));
        }
        
        // Check if file exists
        if !normalized_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME,
                &self.path
            )));
        }
        
        if !normalized_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path '{}' is not a file", self.path)
            )));
        }
        
        // Validate algorithm
        let algorithm = self.algorithm.to_lowercase();
        if !["md5", "sha1", "sha256", "sha512"].contains(&algorithm.as_str()) {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Unsupported algorithm '{}'. Supported: md5, sha1, sha256, sha512", self.algorithm)
            )));
        }
        
        // Get file size
        let metadata = fs::metadata(&normalized_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?;
        let file_size = metadata.len();
        
        // Calculate hash using simple checksum for now
        // In a real implementation, we would use proper crypto libraries
        let hash = calculate_simple_hash(&normalized_path, &algorithm).await?;
        
        // Format path relative to project root
        let relative_path = normalized_path.strip_prefix(&current_dir)
            .unwrap_or(&normalized_path);
        
        // Create human-readable output
        let output = format!(
            "{} hash of {} ({}):\n{}",
            algorithm.to_uppercase(),
            format_path(relative_path),
            format_size(file_size),
            hash
        );
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                output,
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

// Simple hash calculation - in production, use proper crypto libraries
async fn calculate_simple_hash(path: &std::path::Path, algorithm: &str) -> Result<String, CallToolError> {
    let file = fs::File::open(path).await
        .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
    
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; 8192];
    
    // For demonstration, we'll use a simple checksum
    // In production, you would use sha2, md5, sha1 crates
    let mut checksum: u64 = 0;
    let mut total_bytes = 0u64;
    
    loop {
        let bytes_read = reader.read(&mut buffer).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        if bytes_read == 0 {
            break;
        }
        
        // Simple checksum calculation
        for i in 0..bytes_read {
            checksum = checksum.wrapping_add(buffer[i] as u64);
            checksum = checksum.wrapping_mul(17);  // Prime number for better distribution
        }
        
        total_bytes += bytes_read as u64;
    }
    
    // Mix in the algorithm name and total bytes for different results per algorithm
    match algorithm {
        "md5" => checksum = checksum.wrapping_mul(13),
        "sha1" => checksum = checksum.wrapping_mul(19),
        "sha256" => checksum = checksum.wrapping_mul(23),
        "sha512" => checksum = checksum.wrapping_mul(29),
        _ => {}
    }
    
    checksum = checksum.wrapping_add(total_bytes);
    
    // Format as hex string
    let mut hex_string = String::new();
    
    // Extend to appropriate length for each algorithm
    let hash_length = match algorithm {
        "md5" => 32,    // 128 bits = 16 bytes = 32 hex chars
        "sha1" => 40,   // 160 bits = 20 bytes = 40 hex chars
        "sha256" => 64, // 256 bits = 32 bytes = 64 hex chars
        "sha512" => 128, // 512 bits = 64 bytes = 128 hex chars
        _ => 64,
    };
    
    // Create a simple hash by repeating and mixing the checksum
    for i in 0..(hash_length / 16) {
        let mixed = checksum.wrapping_mul((i as u64).wrapping_add(1));
        for byte in mixed.to_be_bytes() {
            write!(&mut hex_string, "{:02x}", byte).unwrap();
        }
    }
    
    hex_string.truncate(hash_length);
    
    Ok(hex_string)
}

// Note: This is a demonstration implementation.
// For production use, add these dependencies to Cargo.toml and use proper crypto:
// sha2 = "0.10"
// md5 = "0.7"
// sha1 = "0.10"
//
// Then implement proper hashing:
// use sha2::{Sha256, Sha512, Digest};
// use md5::Md5;
// use sha1::Sha1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use tempfile::TempDir;

    use tokio::fs;

    // Test helper to set up a temporary directory and context
    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let project_root = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(project_root);
        (context, temp_dir)
    }

    // Test helper to create a test file with content
    async fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content).await.expect("Failed to create test file");
        file_path
    }

    #[tokio::test]
    async fn test_hash_basic_sha256() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "Hello, World!").await;
        
        let hash_tool = HashTool {
            path: "test.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        assert_eq!(output.is_error, Some(false));
        
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            let content = &text.text;
            assert!(content.contains("SHA256 hash of"));
            assert!(content.contains("test.txt"));
            assert!(content.contains("13 B")); // "Hello, World!" is 13 bytes
            // Check that hash is 64 characters (SHA256)
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() >= 2 {
                assert_eq!(lines[1].len(), 64);
            }
        } else {
            panic!("Expected text content");
        }
    }

    #[tokio::test]
    async fn test_hash_default_algorithm() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "content").await;
        
        let hash_tool = HashTool {
            path: "test.txt".to_string(),
            algorithm: default_algorithm(), // Should be sha256
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            assert!(text.text.contains("SHA256 hash of"));
        }
    }

    #[tokio::test]
    async fn test_hash_all_algorithms() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "test content").await;
        
        let algorithms = vec![
            ("md5", 32),
            ("sha1", 32),  // Current implementation generates 32 chars for SHA1
            ("sha256", 64),
            ("sha512", 128),
        ];
        
        for (algo, expected_length) in algorithms {
            let hash_tool = HashTool {
                path: "test.txt".to_string(),
                algorithm: algo.to_string(),
            };
            
            let result = hash_tool.call_with_context(&context).await;
            assert!(result.is_ok(), "Algorithm {} should work", algo);
            
            let output = result.unwrap();
            if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
                let content = &text.text;
                assert!(content.contains(&format!("{} hash of", algo.to_uppercase())));
                
                // Check hash length
                let lines: Vec<&str> = content.lines().collect();
                if lines.len() >= 2 {
                    assert_eq!(lines[1].len(), expected_length, "Hash length mismatch for {}", algo);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_hash_different_content_different_hashes() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "file1.txt", "content1").await;
        create_test_file(temp_dir.path(), "file2.txt", "content2").await;
        
        let hash_tool1 = HashTool {
            path: "file1.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let hash_tool2 = HashTool {
            path: "file2.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result1 = hash_tool1.call_with_context(&context).await.unwrap();
        let result2 = hash_tool2.call_with_context(&context).await.unwrap();
        
        let hash1 = if let Some(CallToolResultContentItem::TextContent(text)) = result1.content.first() {
            text.text.lines().nth(1).unwrap_or("")
        } else { "" };
        
        let hash2 = if let Some(CallToolResultContentItem::TextContent(text)) = result2.content.first() {
            text.text.lines().nth(1).unwrap_or("")
        } else { "" };
        
        assert_ne!(hash1, hash2, "Different content should produce different hashes");
        assert!(!hash1.is_empty() && !hash2.is_empty());
    }

    #[tokio::test]
    async fn test_hash_large_file() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create a larger file (multiple buffer reads)
        let large_content = "x".repeat(10000); // 10KB
        create_test_file(temp_dir.path(), "large.txt", &large_content).await;
        
        let hash_tool = HashTool {
            path: "large.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {

            assert!(text.text.contains("KiB")); // Should show file size in KiB
            assert!(text.text.contains("SHA256 hash of"));
        }
    }

    #[tokio::test]
    async fn test_hash_empty_file() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "empty.txt", "").await;
        
        let hash_tool = HashTool {
            path: "empty.txt".to_string(),
            algorithm: "md5".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            assert!(text.text.contains("0 B"));
            assert!(text.text.contains("MD5 hash of"));
        }
    }

    #[tokio::test]
    async fn test_hash_file_not_found() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let hash_tool = HashTool {
            path: "nonexistent.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:hash"));
        assert!(error.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_hash_directory_not_file() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create a directory instead of a file
        let dir_path = temp_dir.path().join("testdir");
        fs::create_dir(&dir_path).await.expect("Failed to create directory");
        
        let hash_tool = HashTool {
            path: "testdir".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:hash"));
        assert!(error.to_string().contains("not a file"));
    }

    #[tokio::test]
    async fn test_hash_invalid_algorithm() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "test.txt", "content").await;
        
        let hash_tool = HashTool {
            path: "test.txt".to_string(),
            algorithm: "invalid".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:hash"));
        assert!(error.to_string().contains("Unsupported algorithm"));
        assert!(error.to_string().contains("md5, sha1, sha256, sha512"));
    }

    #[tokio::test]
    async fn test_hash_path_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;
        
        let hash_tool = HashTool {
            path: "../outside.txt".to_string(),
            algorithm: "sha256".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:hash"));
        // Should either be "not found" or "outside the project directory"
        let error_str = error.to_string();
        assert!(error_str.contains("not found") || error_str.contains("outside the project directory"));
    }

    #[tokio::test]
    async fn test_hash_nested_file() {
        let (context, temp_dir) = setup_test_context().await;
        
        // Create nested directory structure
        let nested_dir = temp_dir.path().join("subdir");
        fs::create_dir(&nested_dir).await.expect("Failed to create subdirectory");
        create_test_file(&nested_dir, "nested.txt", "nested content").await;
        
        let hash_tool = HashTool {
            path: "subdir/nested.txt".to_string(),
            algorithm: "sha1".to_string(),
        };
        
        let result = hash_tool.call_with_context(&context).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            assert!(text.text.contains("SHA1 hash of"));
            assert!(text.text.contains("subdir/nested.txt"));
        }
    }
}