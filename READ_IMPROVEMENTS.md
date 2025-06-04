# Analysis: Why Task Tool Was Chosen Over Direct File Reading

## Current Behavior Analysis

When asked to explore the archetect codebase for archetype manifest information, I chose to use the `Task` tool instead of directly using `mcp__projectfiles__read` to read files from the archetect symlink.

## Why This Was Suboptimal

### 1. **Direct Access Available**
- The `mcp__projectfiles__read` tool can read files directly from the archetect symlink
- This provides immediate, accurate access to the actual source code
- No need for an intermediary agent when I can read the files myself

### 2. **More Efficient**
- Direct file reading is faster than launching a separate agent
- Reduces context switching and potential information loss
- Allows for precise examination of specific files

### 3. **Better Control**
- I can read exactly the files I need in the order I need them
- Can use line numbers and offsets for precise code examination
- Can follow up with additional reads based on initial findings

## Root Cause Analysis

### Why I Chose Task Tool Instead

1. **Overestimation of Complexity**: I assumed the search required complex pattern matching across many files
2. **Habit from Traditional Workflows**: I default to search-based approaches instead of direct exploration
3. **Unclear File Structure**: I didn't first explore the archetect directory structure to understand what files to read
4. **Tool Selection Priority**: I prioritized "search" over "direct access" when both were available

## Improvement Plan

### 1. **Decision Tree for File Reading**
When exploring codebases:
1. **First**: Use `mcp__projectfiles__list` or `mcp__projectfiles__find` to understand directory structure
2. **Second**: Use `mcp__projectfiles__read` to read specific files directly
3. **Last Resort**: Use `Task` tool only when complex search patterns are needed across many unknown files

### 2. **Specific Improvements for Archetect Exploration**

#### Step 1: Explore Structure
```
mcp__projectfiles__list: archetect/ (get overview)
mcp__projectfiles__find: name_pattern="*.yaml" path="archetect" (find example archetype.yaml files)
mcp__projectfiles__find: name_pattern="*manifest*" path="archetect" (find manifest-related code)
```

#### Step 2: Read Core Files Directly
```
mcp__projectfiles__read: archetect/[specific manifest parsing code]
mcp__projectfiles__read: archetect/[example archetype.yaml files]
mcp__projectfiles__read: archetect/[struct definitions for manifests]
```

#### Step 3: Use Grep for Specific Patterns
```
mcp__projectfiles__grep: pattern="archetype.yaml" path="archetect"
mcp__projectfiles__grep: pattern="manifest" path="archetect"
```

### 3. **General Guidelines**

#### When to Use Direct Reading (`mcp__projectfiles__read`)
- Exploring specific known files
- Reading configuration files, examples, or documentation
- Following imports/references between files
- When file paths are known or discoverable

#### When to Use Search Tools (`mcp__projectfiles__grep`, `mcp__projectfiles__find`)
- Looking for specific patterns across many files
- Finding files by name patterns
- Discovering unknown file locations

#### When to Use Task Tool
- Complex analysis requiring multiple rounds of search and discovery
- When the structure is completely unknown and needs exploration
- When I need an agent to perform comprehensive research across many files

### 4. **Implementation Strategy**

For the current archetype manifest task:
1. **Immediate**: Use `mcp__projectfiles__find` to locate archetype.yaml files
2. **Then**: Use `mcp__projectfiles__read` to examine specific manifest files and parsing code
3. **Finally**: Use `mcp__projectfiles__grep` for specific patterns if needed

### 5. **Verification Approach**

Before choosing tools, ask:
- "Can I read the files directly?" → Use `mcp__projectfiles__read`
- "Do I know what files to look for?" → Use `mcp__projectfiles__find` first
- "Am I looking for patterns across unknown files?" → Use `mcp__projectfiles__grep`
- "Is this a complex research task?" → Use `Task` tool as last resort

## Expected Outcome

By following this plan, I should:
1. Get faster, more accurate results
2. Have better control over the exploration process
3. Reduce unnecessary tool complexity
4. Provide more precise documentation based on actual source code

## Fixed: Symlink Support for Files Within Symlinked Directories

### Bug Identified and Fixed

There was a bug where **files within symlinked directories** could not be read, even though direct symlinked files worked correctly. For example:
- ✅ `symlink.txt` → `/external/file.txt` (worked)
- ❌ `symlinked_dir/file.txt` where `symlinked_dir` → `/external/dir/` (failed)

### Root Cause

The `resolve_path_for_read` function only checked if the exact path was a symlink, but didn't handle the case where a parent directory in the path was a symlink.

### The Fix

Modified `resolve_path_for_read` to check each component of the path for symlinks:

```rust
// Check if any parent directory in the path is a symlink
let mut current_path = PathBuf::new();
for component in absolute_path.components() {
    current_path.push(component);
    if current_path.is_symlink() {
        // Found a symlink in the path, canonicalize the full path
        match absolute_path.canonicalize() {
            Ok(target_path) => {
                // Allow reading through symlinked directories
                return Ok(target_path);
            }
            // ... error handling
        }
    }
}
```

### Test Results

After the fix, reading through the `archetect` symlinked directory works correctly:

```bash
# With follow_symlinks=true - NOW WORKS
mcp__projectfiles__read path="archetect/README.md" follow_symlinks=true
# Successfully reads the file content

# With follow_symlinks=false - CORRECTLY DENIED
mcp__projectfiles__read path="archetect/README.md" follow_symlinks=false
# Error: "Access denied: Path is outside the project directory"
```

### Design Intent

This behavior is intentional and serves as a security feature with an opt-in escape hatch:

1. **Default Security**: Tools are restricted to the project directory for security
2. **Opt-in Access**: Users can explicitly create symlinks within their project to access external content
3. **Controlled by Parameter**: The `follow_symlinks` parameter gives fine-grained control over this behavior

### Use Cases

This design supports common development patterns:
- Referencing shared codebases through symlinks
- Development setups with linked dependencies
- Documentation projects that reference external source code
- Monorepo structures with cross-project references

### Conclusion

No bug fix is needed. The symlink functionality is working as designed and provides the intended balance between security and flexibility.