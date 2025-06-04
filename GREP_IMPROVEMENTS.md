# Grep Tool Improvements

## Problem: "Path is not a directory" Error

The grep tool frequently generates errors when users attempt to search in a specific file rather than a directory. This happens because:

1. **Ambiguous parameter naming**: The `path` parameter suggests it accepts any path, but it only accepts directories.
2. **Common user expectation**: Users often want to grep within a specific file, similar to the Unix `grep` command.
3. **Inconsistent behavior**: Other tools like `read` and `edit` accept file paths, creating confusion.

## Current Behavior

```bash
# This works:
grep --pattern "TODO" --path "src/"

# This fails with "Path is not a directory":
grep --pattern "TODO" --path "src/main.rs"
```

## Proposed Improvements

### 1. Enhanced Tool Description

Update the tool description to be more explicit about the directory requirement:

```rust
#[mcp_tool(name = "grep", description = "Search for patterns in text files within directories. Preferred over system 'grep' or 'rg'.

IMPORTANT: The 'path' parameter must be a DIRECTORY, not a file path.
- To search in a specific directory: path=\"src/\"
- To search specific files: use path=\".\" with include=\"*.rs\" or include=\"main.rs\"
- To search a single file: use path=\"parent/dir\" with include=\"filename.ext\"

Parameters:
- pattern: Regex pattern to search (required)
- path: Directory to search in (MUST be a directory, default: \".\")
- include: File pattern to include (e.g., \"*.rs\", \"main.rs\")
...")]
```

### 2. Rename Parameter for Clarity

Consider renaming `path` to `search_dir` or `directory`:

```rust
/// Directory to search in (relative to project root, default: ".")
#[serde(default = "default_search_dir")]
pub search_dir: String,
```

### 3. Add File Path Support

Enhance the tool to accept both directories and file paths:

```rust
// In call_with_context:
let canonical_path = resolve_path_for_read(&self.path, &project_root, self.follow_search_path, TOOL_NAME)?;

if canonical_path.is_file() {
    // Handle single file search
    return self.search_single_file(canonical_path, &regex).await;
} else if canonical_path.is_dir() {
    // Continue with directory search (existing behavior)
    // ...
}
```

### 4. Improve Error Messages

Provide more helpful error messages with examples:

```rust
if !canonical_search_path.is_dir() {
    return Err(CallToolError::from(tool_errors::invalid_input(
        TOOL_NAME, 
        &format!(
            "Path '{}' is not a directory. The grep tool searches within directories.\n\
            To search in this specific file, use:\n\
            - path=\"{}\" with include=\"{}\"\n\
            Or to search all files in the parent directory:\n\
            - path=\"{}\"",
            self.path,
            canonical_search_path.parent().unwrap_or(&PathBuf::from(".")).display(),
            canonical_search_path.file_name().unwrap_or_default().to_string_lossy(),
            canonical_search_path.parent().unwrap_or(&PathBuf::from(".")).display()
        )
    )));
}
```

### 5. Add Examples to Documentation

Include clear examples in the tool description:

```
Examples:
- Search all files in current dir: {\"pattern\": \"TODO\", \"path\": \".\"}
- Search in src/ directory: {\"pattern\": \"TODO\", \"path\": \"src/\"}
- Search only .rs files: {\"pattern\": \"TODO\", \"path\": \".\", \"include\": \"*.rs\"}
- Search specific file: {\"pattern\": \"TODO\", \"path\": \"src/\", \"include\": \"main.rs\"}
```

## Implementation Priority

1. **High Priority**: Update tool description with clear documentation about directory requirement
2. **Medium Priority**: Improve error messages with helpful suggestions
3. **Low Priority**: Consider adding single file support or parameter renaming (breaking change)

## Benefits

- Reduces user confusion and frustration
- Provides clear guidance on proper usage
- Maintains backward compatibility while improving usability
- Aligns with user expectations from Unix grep command