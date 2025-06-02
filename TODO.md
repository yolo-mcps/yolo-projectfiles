## Edit Tool Improvements
- [ ] Implement parameter validation to prevent mixing single/multi-edit modes
- [ ] Update tool description to clarify the two usage modes
- [ ] Add comprehensive documentation with JSON examples
- [ ] Add tests for parameter validation and mode detection
- [ ] Consider parameter restructuring for v2.0 (breaking change)

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
