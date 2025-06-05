use crate::config::tool_errors;
use crate::context::{StatefulTool, ToolContext};
use crate::tools::utils::resolve_path_for_read;
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

const TOOL_NAME: &str = "file";

fn default_follow_symlinks() -> bool {
    true
}

#[mcp_tool(
    name = "file",
    description = "Analyze file type, encoding, and content. Binary detection, MIME types, language identification.
Examples: {\"path\": \"README.md\"} or {\"path\": \"logo.png\"}"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct FileTool {
    /// File to analyze (relative to project root)
    pub path: String,
    /// Follow symlinks to analyze files outside the project directory (optional, default: true)
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
}

#[async_trait]
impl StatefulTool for FileTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Get project root and resolve path
        let project_root = context.get_project_root().map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get project root: {}", e),
            ))
        })?;

        // Use the utility function to resolve path with symlink support
        let normalized_path =
            resolve_path_for_read(&self.path, &project_root, self.follow_symlinks, TOOL_NAME)?;

        // Check if file exists
        if !normalized_path.exists() {
            return Err(CallToolError::from(tool_errors::file_not_found(
                TOOL_NAME, &self.path,
            )));
        }

        if !normalized_path.is_file() {
            return Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Path '{}' is not a file", self.path),
            )));
        }

        // Get file metadata
        let metadata = fs::metadata(&normalized_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to get file metadata: {}", e),
            ))
        })?;

        // Read first chunk of file for analysis
        let mut file = fs::File::open(&normalized_path).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to open file: {}", e),
            ))
        })?;

        let mut buffer = vec![0u8; 8192]; // Read up to 8KB for analysis
        let bytes_read = file.read(&mut buffer).await.map_err(|e| {
            CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME,
                &format!("Failed to read file: {}", e),
            ))
        })?;

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

        // Additional analysis for text files
        let (line_ending, shebang, language, preview_lines) = if is_text {
            let text_str = String::from_utf8_lossy(&buffer);
            let line_ending = detect_line_ending(&buffer);
            let shebang = detect_shebang(&text_str);
            let language = detect_programming_language(&normalized_path, &text_str);
            let preview_lines = get_preview_lines(&text_str, 5);
            (Some(line_ending), shebang, language, Some(preview_lines))
        } else {
            (None, None, None, None)
        };

        // Create result
        let mut result = serde_json::json!({
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

        // Add text-specific fields if applicable
        if is_text {
            result["line_ending"] = serde_json::json!(line_ending);
            result["shebang"] = serde_json::json!(shebang);
            result["language"] = serde_json::json!(language);
            result["preview_lines"] = serde_json::json!(preview_lines);
        }

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                serde_json::to_string_pretty(&result).map_err(|e| {
                    CallToolError::from(tool_errors::invalid_input(
                        TOOL_NAME,
                        &format!("Failed to serialize result: {}", e),
                    ))
                })?,
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
    let non_printable_count = data
        .iter()
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
        if data.len() >= 8 && &data[0..8] == b"\x89\x50\x4E\x47\x0D\x0A\x1A\x0A" {
            return "image/png".to_string();
        }
        if data.len() >= 6 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
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

fn detect_line_ending(data: &[u8]) -> String {
    let mut crlf_count = 0;
    let mut lf_count = 0;

    let mut i = 0;
    while i < data.len() {
        if i + 1 < data.len() && data[i] == b'\r' && data[i + 1] == b'\n' {
            crlf_count += 1;
            i += 2; // Skip both \r and \n
        } else if data[i] == b'\n' {
            lf_count += 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    if crlf_count > lf_count {
        "CRLF".to_string()
    } else if lf_count > 0 {
        "LF".to_string()
    } else if crlf_count > 0 {
        "CRLF".to_string()
    } else {
        "unknown".to_string()
    }
}

fn detect_shebang(text: &str) -> Option<String> {
    if text.starts_with("#!") {
        text.lines().next().map(|s| s.to_string())
    } else {
        None
    }
}

fn detect_programming_language(path: &Path, content: &str) -> Option<String> {
    // First check by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lang = match ext.to_lowercase().as_str() {
            "rs" => "Rust",
            "py" => "Python",
            "js" | "mjs" => "JavaScript",
            "ts" | "tsx" => "TypeScript",
            "jsx" => "JavaScript (JSX)",
            "java" => "Java",
            "c" => "C",
            "cpp" | "cc" | "cxx" => "C++",
            "h" | "hpp" => "C/C++ Header",
            "go" => "Go",
            "rb" => "Ruby",
            "php" => "PHP",
            "swift" => "Swift",
            "kt" => "Kotlin",
            "cs" => "C#",
            "sh" | "bash" => "Shell",
            "ps1" => "PowerShell",
            "r" => "R",
            "m" => "MATLAB/Objective-C",
            "lua" => "Lua",
            "pl" => "Perl",
            "scala" => "Scala",
            "clj" => "Clojure",
            "ex" | "exs" => "Elixir",
            "erl" => "Erlang",
            "hs" => "Haskell",
            "ml" => "OCaml",
            "vue" => "Vue",
            "dart" => "Dart",
            "nim" => "Nim",
            "zig" => "Zig",
            _ => return None,
        };
        return Some(lang.to_string());
    }

    // Check shebang for scripts without extension
    if let Some(shebang) = detect_shebang(content) {
        if shebang.contains("python") {
            return Some("Python".to_string());
        } else if shebang.contains("bash") || shebang.contains("/sh") {
            return Some("Shell".to_string());
        } else if shebang.contains("node") {
            return Some("JavaScript".to_string());
        } else if shebang.contains("ruby") {
            return Some("Ruby".to_string());
        } else if shebang.contains("perl") {
            return Some("Perl".to_string());
        }
    }

    None
}

fn get_preview_lines(text: &str, max_lines: usize) -> Vec<String> {
    text.lines()
        .take(max_lines)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ToolContext;
    use serde_json::Value;
    use tempfile::TempDir;
    use tokio::fs;

    async fn setup_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::with_project_root(canonical_path);
        (context, temp_dir)
    }

    async fn create_test_file(
        dir: &std::path::Path,
        name: &str,
        content: &[u8],
    ) -> std::path::PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content)
            .await
            .expect("Failed to create test file");
        file_path
    }

    async fn parse_output(output: &CallToolResult) -> Value {
        if let Some(CallToolResultContentItem::TextContent(text)) = output.content.first() {
            serde_json::from_str(&text.text).expect("Failed to parse JSON output")
        } else {
            panic!("Expected text content");
        }
    }

    fn create_file_tool(path: &str) -> FileTool {
        FileTool {
            path: path.to_string(),
            follow_symlinks: true,
        }
    }

    #[tokio::test]
    async fn test_file_text_file() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "Hello, World!\nThis is a text file.";
        create_test_file(temp_dir.path(), "text.txt", content.as_bytes()).await;

        let file_tool = create_file_tool("text.txt");

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.is_error, Some(false));

        let json = parse_output(&output).await;
        assert_eq!(json["path"], "text.txt");
        assert_eq!(json["is_text"], true);
        assert_eq!(json["is_binary"], false);
        assert!(
            json["encoding"].as_str().unwrap().contains("UTF-8")
                || json["encoding"].as_str().unwrap().contains("ASCII")
        );
        assert_eq!(json["mime_type"], "text/plain");
        assert_eq!(json["extension"], "txt");
        assert_eq!(json["has_bom"], false);
        assert_eq!(json["bom_type"], Value::Null);
    }

    #[tokio::test]
    async fn test_file_binary_file() {
        let (context, temp_dir) = setup_test_context().await;
        let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD];
        create_test_file(temp_dir.path(), "binary.bin", &binary_content).await;

        let file_tool = create_file_tool("binary.bin");

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], false);
        assert_eq!(json["is_binary"], true);
        assert_eq!(json["encoding"], "binary");
        assert_eq!(json["extension"], "bin");
    }

    #[tokio::test]
    async fn test_file_rust_source() {
        let (context, temp_dir) = setup_test_context().await;
        let rust_content = "fn main() {\n    println!(\"Hello, World!\");\n}";
        create_test_file(temp_dir.path(), "main.rs", rust_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "main.rs".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "text/rust");
        assert_eq!(json["extension"], "rs");
    }

    #[tokio::test]
    async fn test_file_javascript() {
        let (context, temp_dir) = setup_test_context().await;
        let js_content = "console.log('Hello, World!');";
        create_test_file(temp_dir.path(), "script.js", js_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "script.js".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "text/javascript");
        assert_eq!(json["extension"], "js");
    }

    #[tokio::test]
    async fn test_file_json() {
        let (context, temp_dir) = setup_test_context().await;
        let json_content = r#"{"name": "test", "value": 42}"#;
        create_test_file(temp_dir.path(), "data.json", json_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "data.json".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "application/json");
        assert_eq!(json["extension"], "json");
    }

    #[tokio::test]
    async fn test_file_utf8_bom() {
        let (context, temp_dir) = setup_test_context().await;
        let mut content = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        content.extend_from_slice("Hello UTF-8 with BOM".as_bytes());
        create_test_file(temp_dir.path(), "utf8_bom.txt", &content).await;

        let file_tool = FileTool {
            path: "utf8_bom.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], true);
        assert_eq!(json["encoding"], "UTF-8 with BOM");
        assert_eq!(json["has_bom"], true);
        assert_eq!(json["bom_type"], "UTF-8 with BOM");
    }

    #[tokio::test]
    async fn test_file_utf16_bom() {
        let (context, temp_dir) = setup_test_context().await;
        let mut content = vec![0xFF, 0xFE]; // UTF-16 LE BOM
        content.extend_from_slice("H\0e\0l\0l\0o\0".as_bytes());
        create_test_file(temp_dir.path(), "utf16.txt", &content).await;

        let file_tool = FileTool {
            path: "utf16.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["has_bom"], true);
        assert_eq!(json["bom_type"], "UTF-16 LE");
    }

    #[tokio::test]
    async fn test_file_empty_file() {
        let (context, temp_dir) = setup_test_context().await;
        create_test_file(temp_dir.path(), "empty.txt", &[]).await;

        let file_tool = FileTool {
            path: "empty.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["is_text"], true); // Empty file is considered text
        assert_eq!(json["size"], 0);
        assert_eq!(json["size_human"], "0 B");
    }

    #[tokio::test]
    async fn test_file_pdf_magic_bytes() {
        let (context, temp_dir) = setup_test_context().await;
        let mut pdf_content = b"%PDF-1.4".to_vec();
        pdf_content.extend_from_slice(&[0x00, 0x01, 0x02]); // Add some binary data
        create_test_file(temp_dir.path(), "document.pdf", &pdf_content).await;

        let file_tool = FileTool {
            path: "document.pdf".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["mime_type"], "application/pdf");
        assert_eq!(json["extension"], "pdf");
    }

    #[tokio::test]
    async fn test_file_png_magic_bytes() {
        let (context, temp_dir) = setup_test_context().await;
        let mut png_signature = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        png_signature.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]); // Add some more bytes
        create_test_file(temp_dir.path(), "image.png", &png_signature).await;

        let file_tool = FileTool {
            path: "image.png".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["mime_type"], "image/png");
        assert_eq!(json["is_binary"], true);
    }

    #[tokio::test]
    async fn test_file_jpeg_magic_bytes() {
        let (context, temp_dir) = setup_test_context().await;
        let jpeg_signature = vec![0xFF, 0xD8, 0xFF, 0xE0];
        create_test_file(temp_dir.path(), "photo.jpg", &jpeg_signature).await;

        let file_tool = FileTool {
            path: "photo.jpg".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["mime_type"], "image/jpeg");
        assert_eq!(json["extension"], "jpg");
    }

    #[tokio::test]
    async fn test_file_zip_magic_bytes() {
        let (context, temp_dir) = setup_test_context().await;
        let zip_signature = vec![0x50, 0x4B, 0x03, 0x04];
        create_test_file(temp_dir.path(), "archive.zip", &zip_signature).await;

        let file_tool = FileTool {
            path: "archive.zip".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["mime_type"], "application/zip");
    }

    #[tokio::test]
    async fn test_file_no_extension() {
        let (context, temp_dir) = setup_test_context().await;
        let content = "#!/bin/bash\necho 'Hello World'";
        create_test_file(temp_dir.path(), "script", content.as_bytes()).await;

        let file_tool = FileTool {
            path: "script".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["extension"], "");
        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "text/plain"); // Fallback for unknown extension
    }

    #[tokio::test]
    async fn test_file_file_not_found() {
        let (context, _temp_dir) = setup_test_context().await;

        let file_tool = FileTool {
            path: "nonexistent.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();

        assert!(error_str.contains("projectfiles:file"));
    }

    #[tokio::test]
    async fn test_file_directory_not_file() {
        let (context, temp_dir) = setup_test_context().await;

        let dir_path = temp_dir.path().join("testdir");
        fs::create_dir(&dir_path)
            .await
            .expect("Failed to create directory");

        let file_tool = FileTool {
            path: "testdir".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("projectfiles:file"));
        assert!(error.to_string().contains("not a file"));
    }

    #[tokio::test]
    async fn test_file_path_outside_project() {
        let (context, _temp_dir) = setup_test_context().await;

        let file_tool = FileTool {
            path: "../outside.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();

        assert!(error_str.contains("projectfiles:file"));
    }

    #[tokio::test]
    async fn test_file_large_file_size_formatting() {
        let (context, temp_dir) = setup_test_context().await;
        let large_content = vec![b'x'; 2048]; // 2KB
        create_test_file(temp_dir.path(), "large.txt", &large_content).await;

        let file_tool = FileTool {
            path: "large.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["size"], 2048);
        assert_eq!(json["size_human"], "2.00 KB");
    }

    #[tokio::test]
    async fn test_file_nested_path() {
        let (context, temp_dir) = setup_test_context().await;

        let nested_dir = temp_dir.path().join("subdir");
        fs::create_dir(&nested_dir)
            .await
            .expect("Failed to create subdirectory");
        let content = "Nested file content";
        create_test_file(&nested_dir, "nested.txt", content.as_bytes()).await;

        let file_tool = FileTool {
            path: "subdir/nested.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["path"], "subdir/nested.txt");
        assert_eq!(json["is_text"], true);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_symlink_to_file_within_project() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file
        let target_content = "Target file content for symlink test";
        create_test_file(temp_dir.path(), "target.txt", target_content.as_bytes()).await;

        // Create a symlink to the target file
        let target_path = temp_dir.path().join("target.txt");
        let symlink_path = temp_dir.path().join("link_to_target.txt");
        std::os::unix::fs::symlink(&target_path, &symlink_path).expect("Failed to create symlink");

        let file_tool = FileTool {
            path: "link_to_target.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        // Should analyze the target file, not the symlink itself
        assert_eq!(json["path"], "link_to_target.txt");
        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "text/plain");
        assert_eq!(json["extension"], "txt");
        assert!(json["size"].as_u64().unwrap() > 0); // Should have the size of the target file
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_symlink_to_file_outside_project() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file outside the project directory
        let external_temp_dir = TempDir::new().unwrap();
        let external_target = external_temp_dir.path().join("external_target.txt");
        fs::write(&external_target, "External file content")
            .await
            .expect("Failed to create external file");

        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path)
            .expect("Failed to create symlink");

        let file_tool = FileTool {
            path: "link_to_external.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        // Should analyze the external target file
        assert_eq!(json["path"], "link_to_external.txt");
        assert_eq!(json["is_text"], true);
        assert_eq!(json["mime_type"], "text/plain");
        assert_eq!(json["extension"], "txt");
        assert!(json["size"].as_u64().unwrap() > 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_symlink_with_follow_disabled() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a target file outside the project to ensure symlink behavior is tested
        let external_temp_dir = TempDir::new().unwrap();
        let external_target = external_temp_dir.path().join("external_target.txt");
        fs::write(&external_target, "External target content")
            .await
            .expect("Failed to create external file");

        // Create a symlink within the project to the external file
        let symlink_path = temp_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&external_target, &symlink_path)
            .expect("Failed to create symlink");

        let file_tool = FileTool {
            path: "link_to_external.txt".to_string(),
            follow_symlinks: false,
        };

        let result = file_tool.call_with_context(&context).await;
        // With follow_symlinks=false, the symlink should not be resolved,
        // so it should fail when the canonicalized path is outside the project
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:file"));
        assert!(error_str.contains("Cannot access symlink"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_broken_symlink() {
        let (context, temp_dir) = setup_test_context().await;

        // Create a symlink to a non-existent target
        let target_path = temp_dir.path().join("nonexistent_target.txt");
        let symlink_path = temp_dir.path().join("broken_link.txt");
        std::os::unix::fs::symlink(&target_path, &symlink_path).expect("Failed to create symlink");

        let file_tool = FileTool {
            path: "broken_link.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(error_str.contains("projectfiles:file"));
        // Should indicate file not found since the symlink target doesn't exist
        assert!(error_str.contains("not found") || error_str.contains("No such file"));
    }

    #[tokio::test]
    async fn test_file_line_ending_detection() {
        let (context, temp_dir) = setup_test_context().await;

        // Test LF (Unix)
        let lf_content = "Line 1\nLine 2\nLine 3";
        create_test_file(temp_dir.path(), "unix.txt", lf_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "unix.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;
        assert_eq!(json["line_ending"], "LF");

        // Test CRLF (Windows)
        let crlf_content = "Line 1\r\nLine 2\r\nLine 3";
        create_test_file(temp_dir.path(), "windows.txt", crlf_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "windows.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;
        assert_eq!(json["line_ending"], "CRLF");
    }

    #[tokio::test]
    async fn test_file_shebang_detection() {
        let (context, temp_dir) = setup_test_context().await;

        let python_script = "#!/usr/bin/env python3\nprint('Hello, World!')";
        create_test_file(temp_dir.path(), "script.py", python_script.as_bytes()).await;

        let file_tool = FileTool {
            path: "script.py".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;
        assert_eq!(json["shebang"], "#!/usr/bin/env python3");
        assert_eq!(json["language"], "Python");
    }

    #[tokio::test]
    async fn test_file_language_detection() {
        let (context, temp_dir) = setup_test_context().await;

        // Test various programming languages
        let rust_code = "fn main() {\n    println!(\"Hello\");\n}";
        create_test_file(temp_dir.path(), "main.rs", rust_code.as_bytes()).await;

        let file_tool = FileTool {
            path: "main.rs".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;
        assert_eq!(json["language"], "Rust");

        // Test TypeScript
        let ts_code = "const greeting: string = 'Hello';";
        create_test_file(temp_dir.path(), "app.ts", ts_code.as_bytes()).await;

        let file_tool = FileTool {
            path: "app.ts".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;
        assert_eq!(json["language"], "TypeScript");
    }

    #[tokio::test]
    async fn test_file_preview_lines() {
        let (context, temp_dir) = setup_test_context().await;

        let multi_line_content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7";
        create_test_file(temp_dir.path(), "multi.txt", multi_line_content.as_bytes()).await;

        let file_tool = FileTool {
            path: "multi.txt".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        let preview = json["preview_lines"].as_array().unwrap();
        assert_eq!(preview.len(), 5); // Should only show first 5 lines
        assert_eq!(preview[0], "Line 1");
        assert_eq!(preview[4], "Line 5");
    }

    #[tokio::test]
    async fn test_file_no_extension_with_shebang() {
        let (context, temp_dir) = setup_test_context().await;

        let bash_script = "#!/bin/bash\necho 'Hello from bash'";
        create_test_file(temp_dir.path(), "run", bash_script.as_bytes()).await;

        let file_tool = FileTool {
            path: "run".to_string(),
            follow_symlinks: true,
        };

        let result = file_tool.call_with_context(&context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let json = parse_output(&output).await;

        assert_eq!(json["shebang"], "#!/bin/bash");
        assert_eq!(json["language"], "Shell");
        assert_eq!(json["extension"], "");
    }
}

