# TODO

## Query Engine Fixes to Test After Restart

After recompiling and restarting the MCP server, test these fixes:

1. **Mathematical Division Operations** - Fixed in `operations.rs` line 293
   - Test: `.metrics.performance.response_times | add / length`
   - Expected: Should calculate average (around 127.714...)

2. **Length Function in Object Construction** - Fixed in `operations.rs` line 208
   - Test: `.users | group_by(.active) | map({active: .[0].active, count: length})`
   - Expected: Should return counts as numbers, not literal "length"

3. **Map(select()) Filtering** - Fixed in `functions.rs` line 441
   - Test: `.users | map(select(.active))`
   - Expected: Should return only active users without null values

4. **Array Iterator Support** - Already working
   - Test: `.users[].name`
   - Expected: Should return array of names

5. **Array Slicing** - Already implemented
   - Test: `.users[1:3]`
   - Expected: Should return elements at indices 1 and 2

## tomlq Tool Improvements (In Progress)

### High Priority
- [ ] Extract shared query execution logic from jq/yq into a common module to enable code reuse
- [ ] Implement full jq-style query support to match yq/jq capabilities
- [ ] Add comprehensive test coverage for all tomlq operations

### Medium Priority
- [ ] Implement array operations: map(), select(), filter(), sort(), sort_by(), add, min, max, unique, reverse, flatten, group_by(), indices(), array slicing
- [ ] Implement string functions: split(), join(), trim(), contains(), startswith(), endswith(), test(), match(), case conversion, type conversion
- [ ] Implement object operations: keys, values, has(), del(), to_entries, from_entries, with_entries(), paths, leaf_paths
- [ ] Implement math and logic: arithmetic operators, math functions, if-then-else, boolean operators, null handling
- [ ] Add pipe operation support (|) for chaining operations
- [ ] Add recursive descent (..) and wildcard (.*) support
- [ ] Add try-catch error handling

### TOML-Specific Enhancements
- [ ] Better handling of TOML datetime types
- [ ] Preserve comments during in-place edits
- [ ] Support for inline tables and array of tables
- [ ] Additional TOML-specific formatting options

### Documentation
- [x] Update tomlq documentation to accurately reflect current capabilities
- [ ] Add comprehensive examples once new features are implemented
- [ ] Add "When to use" sections for different features

## Touch Tool Improvements (Completed - Requires Restart)

### Major Enhancements to Touch Tool

- **Documentation**: Completely rewrote to match exemplary tools (read, edit, process, kill)
  - Added structured sections: IMPORTANT, NOTE, HINT, Features, Parameters, Examples, Integration
  - Added comprehensive examples in JSON format
  - Added timestamp format documentation with date-only support
  - Improved clarity with specific use cases

- **New Features Implemented**:
  - **content**: Initial content for new files (supports creating files with text)
  - **encoding**: Support for utf-8, ascii, and latin1 encodings
  - **dry_run**: Preview operations without making changes
  - **Date-only timestamps**: Support for "2023-12-25" format (converts to midnight UTC)
  - **Enhanced output**: Shows file size when creating with content
  - **Better error messages**: Clear feedback for encoding issues

- **Improvements**:
  - More detailed output messages including bytes created
  - Better handling of timestamp references
  - Support for different text encodings with validation
  - Dry run mode shows exactly what would be done

- **Testing**:
  - Added tests for content creation
  - Added tests for dry run mode
  - Added tests for date-only timestamp format
  - Added tests for ASCII encoding validation
  - Added tests for non-ASCII content with ASCII encoding (should fail)
  - All 15 touch tool tests pass successfully

### Future Touch Tool Enhancements to Consider:
- Add batch operations to touch multiple files in one call
- Support glob patterns for touching multiple files
- Add chmod-like permissions setting for new files
- Consider adding template support for new file content
- Add option to preserve existing content when updating timestamps
- Support for setting different times for created/accessed/modified
- Integration with stat tool to show before/after timestamps

## Edit Tool Major Improvements (Completed - Requires Restart)

### Critical Bug Fix: replace_all Parameter

- **Bug Fixed**: The `replace_all` parameter was documented but didn't actually exist!
  - Error messages suggested using `replace_all: true` but the parameter wasn't implemented
  - This caused extreme user confusion when trying to do bulk replacements

- **New Implementation**:
  - Added `replace_all` boolean parameter to both EditTool and EditOperation
  - When `replace_all: true`, the tool ignores the `expected` count and replaces all occurrences
  - Added validation to prevent using both `replace_all: true` and `expected` parameter together
  - Better error message showing actual occurrence count as a hint

- **Documentation Updates**:
  - Updated parameter list to include replace_all
  - Added example showing how to use replace_all for bulk replacements
  - Fixed misleading documentation that mentioned a non-existent feature

- **Improved Error Messages**:
  - Now suggests setting `expected: N` to match actual count as an alternative
  - Shows clearer error when parameters conflict
  - Better guidance on how to fix issues

### Testing Required After Restart:
- [ ] Test basic replace_all functionality
- [ ] Test error when using both replace_all and expected
- [ ] Test that old behavior still works for backward compatibility
- [ ] Verify improved error messages are shown

## Tree Tool Improvements (Partially Complete)

### Completed:
- Enhanced documentation to match read/process tool style
- Added comprehensive examples
- Added JSON output format for programmatic use
- Added max_files limit to prevent overwhelming output
- Started adding exclude_pattern support (not finished due to edit tool not being recompiled)

### Still Pending:
- [ ] Complete exclude_pattern implementation after restart
- [ ] Add size threshold filtering (e.g., show only files > 1MB)
- [ ] Add date filtering (similar to find tool)
- [ ] Test with very large directory structures for performance
- [ ] Add more advanced filtering options

## WC Tool Improvements (Completed - Requires Restart)

### Major Enhancements to WC Tool

- **Documentation**: Completely rewrote to match exemplary tools (read, edit, process)
  - Added structured sections: IMPORTANT, NOTE, HINT, Features, Parameters, Examples, Returns
  - Added comprehensive parameter documentation with types and defaults
  - Added extensive examples showing different use cases in JSON format
  - Added clear description of output formats and error conditions

- **New Features Implemented**:
  - **max_line_length**: Reports the length of the longest line (useful for code style checks)
  - **include_metadata**: Includes file metadata (size, modified time, encoding)
  - **output_format**: Supports "text" and "json" output formats for programmatic use
  - **encoding**: Supports multiple encodings (utf-8, ascii, latin1) like read/write tools
  - **Binary file detection**: Gracefully handles binary files with appropriate error messages

- **Improvements**:
  - Enhanced output formatting with aligned columns and human-readable byte sizes
  - JSON output includes structured data with optional fields based on requested counts
  - Better error messages with consistent tool error prefixes
  - Improved test coverage including all new features

- **Testing**:
  - Added tests for max_line_length feature
  - Added tests for JSON output format
  - Added tests for metadata inclusion
  - Added tests for different encodings
  - Added tests for binary file detection
  - Added tests for invalid parameters
  - Refactored tests with helper function for cleaner code

### Future WC Tool Enhancements to Consider:
- Add pattern parameter to count only lines matching a regex (complement to grep)
- Support for counting multiple files in one call with summary statistics
- Add Unicode grapheme cluster counting option (vs just code points)
- Character/word frequency analysis modes
- Progress reporting for very large files
- Streaming mode to handle files larger than memory
- Integration with find tool for batch operations
- Performance optimizations for large file handling

# TODO

## Write Tool Improvements (Completed - Requires Restart)

### Major Enhancements to Write Tool

- **Documentation**: Completely rewrote to match exemplary tools (read, edit, process)
  - Added structured sections: IMPORTANT, NOTE, HINT, Features, Parameters, Examples
  - Added "When to use" guidance for show_diff and dry_run modes
  - Added integration notes explaining how read/write/edit work together
  - Comprehensive examples covering all major use cases

- **New Features**:
  - **dry_run mode**: Preview operations without actually writing files
  - **show_diff mode**: Display colored diffs when overwriting files (using theme system)
  - **follow_symlinks**: Explicit control over symlink behavior (matching read tool)
  - **force flag**: Override file size safety limits (100MB default)
  - **include_metadata**: Return detailed operation metadata in JSON format

- **Improvements**:
  - Better path resolution using same logic as read tool for consistency
  - Enhanced error messages with specific guidance
  - Improved output formatting with operation type (Created/Wrote/Appended/Would write)
  - Added file size safety check with configurable override
  - Colored diff output respecting terminal capabilities and themes
  - Metadata includes timestamps, sizes, encoding info, and backup details

- **Testing**:
  - Added comprehensive tests for all new features
  - Tests for dry_run, show_diff, metadata, and force modes
  - Refactored tests to use helper function for cleaner code

### Harmonious Integration of Read/Write/Edit
- All three tools now use consistent path resolution logic
- Shared symlink handling behavior with follow_symlinks parameter
- Consistent error messaging patterns
- Complementary features: read for viewing, edit for modifications, write for full replacement

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