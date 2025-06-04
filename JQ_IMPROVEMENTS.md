# JQ Tool Improvements

## Context

When using the `mcp__projectfiles__jq` tool, the current interface requires specifying 7 parameters even for simple operations. This makes it less convenient than using system commands for basic JSON viewing tasks.

## Suggested Improvements

### 1. Smarter Defaults for Optional Parameters

Most parameters could be optional with sensible defaults to reduce the cognitive load and make the tool more convenient for common use cases:

- `operation`: Default to `"read"` (most common operation)
- `output_format`: Default to `"json"` (natural format for jq operations)
- `in_place`: Default to `false` (safer default)
- `backup`: Default to `false` for read operations, `true` for write operations
- `follow_symlinks`: Default to `true` (expected behavior)

With these defaults, a simple read operation would only require:
```
{
  "file_path": "config.json",
  "query": "."
}
```

Instead of the current requirement:
```
{
  "file_path": "config.json",
  "query": ".",
  "operation": "read",
  "output_format": "json",
  "in_place": false,
  "backup": false,
  "follow_symlinks": true
}
```

### 2. Simplified Tool Signature

Consider making the tool function signature accept optional parameters properly, so that unspecified parameters use their defaults rather than requiring explicit values.

### Benefits

- **Reduced friction**: Makes the tool as convenient as shell commands for common operations
- **Better UX**: Users can start simple and add parameters only when needed
- **Maintains flexibility**: Advanced users can still specify all parameters when needed
- **Safer defaults**: Read-only by default, backup enabled for writes

## Implementation Note

These improvements would make the projectfiles jq tool the natural first choice for JSON operations within the project directory, rather than reaching for system commands out of habit.