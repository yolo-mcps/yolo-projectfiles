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