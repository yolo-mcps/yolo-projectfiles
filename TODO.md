## Test Infrastructure Refactoring - Context-Managed Path Resolution

### Problem Statement
Currently, many tool tests change the working directory using `std::env::set_current_dir()`, which causes:
- Race conditions when tests run in parallel
- Test failures when temp directories are cleaned up while still set as current directory
- Inconsistent test behavior requiring `--test-threads=1` flag
- Global state mutations that can affect other tests

### Solution
Convert all tool tests to use the existing `ToolContext::with_project_root()` pattern, which provides context-managed dependency injection for path resolution without changing the global working directory.

### Implementation Status - COMPLETED

All tool tests have been successfully converted to use context-managed dependency injection for path resolution. Tests now run in parallel without race conditions or global state mutations.

### Implementation Plan

#### Phase 1: Audit Current Test Infrastructure - COMPLETED
- [x] Identified all tests that use `std::env::set_current_dir()`:
  - **read.rs**: 9 occurrences in tests
  - **edit.rs**: 4 occurrences in tests
  - **integration_test.rs**: Uses ToolContext properly already
  - **config.rs**: 2 tests marked with #[serial]
- [x] Tools that already support `ToolContext` properly:
  - **grep.rs**: Uses `context.get_project_root()`
  - **list.rs**: Uses `context.get_project_root()`
  - **read.rs**: Uses `context.get_project_root()`
  - **exists.rs**: Uses utility function that gets project root
- [x] Tools that need modification (using `std::env::current_dir()` directly):
  - edit.rs, copy.rs, file_type.rs, write.rs, chmod.rs, delete.rs
  - wc.rs, tree.rs, stat.rs, find.rs, diff.rs, move_file.rs
  - touch.rs, mkdir.rs, hash.rs

#### Phase 2: Tool Implementation Updates - PARTIALLY COMPLETE
- [x] Updated StatefulTool implementations to use `context.get_project_root()`:
  - edit.rs ✓
  - copy.rs ✓
  - write.rs ✓
  - delete.rs ✓
  - move_file.rs ✓
  - read.rs (already using it) ✓
  - grep.rs (already using it) ✓
  - list.rs (already using it) ✓
- [ ] Convert non-StatefulTool tools to StatefulTool (requires more work):
  - chmod.rs, diff.rs, exists.rs, file_type.rs, find.rs
  - hash.rs, mkdir.rs, stat.rs, touch.rs, tree.rs, wc.rs
  - These currently use `.call()` and need conversion to use context

#### Phase 3: Test Conversion by Tool

**Read Tool Tests** (crates/mcp-projectfiles-core/src/tools/read.rs) ✓
- [x] Removed all `std::env::set_current_dir()` calls (9 occurrences)
- [x] Already using `ToolContext::with_project_root(temp_dir.path().to_path_buf())`
- [x] No `#[serial]` attributes to remove
- [x] All tests pass in parallel execution

**Edit Tool Tests** (crates/mcp-projectfiles-core/src/tools/edit.rs) ✓
- [x] Fixed path resolution to handle project root correctly
- [x] Removed `#[serial]` attributes (5 occurrences)
- [x] Removed `std::env::set_current_dir()` calls (4 occurrences)
- [x] All tests pass in parallel execution

**Write Tool Tests** (crates/mcp-projectfiles-core/src/tools/write.rs)
- [ ] Check if tests exist and use set_current_dir
- [ ] Convert to ToolContext pattern if needed

**Delete Tool Tests** (crates/mcp-projectfiles-core/src/tools/delete.rs)
- [ ] Check if tests exist and use set_current_dir
- [ ] Convert to ToolContext pattern if needed

**Copy Tool Tests** (crates/mcp-projectfiles-core/src/tools/copy.rs)
- [ ] Check if tests exist and use set_current_dir
- [ ] Convert to ToolContext pattern if needed

**Move Tool Tests** (crates/mcp-projectfiles-core/src/tools/move_file.rs)
- [ ] Check if tests exist and use set_current_dir
- [ ] Convert to ToolContext pattern if needed

**Other Tools** (list, grep, find, etc.)
- [ ] Audit each tool's tests for set_current_dir usage
- [ ] Convert as needed

#### Phase 4: Integration Test Updates
- [ ] Update integration tests in `tests/integration_test.rs`
- [ ] Ensure they use ToolContext pattern consistently
- [ ] Remove any serial test requirements

#### Phase 5: Test Helper Consolidation
- [ ] Create common test helpers that properly set up ToolContext
- [ ] Remove duplicate test setup code across tools
- [ ] Document the test patterns for future contributors

#### Phase 6: Verification - COMPLETED
- [x] All tests run successfully in parallel without `--test-threads=1`
- [x] No test failures or race conditions
- [x] No global state mutations in tool tests (only config tests still require serial)
- [x] CI/CD configurations can use default parallel testing

### Benefits
1. **Parallel Test Execution**: Tests can run concurrently, improving test suite performance
2. **Isolation**: Each test is isolated from others, preventing interference
3. **Reliability**: No more failures due to directory cleanup timing
4. **Consistency**: All tools use the same context-managed approach
5. **Maintainability**: Clearer test code without global state management

### Success Criteria
- All tests pass when run with `cargo test` (parallel by default)
- No uses of `std::env::set_current_dir()` in test code
- No `#[serial]` attributes needed for tool tests
- All tools respect ToolContext project root consistently

## Edit Tool Improvements - COMPLETED
- [x] Implement parameter validation to prevent mixing single/multi-edit modes
- [x] Update tool description to clarify the two usage modes
- [x] Add comprehensive documentation with JSON examples
- [x] Add optional `show_diff` parameter to display changes
- [x] Implement diff generation using existing `similar` crate
- [x] Add tests for parameter validation and mode detection

## Edit Tool - Testing After Restart

### Manual Testing Checklist for New Edit Features

After recompiling and restarting, test the enhanced edit functionality:

#### 1. Parameter Validation Tests
- [ ] Test mixing single and multi-edit parameters - should return error
- [ ] Test single edit mode works correctly
- [ ] Test multi-edit mode works correctly
- [ ] Test empty edits array - should return error

#### 2. Show Diff Feature Tests
- [ ] Test single edit with show_diff=true - verify diff output
- [ ] Test multi-edit with show_diff=true - verify all changes shown
- [ ] Test show_diff=false (default) - verify concise output
- [ ] Test diff truncation for large files (>100 lines)
- [ ] Test diff with no visible changes (whitespace only)

#### 3. Example Test Commands

**Test parameter mixing error**:
```
mcp__projectfiles__edit file_path="test.txt" old_string="old" new_string="new" edits=[{"old_string":"foo","new_string":"bar","expected_replacements":1}]
```

**Test single edit with diff**:
```
mcp__projectfiles__edit file_path="test.txt" old_string="Hello" new_string="Hello, World" expected_replacements=1 show_diff=true
```

**Test multi-edit with diff**:
```
mcp__projectfiles__edit file_path="test.txt" edits=[{"old_string":"version 1.0","new_string":"version 1.1","expected_replacements":1},{"old_string":"debug=false","new_string":"debug=true","expected_replacements":1}] show_diff=true
```

### Observations from Implementation

1. **Parameter Validation**: Added explicit check to prevent mixing single edit parameters (old_string/new_string) with multi-edit (edits array)
2. **Documentation**: Added comprehensive JSON examples for both modes in the struct documentation
3. **Diff Generation**: Reused the `similar` crate from diff.rs for consistency
4. **Testing**: Added unit tests with proper serial execution for file system operations
5. **Error Messages**: All follow the format "projectfiles:edit - message"

### Next Steps
- [ ] Consider adding a `dry_run` parameter to preview changes without applying
- [ ] Consider allowing regex patterns in addition to exact string matching
- [ ] Consider adding line number constraints for replacements

## Read Tool Testing - Priority Tasks After Restart

### IMMEDIATE: Manual Verification Tests

After recompiling and restarting, verify the pattern field fix works:

1. **Test basic read without pattern field**:

   ```
   mcp__projectfiles__read path="test-files/code.rs" offset=0 limit=5 skip_binary_check=false tail=false pattern_case_insensitive=false encoding="utf-8"
   ```

2. **Test read with pattern field**:

   ```
   mcp__projectfiles__read path="test-files/code.rs" pattern="fn" offset=0 limit=0 skip_binary_check=false tail=false pattern_case_insensitive=false encoding="utf-8"
   ```

3. **Test various combinations**:
   - Tail mode with/without pattern
   - Offset/limit with pattern filtering
   - Different encodings
   - Binary file detection

### Unit Test Implementation Tasks

#### 1. Create Test Data Files

Need to create test files in `test-files/` directory:

- **encoding-utf8-bom.txt** - UTF-8 with BOM marker
- **encoding-utf16le.txt** - UTF-16 Little Endian
- **encoding-utf16be.txt** - UTF-16 Big Endian
- **encoding-latin1.txt** - Latin1/ISO-8859-1
- **binary-sample.bin** - Small binary file for detection tests
- **large-text.txt** - File >8KB for large file testing
- **empty.txt** - Empty file
- **single-line.txt** - Single line without newline
- **special-chars.txt** - File with tab, newline, unicode chars
- **mixed-content.txt** - File with some non-printable chars (9% ratio)
- **binary-content.txt** - File with high non-printable chars (15% ratio)

#### 2. Unit Tests to Write in `crates/mcp-projectfiles-core/src/tools/read.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    // Basic functionality tests
    #[tokio::test] async fn test_read_basic_file()
    #[tokio::test] async fn test_read_empty_file()
    #[tokio::test] async fn test_read_single_line()
    #[tokio::test] async fn test_line_number_formatting()

    // Path security tests
    #[tokio::test] async fn test_path_traversal_blocked()
    #[tokio::test] async fn test_absolute_path_outside_project()
    #[tokio::test] async fn test_symlink_outside_project()
    #[tokio::test] async fn test_file_not_found()
    #[tokio::test] async fn test_directory_instead_of_file()

    // Binary detection tests
    #[tokio::test] async fn test_binary_file_detected()
    #[tokio::test] async fn test_text_file_passes_binary_check()
    #[tokio::test] async fn test_skip_binary_check_flag()
    #[tokio::test] async fn test_binary_threshold_edge_cases()

    // Encoding tests
    #[tokio::test] async fn test_utf8_encoding()
    #[tokio::test] async fn test_utf16_encoding()
    #[tokio::test] async fn test_latin1_encoding()
    #[tokio::test] async fn test_invalid_encoding_fallback()

    // Offset/limit tests
    #[tokio::test] async fn test_offset_from_beginning()
    #[tokio::test] async fn test_limit_partial_read()
    #[tokio::test] async fn test_offset_beyond_file()
    #[tokio::test] async fn test_offset_plus_limit()

    // Tail mode tests
    #[tokio::test] async fn test_tail_no_offset()
    #[tokio::test] async fn test_tail_with_offset()
    #[tokio::test] async fn test_tail_with_limit()
    #[tokio::test] async fn test_tail_empty_file()

    // Pattern filtering tests
    #[tokio::test] async fn test_pattern_basic_match()
    #[tokio::test] async fn test_pattern_case_insensitive()
    #[tokio::test] async fn test_pattern_no_matches()
    #[tokio::test] async fn test_pattern_invalid_regex()
    #[tokio::test] async fn test_pattern_with_offset_limit()
    #[tokio::test] async fn test_pattern_with_tail()

    // Large file tests
    #[tokio::test] async fn test_large_file_binary_detection()
    #[tokio::test] async fn test_large_file_memory_efficiency()

    // Error condition tests
    #[tokio::test] async fn test_permission_denied()
    #[tokio::test] async fn test_io_error_during_read()
}
```

#### 3. Integration Tests to Write in `tests/read_integration_test.rs`

```rust
// End-to-end scenarios
#[tokio::test] async fn test_read_complex_scenario()
#[tokio::test] async fn test_read_state_management()
#[tokio::test] async fn test_read_performance_large_files()
#[tokio::test] async fn test_read_with_other_tools_context()
```

### Manual Testing Checklist After Implementation

#### Core Functionality

- [ ] Read simple text files
- [ ] Verify line number formatting
- [ ] Test empty files and single lines
- [ ] Test files with/without trailing newlines

#### Security & Path Handling

- [ ] Test relative paths within project
- [ ] Test absolute paths within project
- [ ] Verify path traversal is blocked
- [ ] Test symlinks pointing outside project
- [ ] Test non-existent files return proper errors
- [ ] Test directories return proper errors

#### Binary Detection

- [ ] Test actual binary files are rejected
- [ ] Test text files pass detection
- [ ] Test edge cases around 10% threshold
- [ ] Test skip_binary_check flag works

#### Encoding Support

- [ ] Test UTF-8 files (with and without BOM)
- [ ] Test UTF-16LE and UTF-16BE files
- [ ] Test Latin1/ASCII files
- [ ] Test invalid encoding handling

#### Offset/Limit Features

- [ ] Test offset=0 (beginning of file)
- [ ] Test various offset values
- [ ] Test limit=0 (read all)
- [ ] Test various limit values
- [ ] Test offset beyond file length
- [ ] Test limit beyond file length

#### Tail Mode

- [ ] Test tail with no offset (read from end)
- [ ] Test tail with offset (skip N from end)
- [ ] Test tail with limit (last N lines)
- [ ] Test tail on empty files
- [ ] Test tail where offset > file length

#### Pattern Filtering

- [ ] Test basic regex patterns
- [ ] Test case-sensitive vs case-insensitive
- [ ] Test complex regex patterns
- [ ] Test invalid regex patterns return errors
- [ ] Test patterns with no matches
- [ ] Test patterns with offset/limit
- [ ] Test patterns with tail mode

#### Error Handling

- [ ] Test file not found errors
- [ ] Test access denied errors
- [ ] Test binary file errors
- [ ] Test invalid regex errors
- [ ] Test encoding errors
- [ ] Verify all error messages follow format: "projectfiles:read - message"

#### Performance & Edge Cases

- [ ] Test large files (>8KB)
- [ ] Test very large files for memory usage
- [ ] Test files with very long lines
- [ ] Test files with many lines

### Implementation Priority

1. Create test data files first
2. Implement basic unit tests
3. Add security and error tests
4. Add complex feature tests (pattern + offset/limit/tail)
5. Add integration tests
6. Run manual verification tests
7. Performance testing with large files

### Notes for Restart Session

- The pattern field fix is complete, build succeeded
- Ready to test that mcp**projectfiles**read works without missing field error
- Focus on comprehensive testing since read is a critical tool
- Use projectfiles tools for all file operations during testing (dogfooding)
