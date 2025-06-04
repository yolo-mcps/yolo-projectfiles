# Grep Tool Improvements

## Executive Summary

The current grep tool implementation lacks several key features that make grep/ripgrep effective. This document outlines improvements needed to make our grep tool an adequate replacement.

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

## Feature Gap Analysis: grep/ripgrep vs Our Implementation

### Critical Missing Features

#### 1. **Single File Search Support**
- **Current**: Only searches directories
- **Need**: Accept both file and directory paths
- **Impact**: Major usability issue, forces awkward workarounds

#### 2. **Multiple Pattern Support**
- **Current**: Single pattern only
- **grep/rg**: `-e PATTERN` multiple times, OR logic
- **Impact**: Limits complex searches

#### 3. **Gitignore/Ignore File Respect**
- **Current**: No ignore file support
- **ripgrep**: Respects .gitignore, .ignore, .rgignore by default
- **Impact**: Searches unnecessary files (node_modules, build artifacts)

#### 4. **Binary File Handling**
- **Current**: Basic detection, throws error
- **grep/rg**: Skips binary files gracefully, optional binary search
- **Impact**: Tool fails on mixed codebases

#### 5. **Performance**
- **Current**: Reads entire file into memory
- **ripgrep**: Streaming, memory-mapped files, parallel search
- **Impact**: Poor performance on large files/directories

#### 6. **Output Formats**
- **Current**: Basic line output
- **grep/rg**: Multiple formats (--json, --count, --files-with-matches, --files-without-match)
- **Impact**: Limited integration with other tools

#### 7. **Inverse Matching**
- **Current**: Not supported
- **grep/rg**: -v/--invert-match
- **Impact**: Cannot find lines NOT matching pattern

#### 8. **Word/Line Boundaries**
- **Current**: Regex only
- **grep/rg**: -w (word boundaries), -x (whole line)
- **Impact**: Requires complex regex for simple word searches

#### 9. **Multiple Include/Exclude Patterns**
- **Current**: Single include/exclude pattern
- **grep/rg**: Multiple glob patterns
- **Impact**: Limited file filtering

#### 10. **Type-based Filtering**
- **Current**: None
- **ripgrep**: --type rust, --type-not js
- **Impact**: No semantic file filtering

### Nice-to-Have Features

1. **Color Output**: Highlight matches in results
2. **Statistics**: --stats flag for search statistics
3. **Null Separator**: -0/--null for script integration
4. **Column Numbers**: Show column position of match
5. **Multi-line Mode**: -U/--multiline
6. **Replace Mode**: --replace for preview replacements
7. **Hidden File Control**: --hidden flag
8. **Max Depth Control**: Already have max_depth but not exposed well

## Recommended Implementation Roadmap

### Phase 1: Critical Fixes (High Priority)
1. **Support single file search** - Remove directory-only restriction
2. **Improve binary file handling** - Skip gracefully instead of failing
3. **Add inverse matching** (-v flag equivalent)
4. **Support multiple patterns** (array of patterns with OR logic)

### Phase 2: Performance & Usability (Medium Priority)
1. **Streaming file reader** - Don't load entire file into memory
2. **Gitignore support** - Basic .gitignore parsing
3. **Multiple include/exclude patterns**
4. **Output format options** (--count, --files-with-matches)
5. **Better error messages** with actionable suggestions

### Phase 3: Advanced Features (Low Priority)
1. **Type-based filtering** - Predefined file type groups
2. **Color output** - Highlight matches
3. **Word boundary support** (-w flag)
4. **Statistics output**
5. **Parallel directory traversal**

## Proposed API Changes

### Backward Compatible Additions

```rust
pub struct GrepTool {
    // Existing fields...
    
    /// Multiple patterns (OR logic) - new
    pub patterns: Option<Vec<String>>,
    
    /// Invert match - new
    pub invert_match: bool,
    
    /// Output format - new
    pub output_format: String, // "default", "count", "files-with-matches", "files-without-match"
    
    /// Skip binary files instead of error - new
    pub skip_binary: bool,
    
    /// Respect gitignore files - new
    pub respect_ignore: bool,
    
    /// Multiple include patterns - enhance existing
    pub includes: Option<Vec<String>>,
    
    /// Multiple exclude patterns - enhance existing  
    pub excludes: Option<Vec<String>>,
}
```

### Breaking Changes to Consider

1. Rename `path` to `search_path` for clarity
2. Make `pattern` optional when `patterns` is provided
3. Change include/exclude from Option<String> to Option<Vec<String>>

## Conclusion

To make our grep tool a viable replacement for system grep/ripgrep, we need to:

1. **Immediately**: Fix the single file search limitation
2. **Soon**: Add multiple pattern support, improve binary handling, add inverse matching
3. **Eventually**: Add performance optimizations and advanced features

The current implementation is functional but falls short of user expectations set by grep/ripgrep. Implementing Phase 1 improvements would address the most critical gaps and make the tool significantly more useful.