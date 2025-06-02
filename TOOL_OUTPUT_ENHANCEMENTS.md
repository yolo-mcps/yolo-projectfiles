# Tool Output Enhancements Plan

This document outlines planned enhancements to improve the output of all tools in the mcp-projectfiles MCP server.

## Overview

Our analysis identified several key areas for improvement across all tools:
- Inconsistent output formats between tools
- Limited control over output verbosity
- Missing contextual information in operation results
- No progress indicators for long-running operations
- Lack of operation summaries and statistics

## Global Enhancements

### 1. Standardized Output Format Options
**Priority: High**
- Add `--format` parameter to all tools with options: `text` (default), `json`, `simple`
- Ensure consistent JSON schemas across similar operations
- Maintain human-readable text as default for CLI usage

### 2. Verbosity Control
**Priority: Medium**
- Add `--verbose` flag for detailed operation information
- Add `--quiet` flag for minimal output (errors only)
- Default to moderate verbosity with key information

### 3. Progress Reporting
**Priority: Medium**
- Implement progress callbacks for operations on multiple files
- Show progress bars for long-running operations (copy, find, grep)
- Include ETA estimates where possible

## Tool-Specific Enhancements

### chmod
**Current**: Simple success message
**Enhancement**:
- Show before/after permissions in verbose mode
- Add dry-run mode to preview changes
- Include file count in pattern mode
- Show permission changes in octal and symbolic notation

### copy
**Current**: Success message with source/destination
**Enhancement**:
- Add size information (bytes copied)
- Show progress for large files/directories
- Display file count for directory copies
- Include transfer rate in verbose mode
- Add verification option to check copy integrity

### delete
**Current**: Success message with path
**Enhancement**:
- Show size of data deleted
- Display file/directory count
- Add recycling/trash option instead of permanent deletion
- Implement detailed confirmation prompt showing what will be deleted
- Add undo information in verbose mode

### diff
**Current**: Unified diff format
**Enhancement**:
- Add side-by-side diff option
- Include line count summary (added/removed/modified)
- Add option for word-level or character-level diffs
- Support syntax highlighting for code files
- Add summary statistics (similarity percentage)

### edit
**Current**: Success message with replacement count, optional diff
**Enhancement**:
- Always show line numbers where changes occurred
- Add preview mode to show changes without applying
- Include file size change information
- Show execution time for large files
- Add backup file location in verbose mode

### exists
**Current**: JSON with exists boolean and type
**Enhancement**:
- Add human-readable text format option
- Include basic file info (size, modified time) when exists
- Support checking multiple paths in one call
- Add glob pattern support

### file_type
**Current**: JSON with MIME type and binary status
**Enhancement**:
- Add detailed file analysis (encoding confidence, BOM detection)
- Include file magic number information
- Suggest appropriate tools for the file type
- Add language detection for source code files

### find
**Current**: List of matching paths
**Enhancement**:
- Add summary statistics (files found, directories searched, time taken)
- Include file details in verbose mode (size, modified time)
- Show search progress for large directory trees
- Add preview option to show first N results
- Group results by type or directory

### grep
**Current**: JSON with file paths and match details
**Enhancement**:
- Add context lines in output
- Show total match count across all files
- Include search time and files/lines searched
- Add option for plain text output with traditional grep format
- Support output pagination for many matches

### hash
**Current**: Hash value only
**Enhancement**:
- Add file size and name to output
- Support hashing multiple files with summary
- Include hash verification against provided checksum
- Add progress indicator for large files
- Show multiple hash algorithms simultaneously

### list
**Current**: List with [FILE]/[DIR] prefixes
**Enhancement**:
- Add tree-style indentation option
- Include cumulative size for directories
- Show file count summary
- Add color coding for file types (when terminal supports)
- Support custom sorting and grouping

### mkdir
**Current**: Success message with path
**Enhancement**:
- Show full created path (especially with parents=true)
- Display number of directories created
- Include permission information
- Add verbose mode showing each directory created

### move_file
**Current**: Success message with source/destination
**Enhancement**:
- Show file size being moved
- Add progress for large files
- Include rename detection (same directory moves)
- Display updated file tracking information
- Add conflict resolution options

### process
**Current**: JSON with process details
**Enhancement**:
- Add human-readable table format
- Include CPU and memory usage
- Show process tree/hierarchy
- Add sorting options (by CPU, memory, etc.)
- Include process age/uptime

### read
**Current**: Content with line numbers
**Enhancement**:
- Add syntax highlighting option for code files
- Include file metadata header (size, modified time)
- Show reading progress for large files
- Add option to show non-printable characters
- Include encoding detection result

### stat
**Current**: JSON with detailed metadata
**Enhancement**:
- Add human-readable format with nice formatting
- Include file type description
- Show relative timestamps ("2 hours ago")
- Add checksum information
- Include extended attributes

### touch
**Current**: Success message
**Enhancement**:
- Show before/after timestamps
- Display whether file was created or updated
- Include file size (for existing files)
- Add verbose mode showing all timestamp changes

### tree
**Current**: Tree structure with lines
**Enhancement**:
- Add file sizes and counts at each level
- Include modification times
- Support JSON output for parsing
- Add summary statistics at bottom
- Include gitignore respect option

### wc
**Current**: JSON with counts
**Enhancement**:
- Add traditional wc text format
- Include max line length
- Show statistics for multiple files with totals
- Add character encoding information
- Include reading time estimate

### write
**Current**: Success message with path and size
**Enhancement**:
- Show before/after file size (for overwrites)
- Include backup file location (when backup=true)
- Add write speed for large files
- Show line ending normalization if applied
- Include encoding confirmation

## Implementation Priority

1. **Phase 1** (Immediate):
   - Standardize success message formats
   - Add basic statistics to all operations
   - Implement --format option for tools currently using only JSON

2. **Phase 2** (Short-term):
   - Add verbosity controls
   - Implement progress reporting for long operations
   - Enhance operation context (before/after states)

3. **Phase 3** (Medium-term):
   - Add advanced format options (tables, trees)
   - Implement dry-run modes
   - Add operation previews

4. **Phase 4** (Long-term):
   - Syntax highlighting and rich formatting
   - Undo/redo capabilities
   - Advanced statistics and analytics

## Success Metrics

- Consistent output experience across all tools
- Reduced need for follow-up operations to get more info
- Better debugging capability with verbose modes
- Improved user confidence with preview/dry-run options
- Enhanced scriptability with structured outputs