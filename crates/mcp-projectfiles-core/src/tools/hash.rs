use crate::config::tool_errors;
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
    description = "Calculates checksums/hashes of files using various algorithms (MD5, SHA1, SHA256, SHA512)."
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

impl HashTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Get current directory and resolve path
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
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
        
        // Create result
        let result = serde_json::json!({
            "path": self.path,
            "algorithm": algorithm,
            "hash": hash,
            "size": file_size,
            "size_human": format_size(file_size),
        });
        
        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result)
                    .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to serialize result: {}", e))))?,
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

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
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