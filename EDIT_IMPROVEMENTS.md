# Edit Tool Improvements

## Current State

The MCP projectfiles server implements a combined edit tool that supports both single and multi-edit operations through optional parameters:
- Single edit mode: `old_string`, `new_string`, `expected_replacements`
- Multi-edit mode: `edits` array containing `EditOperation` objects

## Improvement Plan

### 1. Enhanced Parameter Validation

Add explicit validation to prevent mixing of single and multi-edit parameters:

```rust
// In edit.rs call_with_context method
if self.edits.is_some() && (self.old_string.is_some() || self.new_string.is_some()) {
    return Err(CallToolError::from(tool_errors::invalid_input(
        TOOL_NAME, 
        "Cannot mix single edit parameters (old_string/new_string) with multi-edit (edits array)"
    )));
}
```

### 2. Improved Documentation

Update the tool description and add usage examples:

```rust
#[mcp_tool(
    name = "edit", 
    description = "Performs exact string replacements in files. Supports two modes: \
                   1) Single edit: use 'old_string' and 'new_string' parameters \
                   2) Multi-edit: use 'edits' array for sequential replacements. \
                   IMPORTANT: File must be read first. Do not mix parameters from different modes."
)]
```

### 3. Add Documentation Comments

Add comprehensive examples in the struct documentation:

```rust
/// Edit tool for performing string replacements in files.
/// 
/// # Single Edit Mode
/// ```json
/// {
///   "file_path": "src/main.rs",
///   "old_string": "println!(\"Hello\")",
///   "new_string": "println!(\"Hello, World\")",
///   "expected_replacements": 1
/// }
/// ```
/// 
/// # Multi-Edit Mode
/// ```json
/// {
///   "file_path": "src/config.rs",
///   "edits": [
///     {
///       "old_string": "const VERSION: &str = \"1.0.0\"",
///       "new_string": "const VERSION: &str = \"1.1.0\"",
///       "expected_replacements": 1
///     },
///     {
///       "old_string": "debug = false",
///       "new_string": "debug = true",
///       "expected_replacements": 2
///     }
///   ]
/// }
/// ```
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditTool {
    // ... existing fields
}
```

### 4. Consider Parameter Grouping

Use serde field attributes to make the modes clearer:

```rust
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditTool {
    /// Path to the file to edit (relative to project root)
    pub file_path: String,
    
    // Single edit mode parameters
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_edit: Option<SingleEdit>,
    
    // Multiple edit mode
    /// Array of edit operations to perform sequentially
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<EditOperation>>,
}

#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct SingleEdit {
    /// The exact string to find and replace
    pub old_string: String,
    /// The string to replace it with
    pub new_string: String,
    /// Expected number of replacements (defaults to 1)
    #[serde(default = "default_expected_replacements")]
    pub expected_replacements: u32,
}
```

### 5. Add Input Validation Helper

Create a helper method to clarify mode detection:

```rust
impl EditTool {
    fn get_edit_mode(&self) -> Result<EditMode, CallToolError> {
        match (self.single_edit.as_ref(), self.edits.as_ref()) {
            (Some(_), Some(_)) => Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME, 
                "Cannot specify both single edit and multi-edit parameters"
            ))),
            (Some(single), None) => Ok(EditMode::Single(single.clone())),
            (None, Some(edits)) if !edits.is_empty() => Ok(EditMode::Multi(edits.clone())),
            _ => Err(CallToolError::from(tool_errors::invalid_input(
                TOOL_NAME, 
                "Must provide either single edit parameters or edits array"
            )))
        }
    }
}

enum EditMode {
    Single(SingleEdit),
    Multi(Vec<EditOperation>),
}
```

## Benefits of These Improvements

1. **Clearer API** - Users immediately understand the two distinct modes
2. **Better error messages** - Explicit validation prevents confusion
3. **Improved discoverability** - Examples in documentation help users
4. **Type safety** - Grouping parameters reduces chance of errors
5. **Maintains backwards compatibility** - Existing usage patterns still work

## Implementation Priority

1. **High Priority**: Add parameter validation to prevent mixing modes
2. **High Priority**: Update tool description with clear mode explanation
3. **Medium Priority**: Add comprehensive documentation with examples
4. **Low Priority**: Consider parameter restructuring (breaking change)

## Testing Requirements

- Add tests for mixed parameter rejection
- Verify both modes work correctly after changes
- Ensure error messages are clear and helpful
- Test backwards compatibility with existing code

## 6. Add Diff Output for File Changes

### Background

Based on research of MCP best practices and the existing codebase patterns:
- Current design philosophy separates modification tools (concise summaries) from the diff tool (detailed changes)
- All existing file modification tools (edit, write, copy, move) return simple success messages
- This allows users to control when they want to see detailed diffs

### Implementation Options

#### Option 1: Add Optional Diff Parameter (Recommended)
Add an optional `show_diff` parameter to the edit tool:

```rust
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct EditTool {
    /// Path to the file to edit (relative to project root)
    pub file_path: String,
    
    /// Show a diff of the changes made (default: false)
    #[serde(default)]
    pub show_diff: bool,
    
    // ... existing parameters
}
```

Implementation approach:
1. Capture the original content before edits
2. If `show_diff` is true, generate a unified diff using the existing `similar` crate
3. Include the diff in the response along with the summary

Example response with diff:
```
Successfully replaced 2 occurrences in src/main.rs

--- src/main.rs
+++ src/main.rs
@@ -10,7 +10,7 @@
 fn main() {
-    println!("Hello");
+    println!("Hello, World");
     let config = Config {
-        debug: false,
+        debug: true,
     };
 }
```

#### Option 2: Return Structured Change Information
Instead of a text diff, return structured data about what changed:

```rust
#[derive(Serialize)]
struct EditResult {
    summary: String,
    changes: Vec<ChangeDetail>,
}

#[derive(Serialize)]
struct ChangeDetail {
    line_number: usize,
    old_text: String,
    new_text: String,
    context: String,  // Surrounding lines for context
}
```

#### Option 3: Use Tool Metadata
Utilize the unused `meta` field in `CallToolResult` to include diff information:

```rust
Ok(CallToolResult {
    content: vec![CallToolResultContentItem::TextContent(TextContent::new(
        summary_message, None,
    ))],
    is_error: Some(false),
    meta: if self.show_diff { 
        Some(serde_json::json!({ "diff": diff_text })) 
    } else { 
        None 
    },
})
```

### Recommendation

**Implement Option 1** with these considerations:

1. **Default behavior unchanged** - Maintains backwards compatibility and current concise output
2. **Opt-in verbosity** - Users explicitly request diffs when needed
3. **Reuse existing diff logic** - Leverage the `similar` crate already used in diff.rs
4. **Consistent with MCP principles** - Tools should be model-controlled with clear parameters
5. **Limit diff size** - For large files, show only relevant context around changes

### Implementation Details

1. **Add the parameter**:
   ```rust
   /// Show a diff of the changes made (default: false)
   #[serde(default)]
   pub show_diff: bool,
   ```

2. **Capture original content**:
   ```rust
   let original_content = content.clone();
   ```

3. **Generate diff if requested**:
   ```rust
   let message = if self.show_diff {
       let diff = TextDiff::from_lines(&original_content, &content);
       let mut diff_output = String::new();
       
       // Add summary
       diff_output.push_str(&format!(
           "Successfully replaced {} occurrence{} in {}\n\n",
           total_replacements,
           if total_replacements == 1 { "" } else { "s" },
           self.file_path
       ));
       
       // Add unified diff
       diff_output.push_str(&format!("--- {}\n", self.file_path));
       diff_output.push_str(&format!("+++ {}\n", self.file_path));
       
       for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
           if idx > 0 {
               diff_output.push_str("...\n");
           }
           for op in group {
               for change in diff.iter_inline_changes(op) {
                   // Format diff lines
               }
           }
       }
       
       diff_output
   } else {
       // Current summary message
   };
   ```

4. **Handle large diffs**:
   - Limit to changes plus 3 lines of context
   - Truncate if total diff exceeds reasonable size (e.g., 100 lines)
   - Add message if truncated: "... (diff truncated, showing first N changes)"

### Benefits

1. **User control** - Developers can choose when to see diffs
2. **Performance** - No overhead when diffs aren't needed
3. **Clarity** - Diffs help verify complex multi-edit operations
4. **Debugging** - Easier to troubleshoot unexpected replacements
5. **Consistency** - Aligns with MCP principle of model-controlled tools

### Testing Requirements

- Test diff generation with single and multi-edit modes
- Verify large file handling and truncation
- Ensure diff format matches standard unified diff
- Test performance impact of diff generation
- Verify backwards compatibility (show_diff defaults to false)