use crate::config::tool_errors;
use std::path::Path;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;

const TOOL_NAME: &str = "file_type";

#[mcp_tool(
    name = "file_type",
    description = "Detects file type, encoding, and whether it's text or binary. Provides MIME type detection for common file formats."
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FileTypeTool {
    /// Path to the file to analyze (relative to project root)
    pub path: String,
}

impl FileTypeTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        // Get current directory and resolve path
        let current_dir = std::env::current_dir()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get current directory: {}", e))))?;
        
        let target_path = current_dir.join(&self.path);
        
        // Security check - ensure path is within project directory
        let normalized_path = target_path
            .canonicalize()
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to resolve path: {}", e))))?;
            
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
        
        // Get file metadata
        let metadata = fs::metadata(&normalized_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to get file metadata: {}", e))))?;
        
        // Read first chunk of file for analysis
        let mut file = fs::File::open(&normalized_path).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to open file: {}", e))))?;
        
        let mut buffer = vec![0u8; 8192]; // Read up to 8KB for analysis
        let bytes_read = file.read(&mut buffer).await
            .map_err(|e| CallToolError::from(tool_errors::invalid_input(TOOL_NAME, &format!("Failed to read file: {}", e))))?;
        
        buffer.truncate(bytes_read);
        
        // Analyze the file
        let is_text = is_text_file(&buffer);
        let mime_type = detect_mime_type(&normalized_path, &buffer);
        let encoding = if is_text {
            detect_encoding(&buffer)
        } else {
            "binary".to_string()
        };
        
        // Check for BOM
        let has_bom = detect_bom(&buffer).is_some();
        let bom_type = detect_bom(&buffer);
        
        // Create result
        let result = serde_json::json!({
            "path": self.path,
            "is_text": is_text,
            "is_binary": !is_text,
            "encoding": encoding,
            "mime_type": mime_type,
            "size": metadata.len(),
            "size_human": format_size(metadata.len()),
            "has_bom": has_bom,
            "bom_type": bom_type,
            "extension": normalized_path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or(""),
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

fn is_text_file(data: &[u8]) -> bool {
    // Check if file contains null bytes (strong indicator of binary)
    if data.contains(&0) {
        return false;
    }
    
    // Count non-printable characters
    let non_printable_count = data.iter()
        .filter(|&&b| {
            // Allow common text control characters
            !matches!(b, 9..=13 | 32..=126 | 128..=255)
        })
        .count();
    
    // If more than 10% non-printable, likely binary
    let threshold = data.len() / 10;
    non_printable_count <= threshold
}

fn detect_encoding(data: &[u8]) -> String {
    // Check for BOM first
    if let Some(bom) = detect_bom(data) {
        return bom;
    }
    
    // Simple UTF-8 validation
    if std::str::from_utf8(data).is_ok() {
        return "UTF-8".to_string();
    }
    
    // Check for common encodings
    // This is a simplified check - real encoding detection is complex
    let ascii_count = data.iter().filter(|&&b| b < 128).count();
    let ratio = ascii_count as f64 / data.len() as f64;
    
    if ratio > 0.95 {
        "ASCII".to_string()
    } else if ratio > 0.8 {
        "UTF-8 (probable)".to_string()
    } else {
        "Unknown (non-UTF-8)".to_string()
    }
}

fn detect_bom(data: &[u8]) -> Option<String> {
    if data.len() >= 3 && &data[0..3] == &[0xEF, 0xBB, 0xBF] {
        Some("UTF-8 with BOM".to_string())
    } else if data.len() >= 2 && &data[0..2] == &[0xFF, 0xFE] {
        Some("UTF-16 LE".to_string())
    } else if data.len() >= 2 && &data[0..2] == &[0xFE, 0xFF] {
        Some("UTF-16 BE".to_string())
    } else if data.len() >= 4 && &data[0..4] == &[0xFF, 0xFE, 0x00, 0x00] {
        Some("UTF-32 LE".to_string())
    } else if data.len() >= 4 && &data[0..4] == &[0x00, 0x00, 0xFE, 0xFF] {
        Some("UTF-32 BE".to_string())
    } else {
        None
    }
}

fn detect_mime_type(path: &Path, data: &[u8]) -> String {
    // First check by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let mime = match ext.to_lowercase().as_str() {
            // Text files
            "txt" => "text/plain",
            "md" | "markdown" => "text/markdown",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" | "mjs" => "text/javascript",
            "json" => "application/json",
            "xml" => "application/xml",
            "yaml" | "yml" => "text/yaml",
            "toml" => "text/toml",
            
            // Programming languages
            "rs" => "text/rust",
            "py" => "text/x-python",
            "java" => "text/x-java",
            "c" => "text/x-c",
            "cpp" | "cc" | "cxx" => "text/x-c++",
            "h" | "hpp" => "text/x-c-header",
            "go" => "text/x-go",
            "rb" => "text/x-ruby",
            "php" => "text/x-php",
            "swift" => "text/x-swift",
            "kt" => "text/x-kotlin",
            "ts" | "tsx" => "text/typescript",
            "jsx" => "text/jsx",
            "vue" => "text/vue",
            "sh" | "bash" => "text/x-shellscript",
            "ps1" => "text/x-powershell",
            
            // Data files
            "csv" => "text/csv",
            "tsv" => "text/tab-separated-values",
            "sql" => "application/sql",
            
            // Image files
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "svg" => "image/svg+xml",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            
            // Archive files
            "zip" => "application/zip",
            "tar" => "application/x-tar",
            "gz" | "gzip" => "application/gzip",
            "rar" => "application/x-rar-compressed",
            "7z" => "application/x-7z-compressed",
            
            // Document files
            "pdf" => "application/pdf",
            "doc" => "application/msword",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "xls" => "application/vnd.ms-excel",
            "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "ppt" => "application/vnd.ms-powerpoint",
            "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            
            // Binary executables
            "exe" => "application/x-msdownload",
            "dll" => "application/x-msdownload",
            "so" => "application/x-sharedlib",
            "dylib" => "application/x-sharedlib",
            
            _ => {
                // Check by content if extension doesn't match
                return detect_mime_by_content(data);
            }
        };
        return mime.to_string();
    }
    
    // Fallback to content detection
    detect_mime_by_content(data)
}

fn detect_mime_by_content(data: &[u8]) -> String {
    // Check magic bytes for common formats
    if data.len() >= 4 {
        // Images
        if &data[0..2] == b"\xFF\xD8" {
            return "image/jpeg".to_string();
        }
        if &data[0..8] == b"\x89\x50\x4E\x47\x0D\x0A\x1A\x0A" {
            return "image/png".to_string();
        }
        if &data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a" {
            return "image/gif".to_string();
        }
        
        // Archives
        if &data[0..2] == b"PK" {
            return "application/zip".to_string();
        }
        
        // PDF
        if data.len() >= 5 && &data[0..5] == b"%PDF-" {
            return "application/pdf".to_string();
        }
    }
    
    // Default based on whether it's text or binary
    if is_text_file(data) {
        "text/plain".to_string()
    } else {
        "application/octet-stream".to_string()
    }
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