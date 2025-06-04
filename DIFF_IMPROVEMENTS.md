# Diff Tool Improvements

## Overview
The diff tool is already well-implemented with excellent test coverage (20 tests) and good documentation. This document outlines potential improvements to make it even more useful for agentic coding LLMs and enhance the developer experience.

## Current State
- ✅ Basic diff functionality with unified format
- ✅ Configurable context lines
- ✅ Whitespace ignoring option
- ✅ Symlink support
- ✅ Comprehensive test coverage
- ✅ Clear documentation with examples

## Proposed Improvements

### Phase 1: Essential Features for LLM Usage

#### 1.1 Line Number Support
**Priority**: High  
**Description**: Add `show_line_numbers` parameter to include line numbers in diff output  
**Benefits**:
- LLMs can reference specific lines when discussing changes
- Easier to correlate diff output with file content
- Better for code review workflows

**Implementation**:
```json
{"file1": "old.py", "file2": "new.py", "show_line_numbers": true}
```

**Output format**:
```
-123: deleted line content
+456: added line content
 789: context line content
```

#### 1.2 Stats-Only Mode
**Priority**: High  
**Description**: Add `output_format: "stats_only"` option for summary without full diff  
**Benefits**:
- Quick overview of changes
- Useful for large files where full diff would be overwhelming
- Reduces token usage for LLMs

**Example output**:
```
15 additions(+), 8 deletions(-), 142 unchanged lines
Total changes: 23
Files differ in 5 locations
```

### Phase 2: Enhanced Output Formats

#### 2.1 Side-by-Side Format
**Priority**: Medium  
**Description**: Add `output_format: "side_by_side"` for visual comparison  
**Benefits**:
- Better for comparing similar structures
- Easier to see replacements vs additions/deletions
- More intuitive for some use cases

**Example**:
```
old.txt                          | new.txt
---------------------------------+---------------------------------
Line 1                           | Line 1
Line 2 original                  | Line 2 modified
Line 3                           | Line 3
                                 | + Line 4 added
```

#### 2.2 JSON Output Format
**Priority**: Medium  
**Description**: Add `output_format: "json"` for structured diff data  
**Benefits**:
- Machine-readable format
- Easy to process programmatically
- Useful for tools building on top of diff

**Example structure**:
```json
{
  "files": {
    "old": "file1.txt",
    "new": "file2.txt"
  },
  "stats": {
    "additions": 10,
    "deletions": 5,
    "unchanged": 100
  },
  "hunks": [
    {
      "old_start": 10,
      "old_lines": 5,
      "new_start": 10,
      "new_lines": 8,
      "changes": [
        {"type": "delete", "line": 12, "content": "old line"},
        {"type": "add", "line": 13, "content": "new line"}
      ]
    }
  ]
}
```

### Phase 3: Visual Enhancements

#### 3.1 Color Support
**Priority**: Low  
**Description**: Add `color` parameter to enable syntax highlighting  
**Benefits**:
- Better visual distinction between changes
- Uses existing DiffTheme infrastructure
- Improves readability

**Implementation**:
- Reuse `DiffTheme` from `theme.rs`
- Apply colors: additions (green), deletions (red), headers (cyan)
- Respect NO_COLOR environment variable

### Phase 4: Advanced Features

#### 4.1 Enhanced Context Options
**Priority**: Low  
**Features**:
- `show_function_context`: Include function/class names in hunk headers
- `context_type`: "unified" (default), "full", or "minimal"
- Smart context that includes logical code blocks

#### 4.2 Semantic Diff Features
**Priority**: Low  
**Features**:
- `ignore_comments`: Skip comment-only changes
- `ignore_imports`: Ignore import statement reordering
- `language`: Specify language for syntax-aware diff
- Integration with tree-sitter for AST-based comparison

#### 4.3 Performance Optimizations
**Priority**: Low  
**Features**:
- Streaming diff for very large files (>10MB)
- `max_changes`: Early termination after N changes
- Parallel processing for multiple file comparisons
- Memory-efficient algorithms for huge files

#### 4.4 Integration Features
**Priority**: Low  
**Features**:
- `base_revision`: Compare against git revision
- `patch_format`: Generate applicable patch files
- `three_way_diff`: Support for merge conflict resolution
- `blame_info`: Include git blame information in output

## Implementation Plan

### Step 1: Core Infrastructure
1. Refactor output generation to support multiple formats
2. Create output format abstraction
3. Add format validation

### Step 2: Basic Features (Phase 1)
1. Implement line number support
2. Add stats-only mode
3. Update documentation and tests

### Step 3: Format Extensions (Phase 2)
1. Implement side-by-side format
2. Add JSON output format
3. Create format-specific tests

### Step 4: Visual Features (Phase 3)
1. Integrate DiffTheme
2. Add color application logic
3. Test with various themes

### Step 5: Advanced Features (Phase 4)
1. Research and implement semantic features
2. Add performance optimizations
3. Build integration features

## Testing Strategy

### Unit Tests
- Test each output format independently
- Verify format-specific options
- Edge cases for each feature

### Integration Tests
- Combine multiple features
- Large file handling
- Performance benchmarks

### Compatibility Tests
- Ensure backward compatibility
- Verify all existing tests pass
- Check theme integration

## Documentation Updates

### Tool Description
- Add new parameters to description
- Include format-specific examples
- Document performance characteristics

### Examples Section
- Add examples for each output format
- Show combined feature usage
- Include real-world scenarios

## Success Metrics
- All existing tests continue to pass
- New features have >90% test coverage
- Documentation is clear and comprehensive
- Performance doesn't degrade for basic usage
- LLM users report improved usability

## Notes
- Maintain backward compatibility
- Keep default behavior unchanged
- Follow existing code patterns
- Coordinate with theme system changes
- Consider memory usage for large files