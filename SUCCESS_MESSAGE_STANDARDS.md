# Success Message Standards

This document defines the standardized formats for success messages across all mcp-projectfiles tools.

## General Principles

1. **Consistency**: All tools should follow predictable patterns
2. **Clarity**: Messages should be clear and informative
3. **Context**: Include relevant metrics and details
4. **Formatting**: Use consistent formatting for paths, sizes, and numbers

## Message Format Standards

### 1. File Operation Tools (read, write, copy, move, delete)

**Pattern**: `{Action} {type} '{path}' ({metrics})`

- Use past tense verbs without "Successfully"
- Use single quotes for paths
- Include metrics in parentheses
- Type should be "file" or "directory"

**Examples**:
```
Read file 'config.json' (2.5 KiB, 120 lines)
Wrote file 'output.txt' (1.2 KiB)
Copied file 'source.txt' to 'dest.txt' (3.8 KiB)
Moved directory 'old' to 'new' (15 files, 45.2 KiB)
Deleted file 'temp.txt' (512 B)
```

### 2. Creation/Modification Tools (mkdir, touch, chmod)

**Pattern**: `Created/Updated {type} '{path}'` or `Changed {attribute} for '{path}'`

- Use simple past tense
- No "Successfully" prefix
- Include what was changed

**Examples**:
```
Created directory 'new-folder'
Updated file 'data.txt' (timestamp)
Changed permissions to 755 for 'script.sh'
```

### 3. Edit Tool

**Pattern**: `Edited file '{path}' ({changes} at line {line})`

For multiple edits:
```
Edited file '{path}' ({total} changes)
- Line {n}: {brief description}
- Line {n}: {brief description}
```

### 4. Search/List Tools (grep, find, list, tree)

These tools should provide structured output with a summary line:

**Pattern**: `Found {count} {items} in {scope}`

**Examples**:
```
Found 25 matches in 5 files
Found 10 files matching '*.txt'
Listed 45 items in 'src/'
```

### 5. Information Tools (stat, exists, wc, hash, file_type)

These should provide structured data, with option for human-readable format:

**JSON Format** (default for programmatic use):
```json
{
  "path": "file.txt",
  "exists": true,
  "type": "file",
  "size": 1234
}
```

**Human Format** (with --human flag):
```
File: 'data.txt'
Size: 1.2 KiB
Type: text/plain
Hash: sha256:abc123...
```

## Size Formatting Standards

1. **Binary Units**: Use KiB, MiB, GiB (not KB, MB, GB)
2. **Precision**: 1 decimal place for sizes > 1 KiB
3. **Threshold**: Show bytes for < 1 KiB, otherwise use appropriate unit
4. **Function**: Create a common formatting function

**Examples**:
- 512 B
- 1.2 KiB
- 45.8 MiB
- 2.3 GiB

## Path Formatting Standards

1. **Quotes**: Always use single quotes around paths
2. **Relative**: Show relative paths when possible
3. **Escaping**: Handle paths with quotes properly

## Number Formatting Standards

1. **Large Numbers**: Use comma separators for > 999
2. **Counts**: "1 file" vs "2 files" (singular/plural)
3. **Percentages**: Show with 1 decimal place

**Examples**:
- 1,234 files
- 10,567 lines
- 45.2% complete

## Error Message Standards

**Pattern**: `Error: {operation} failed - {reason}`

**Examples**:
```
Error: Read failed - File not found: 'missing.txt'
Error: Write failed - Permission denied: 'protected.txt'
Error: Edit failed - No matches found for pattern
```

## Implementation Checklist

- [ ] Create common formatting utilities
- [ ] Update read tool messages
- [ ] Update write tool messages
- [ ] Update copy tool messages
- [ ] Update move tool messages
- [ ] Update delete tool messages
- [ ] Update edit tool messages
- [ ] Update creation tools (mkdir, touch)
- [ ] Update search tools (grep, find)
- [ ] Add human-readable options to JSON tools
- [ ] Standardize error messages
- [ ] Update tests for new formats