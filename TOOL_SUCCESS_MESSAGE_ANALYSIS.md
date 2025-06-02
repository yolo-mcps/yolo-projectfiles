# Tool Success Message Analysis

This document analyzes the success message patterns found in the mcp-projectfiles tool implementations.

## Summary of Findings

### Message Format Patterns

1. **Plain Text Messages** - Most tools use simple string messages
2. **JSON Output** - Only `stat` tool returns JSON formatted output
3. **Multi-line Output** - `grep`, `find`, `list`, and `read` tools return structured multi-line output
4. **Metrics/Statistics** - Many tools include size, count, or performance metrics

### Common Success Message Patterns

#### 1. Action-Based Messages
Most tools follow the pattern: `"Successfully {action} {target}"`

Examples:
- `chmod`: "Successfully changed permissions to {mode} for '{path}'"
- `mkdir`: "Successfully created directory '{path}'"
- `touch`: "Successfully {action} file '{path}'" (where action = created/updated/touched)

#### 2. Completed Action Messages
Some tools use past tense: `"{Action} {target}"`

Examples:
- `copy`: "Copied {type} \"{source}\" to \"{destination}\" ({metrics})"
- `move`: "Moved {type} \"{source}\" to \"{destination}\" ({metrics})"
- `delete`: "Deleted {type} \"{path}\" ({metrics})"

#### 3. Summary Messages
Tools that process multiple items often include summaries:

Examples:
- `grep`: "Found {count} matches for pattern '{pattern}' in {files} files"
- `find`: "--- Found {count} items, searched {total} items total ---"

## Tool-by-Tool Analysis

### chmod
- **Pattern**: "Successfully changed permissions to {mode} for '{path}'"
- **Bulk Pattern**: "Successfully changed permissions for {count} {paths|path} matching pattern '{pattern}':
{list}"
- **Recursive**: Includes item count in parentheses
- **Metrics**: Item count for recursive operations

### copy
- **Pattern**: "Copied {file|directory} \"{source}\" to \"{destination}\" ({metrics})"
- **Metrics**: 
  - Files: Size in human-readable format
  - Directories: "{files} files, {dirs} directories, {size}"
- **Size Format**: Uses KiB, MiB, GiB units with 2 decimal places

### delete
- **Pattern**: "Deleted {file|directory} \"{path}\" ({metrics})"
- **Bulk Pattern**: "Deleted {count} {items|item} matching pattern \"{pattern}\" ({files} files, {dirs} directories, {size} freed):
{list}"
- **Metrics**:
  - Size freed (human-readable)
  - File and directory counts for recursive deletes
- **Special**: Lists deleted items in bulk mode

### edit
- **Single Edit**: "Successfully replaced {count} occurrence{s} in {file}"
- **Multi Edit**: "Successfully applied {count} edit{s} with {total} total replacement{s} in {file}"
- **Diff Mode**: Includes unified diff output when `show_diff=true`
- **Special Features**: Can show before/after diff, truncates large diffs

### write
- **Pattern**: "Successfully {wrote|appended} {bytes} bytes to {path}{backup_note}"
- **Backup Note**: " (backup created)" when backup=true
- **Metrics**: Raw byte count (not human-readable)

### read
- **Format**: Line-numbered output with tab separation
- **Pattern**: "{line_number}	{content}"
- **Truncation Messages**:
  - Normal: "[Truncated at line {n}. File has {total} total lines. Use offset={next} to continue reading]"
  - Pattern: "[Pattern matched {matches} lines out of {total} total.]"
  - Tail: "[Tail mode: Showing last {n} lines. File has {total} total lines.]"
- **Empty File**: "[No content at specified offset]"

### move
- **Pattern**: "Moved {file|directory} \"{source}\" to \"{destination}\" ({metrics})"
- **Metrics**:
  - File size (human-readable)
  - "(metadata preserved)" when preserve_metadata=true

### mkdir
- **Pattern**: "Successfully created directory '{path}'"
- **With Parents**: "Successfully created directory '{path}' (with parents)"
- **Already Exists**: "Directory '{path}' already exists"

### touch
- **Pattern**: "Successfully {action} file '{path}'"
- **Actions**: created, updated, touched
- **No metrics included**

### grep
- **Pattern**: "Found {matches} matches for pattern '{pattern}' in {files} files:"
- **No Matches**: "No matches found for pattern '{pattern}' in {files} files searched."
- **Output Format**: 
  - With line numbers: "{file}:{line}:	{content}"
  - Without: "{file}: {content}"
- **Context**: Shows lines before/after with "-" suffix
- **Limit Note**: "[limited to {max} results]"

### find
- **Output Format**: "{[FILE]|[DIR]} {path} ({size})"
- **Summary**: "--- Found {items} items (showing first {max}), searched {total} items total ---"
- **Size**: Human-readable for files, empty for directories

### list
- **Simple Format**: "{[FILE]|[DIR]} {name}"
- **Metadata Format**: "{[FILE]|[DIR]} {size:>10} {perms} {modified} {name}"
- **Sorting**: By name, size, or modified time
- **Permissions**: Unix-style (e.g., "-rw-r--r--")

### stat
- **Format**: JSON output (only tool using JSON)
- **Fields**: Comprehensive metadata including:
  - Path info (path, absolute_path)
  - Type info (type, is_file, is_dir, is_symlink)
  - Size (size, size_human)
  - Timestamps (modified, accessed, created)
  - Unix metadata (mode, uid, gid, permissions)
- **Pretty printed JSON output**

### Other Common Tools

#### exists
- Returns simple existence check result

#### diff
- Shows unified diff format between files

#### tree
- Displays hierarchical directory structure

#### wc (word count)
- Returns line, word, character, and byte counts

#### hash
- Returns checksum/hash of file

#### process
- Lists matching processes with PID and status

## Key Observations

1. **Consistency Issues**:
   - Some tools use "Successfully" prefix, others don't
   - Byte counts vs human-readable sizes are inconsistent
   - Quote usage varies (single vs double quotes)

2. **Error Handling**:
   - All tools use standardized error format from tool_errors
   - Pattern: "Error: projectfiles:{tool} - {message}"

3. **Metrics Presentation**:
   - File sizes use various units (B, KB/KiB, MB/MiB, etc.)
   - Some tools show raw bytes, others format human-readable
   - Statistical information varies widely between tools

4. **Output Types**:
   - Most return plain text via TextContent
   - Only `stat` returns structured JSON
   - Multi-line tools use consistent formatting

5. **Special Features**:
   - Pattern matching tools include match counts
   - Bulk operations list affected items
   - Some tools include execution metrics (item counts, sizes)

## Recommendations for Standardization

1. **Consistent Success Prefix**: Either always use "Successfully" or never use it
2. **Standardized Size Formatting**: Pick one unit system (binary KiB or decimal KB)
3. **Consistent Metrics**: Always include relevant metrics in parentheses
4. **Quote Consistency**: Standardize on double quotes for paths
5. **JSON Option**: Consider adding JSON output option for all tools
6. **Summary Format**: Standardize summary lines for bulk operations
