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

## Recently Completed
- [x] Fixed symlink bug in `resolve_path_for_read` - now properly handles files within symlinked directories
- [x] Added test case `test_read_file_in_symlinked_directory` to ensure the fix works
- [x] Created dedicated `SymlinkAccessDenied` error type for clearer error messages
- [x] Updated all tool descriptions to document symlink behavior and `follow_symlinks` parameter
- [x] Added `resolve_path_allowing_symlinks` function to allow exists/stat tools to check symlink metadata
- [x] Fixed all test failures related to symlink error handling
- [x] Tested all tools with symlinks for security and proper behavior
- [x] Fixed kill tool bug where lsof wasn't properly filtering by PID (missing `-a` flag)
- [x] Fixed kill tool path comparison to canonicalize paths before checking if process is in project directory
- [x] Added integration test for kill tool process detection and killing

## CRITICAL SECURITY BUGS

### Symlink Write Vulnerabilities
- [ ] **CRITICAL**: The `copy` tool allows copying files INTO symlinked directories (destination path validation missing)
- [ ] **CRITICAL**: The `tomlq`, `yq`, and `jq` tools in write mode allow modifying files in symlinked directories
- [ ] Add comprehensive symlink security tests to integration tests for write operations



## High Priority

### Testing & Verification
- [ ] Run full test suite to ensure symlink fix doesn't break other functionality
- [ ] Test symlink fix with other tools that use `resolve_path_for_read` (grep, stat, list, etc.)
- [ ] Verify the fix works on Windows with directory symlinks

### Documentation
- [ ] Update tool documentation to clarify symlink behavior for directories
- [ ] Add examples showing how to use symlinks as escape hatches for external content

## Medium Priority

### Code Improvements
- [ ] Add more comprehensive symlink tests for edge cases (nested symlinks, circular symlinks)
- [ ] Fix exists tool to properly report symlink type when follow_symlinks=false (currently reports target type)

### Performance
- [ ] Profile the symlink path checking to ensure it doesn't impact performance for deep paths

## Low Priority

### Future Enhancements
- [ ] Consider adding a configuration option to disable symlink following globally
- [ ] Add metrics/logging for symlink usage patterns

## Improvements

- Consider consolidating the read tool's parameters
- Test the new hash tool after recompiling and restarting our session
- Recompile and restart to test the new READ_IMPROVEMENTS features
- Fix exists tool to properly detect symlinks when follow_symlinks=false (currently reports target type instead of "symlink")