# TODO

## Recent Improvements (Requires Restart)

### Edit Tool Enhancements

- Added detailed error messages when multiple occurrences are found
  - Shows line numbers and context for up to 5 occurrences
  - Provides hint to use `replace_all: true` or make search more specific
- Added helpful hints when string is not found
  - Detects partial matches at beginning/end of search string
  - Suggests checking for whitespace/special character issues
- Improved error handling to guide users toward successful edits

### Kill Tool Enhancements

- Added dry-run capability matching edit tool pattern
- Improved API documentation with better examples
- Added detailed process information display in dry-run mode
- Shows command line and working directory for each process
- Better safety with clearer confirmation requirements
- **Modified default behavior**: Now kills processes by default (no confirmation required)
- Marked confirm/force parameters as DEPRECATED but kept for compatibility
- Only requires dry_run=true when user wants to be careful

### Lsof Tool Implementation

- Added new `lsof` tool for listing open files within project directory
- Simplified implementation focused on practical use for coding LLMs
- Features:
  - Lists open files with process information
  - Supports file pattern filtering (e.g., `*.log`, `*.db`)
  - Project directory restriction for security
  - Cross-platform support (full on Unix, limited on Windows)
- Includes integration tests for basic functionality
- **Requires restart to use the new tool**

### Move Tool Enhancements

- Enhanced documentation with detailed mcp_tool description matching edit/find/file tools
- Added dry_run parameter for safe preview of moves
- Improved error handling with detailed context about failures
- Added intelligent handling when moving files to directories
  - Automatically appends filename when destination is existing directory
  - Provides helpful error messages for conflicts
- Added comprehensive tests for all new features
- **Requires restart to use the enhanced tool**

## High Priority

### Documentation Updates Needed

- [ ] Update tool documentation to clarify symlink behavior for directories
- [ ] Add examples showing how to use symlinks as escape hatches for external content

## Medium Priority

### Code Improvements

- [ ] Fix exists tool to properly report symlink type when follow_symlinks=false (currently reports target type)

### Testing Improvements

- [ ] Add more comprehensive symlink tests for edge cases (nested symlinks, circular symlinks)
- [ ] Profile the symlink path checking to ensure it doesn't impact performance for deep paths

## Low Priority

### Future Enhancements

- [ ] Consider adding a configuration option to disable symlink following globally
- [ ] Add metrics/logging for symlink usage patterns
- [ ] Consider consolidating the read tool's parameters

## Post-Restart Testing

### Move Tool Testing
- [ ] Test basic move operations after restart
- [ ] Test dry_run functionality with various scenarios
- [ ] Test moving files to existing directories
- [ ] Test error messages for permission denied and cross-device moves
- [ ] Verify the enhanced documentation is properly displayed
