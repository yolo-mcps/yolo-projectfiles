# TODO

## Recently Completed
- [x] Fixed symlink bug in `resolve_path_for_read` - now properly handles files within symlinked directories
- [x] Added test case `test_read_file_in_symlinked_directory` to ensure the fix works
- [x] Created dedicated `SymlinkAccessDenied` error type for clearer error messages
- [x] Updated all tool descriptions to document symlink behavior and `follow_symlinks` parameter
- [x] Added `resolve_path_allowing_symlinks` function to allow exists/stat tools to check symlink metadata
- [x] Fixed all test failures related to symlink error handling
- [x] Tested all tools with symlinks for security and proper behavior

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