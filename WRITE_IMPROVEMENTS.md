# Write Operation Improvements for mcp-projectfiles

This document outlines suggested improvements for error messages when write operations are attempted on symlinked directories through projectfiles tools.

## Current State

When attempting write operations on symlinked directories, projectfiles correctly blocks the operation with messages like:
- `projectfiles:write - Access denied: mcp-projectfiles/test.txt (Path is outside the project directory)`

While technically correct, these messages don't guide users to the appropriate solution.

## Proposed Improvements

### 1. Enhanced Error Messages with Clear Guidance

Each write-related tool should provide specific guidance about when to use projectfiles vs standard tools:

#### Write Tool
```
projectfiles:write - Access denied: mcp-projectfiles/test.txt
(Path is outside the project directory. For files within the project, continue using 
mcp__projectfiles__write. For symlinked directories, use the standard Write tool instead.)
```

#### Edit Tool
```
projectfiles:edit - Access denied: mcp-projectfiles/README.md
(Path is outside the project directory. For files within the project, continue using 
mcp__projectfiles__edit. For symlinked directories, use the standard Edit tool instead.)
```

#### Delete Tool
```
projectfiles:delete - Access denied: mcp-projectfiles/file.txt
(Path is outside the project directory. For files within the project, continue using 
mcp__projectfiles__delete. For symlinked directories, use shell commands or other tools.)
```

#### Directory Operations (mkdir, touch)
```
projectfiles:mkdir - Access denied: mcp-projectfiles/new-dir
(Path is outside the project directory. For directories within the project, continue using 
mcp__projectfiles__mkdir. For symlinked directories, use shell commands or other tools.)
```

#### Move/Copy Operations
```
projectfiles:move - Access denied: mcp-projectfiles/file.txt
(Source path is outside the project directory. For files within the project, continue using 
mcp__projectfiles__move. For symlinked directories, use the standard Move tool or shell commands.)
```

### 2. Implementation Strategy

Create a helper function that generates context-aware error messages:

```rust
fn symlink_write_error(tool: &str, path: &str, operation: &str) -> Error {
    let tool_guidance = match tool {
        "write" => "use the standard Write tool",
        "edit" => "use the standard Edit tool", 
        "delete" => "use shell commands or other tools",
        "mkdir" | "touch" => "use shell commands or other tools",
        "move" | "copy" => "use the standard Move tool or shell commands",
        "chmod" => "use shell commands",
        _ => "use appropriate alternative tools"
    };
    
    Error::AccessDenied {
        server: SERVER_NAME.to_string(),
        tool: tool.to_string(),
        path: path.to_string(),
        reason: format!(
            "Path is outside the project directory. For files within the project, continue using \
             mcp__projectfiles__{}. For symlinked directories, {}.",
            tool, tool_guidance
        ),
    }
}
```

### 3. Consistency Improvements

1. **Standardize error phrasing**: Use "Path is outside the project directory" consistently (not "Path would be outside")

2. **Clarify path types**: Be specific about whether it's the source, destination, or general path:
   - For copy/move: "Source path is outside..." or "Destination path is outside..."
   - For others: "Path is outside..."

### 4. Documentation Updates

Add a section to each tool's documentation explaining the security model:

```
IMPORTANT: This tool operates within the project directory for security. To read files 
from symlinked directories, projectfiles tools work perfectly. To modify files in 
symlinked directories, use the standard file manipulation tools (Write, Edit, etc.) 
instead of the mcp__projectfiles__ variants.
```

### 5. Tool-Specific Recommendations

#### High Priority Tools (Most Common Operations)
1. **write** - Clear guidance to use Write tool for symlinks
2. **edit** - Clear guidance to use Edit tool for symlinks
3. **read** - Already works perfectly, no changes needed

#### Medium Priority Tools
1. **copy/move** - Guidance for using standard tools or shell commands
2. **delete** - Guidance for using shell commands
3. **mkdir/touch** - Guidance for using shell commands

#### Low Priority Tools
1. **chmod** - Less commonly used, basic guidance sufficient
2. Other specialized tools - General guidance sufficient

## Benefits of These Improvements

1. **Clear Mental Model**: Users understand when to use projectfiles (project files) vs standard tools (symlinks)
2. **Reduced Friction**: Users aren't left guessing what to do when an operation fails
3. **Security Clarity**: The security boundary is explicit and well-communicated
4. **Consistent Experience**: All tools provide similar, predictable guidance

## Summary

The key principle is: **projectfiles for project, standard tools for symlinks**
- Continue using mcp__projectfiles__ tools for all operations within the project directory
- Use standard tools (Write, Edit, etc.) for modifying files in symlinked directories
- projectfiles can read from symlinks but cannot write to them (by design)

This creates a clear, secure, and user-friendly experience while maintaining the security boundaries that make projectfiles safe to use.