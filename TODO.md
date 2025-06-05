# TODO

## Recent Improvements (Requires Restart)

### Stat Tool Enhancements

- **Documentation**: Completely rewrote to match exemplary tools (edit, process, lsof, kill)
  - Added structured sections: IMPORTANT, NOTE, Parameters, Features, Examples, Returns, Platform notes
  - Added comprehensive JSON examples showing different use cases
  - Added detailed field descriptions for all return values
  - Improved clarity around symlink behavior and platform differences

- **New Features**:
  - **Symlink target resolution**: Added `symlink_target` field when stat'ing a symlink
  - Shows the path the symlink points to (useful for debugging symlink issues)

- **Test Improvements**:
  - Added test for special filenames (spaces, dots, dashes, underscores)
  - Enhanced symlink test to verify symlink_target field is returned
  - Improved test coverage for edge cases

- **Requires restart to see the improved documentation and symlink_target feature**

#### Future Stat Tool Enhancements to Consider:
- Add executable detection for files (check execute permissions)
- Add file type detection for sockets, FIFOs, block/character devices
- Consider batch operations to stat multiple files in one call
- Add extended attributes (xattr) support for systems that have them
- Consider adding optional hash calculation (MD5/SHA) integration
- Add comparison mode to compare metadata between two files
- Improve Windows support with Windows-specific metadata
- Add support for showing ACLs on systems that support them

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

### Process Tool Enhancements

- Simplified and improved the process listing tool
- Now returns structured JSON with process and port information
- Better cross-platform support with uniform output format
- Improved sorting and filtering capabilities
- Added integration with kill tool workflow
- Optimized for use in coding assistant context

### Read Tool Major Enhancements

- **Documentation**: Reorganized to match exemplary tools (edit, process, lsof, kill)
  - Added comprehensive examples section with JSON examples
  - Added detailed parameter descriptions with defaults
  - Added Returns section documenting output format
  - Improved organization with IMPORTANT/NOTE/HINT sections

- **New Features**:
  - **Line range syntax**: Support for "10-20" syntax in addition to offset/limit
  - **Preview mode**: Show file metadata without reading content (`preview_only`)
  - **Context lines**: Show lines before/after pattern matches (`context_before`, `context_after`)
  - **Inverse matching**: Show lines NOT matching pattern (`invert_match`)
  - **BOM detection**: Detect and report Byte Order Marks
  - **File metadata**: Optional inclusion of size, modified time, line count (`include_metadata`)

- **Improvements**:
  - Better error messages with consistent prefixes
  - Enhanced pattern matching with context lines
  - Improved truncation messages with file size info
  - More comprehensive test coverage including integration tests

### Process Tool Enhancements

- Enhanced documentation with comprehensive mcp_tool description
- Added sort_by parameter supporting: name, pid, cpu, memory
- Added user and start_time fields to process information (Unix only)
- Improved integration points documentation for kill and lsof tools
- Better structured JSON examples and return value documentation
- Added comprehensive integration tests including process/kill/lsof integration
- **Requires restart to use the enhanced tool**

#### Future Process Tool Enhancements to Consider:
- Add parent_pid field to show process hierarchy
- Add filter_by parameter to filter by user, status, or resource usage thresholds
- Add output_format parameter (json, compact, detailed) similar to lsof tool
- Consider adding working_directory field for processes
- Add support for showing child processes when parent is found
- Consider adding CPU/memory usage history if available from system
- Add network connections per process (complement to port checking)

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
