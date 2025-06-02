## Tool Enhancement Needs

### CallToolError Definition Search Issue
- Need a projectfiles tool to search through dependency/external library source code
- Currently had to use Task tool (which uses external tools) to find CallToolError definition in rust-mcp-schema crate
- Consider adding tool to search Cargo.lock dependencies or external crate documentation
- Alternative: Add tool to examine compiled dependency metadata/docs

### Using Built-in Tools Instead of Projectfiles Tools
- I used MultiEdit (built-in) instead of projectfiles edit tool during error message fixes
- Need to be more mindful of using projectfiles tools for all file operations
- This defeats the purpose of dogfooding our own tools
- Always check available projectfiles tools before using built-ins

### Error Message Tool Name Issue  
- All tools currently use `CallToolError::unknown_tool()` for all errors
- Should use proper tool-specific error messages instead
- Need to fix all tool error messages to include proper tool names
- PROGRESS: Created centralized config with format_tool_error() function
- PROGRESS: Updated grep.rs partially (but with wrong tools)