# JSON Query (jq) Tool Documentation

## Overview

The `mcp__projectfiles__jq` tool is the **preferred tool for JSON manipulation** in your projects. It provides a safe, project-scoped alternative to system tools like `jq`, `cat | jq`, or manual JSON parsing. This tool should be your first choice when working with JSON files within your project directory.

## Why Use This Tool?

### Advantages over System Tools

1. **Project Isolation**: Operates only within your project directory, preventing accidental access to system files
2. **Consistent Interface**: Works seamlessly with other MCP project tools
3. **Atomic Operations**: Ensures data integrity with atomic writes
4. **Built-in Safety**: Optional backup creation before modifications
5. **No External Dependencies**: No need to check if `jq` is installed on the system
6. **Cross-Platform**: Works consistently across different operating systems

### When to Use This Tool

- **Always** prefer this over system `jq` command for project files
- Reading and querying JSON configuration files
- Transforming JSON data structures
- Extracting specific values from JSON files
- Updating JSON files programmatically
- Validating JSON structure through queries

## Core Features

### Basic Usage

```json
{
  "file_path": "config.json",
  "query": ".version",
  "operation": "read",
  "output_format": "raw"
}
```

### Parameters

- **file_path** (required): Path to JSON file relative to project root
- **query** (required): jq-style query string
- **operation**: "read" (default) or "write"
- **output_format**: "json" (default), "raw", or "compact"
- **in_place**: Modify file in-place for write operations (default: false)
- **backup**: Create backup before writing (default: false)
- **follow_symlinks**: Follow symlinks when reading files (default: true)

## Query Syntax Reference

### Data Access Patterns

#### Basic Access
- `.field` - Access top-level field
- `.nested.field` - Access nested field
- `.array[0]` - Access array element by index
- `.array[-1]` - Access last array element
- `.users[*].name` - Get all names from users array

#### Advanced Access
- `.[]` - Iterate through array elements
- `..` - Recursive descent (all values)
- `..email` - Find all email fields recursively
- `.users.*` - All values in users object
- `.data.items[*].id` - All IDs from items array

### Filtering and Selection

#### Basic Filtering
```json
// Select active users
{ "query": ".users | map(select(.active))" }

// Filter by age
{ "query": ".users | map(select(.age > 18))" }

// Multiple conditions
{ "query": ".users | map(select(.active and .age >= 21))" }
```

#### Complex Filtering
```json
// Nested conditions
{ "query": ".products | map(select(.price < 100 and .category == \"electronics\"))" }

// Array contains check
{ "query": ".users | map(select(.roles | contains([\"admin\"])))" }

// String matching
{ "query": ".logs | map(select(.message | test(\"error\"; \"i\")))" }
```

### Array Operations

#### Basic Operations
- `add` - Sum numbers or concatenate arrays/strings
- `min` / `max` - Find minimum/maximum values
- `unique` - Remove duplicates
- `reverse` - Reverse array order
- `sort` - Sort array
- `sort_by(.field)` - Sort by specific field
- `length` - Get array/object/string length

#### Advanced Operations
```json
// Flatten nested arrays
{ "query": ".data | flatten" }

// Group by category
{ "query": "group_by(.category)" }

// Find indices of value
{ "query": "indices(\"target\")" }

// Array slicing
{ "query": ".items[2:5]" }

// Array intersection
{ "query": "[.list1, .list2] | map(unique) | .[0] - (.[0] - .[1])" }
```

### Object Manipulation

#### Inspection
- `keys` - Get object keys
- `keys_unsorted` - Get keys in original order
- `values` - Get all values
- `has("field")` - Check if key exists
- `in(object)` - Check if value is key in object

#### Transformation
```json
// Delete field
{ "query": "del(.unwanted_field)" }

// Convert to entries
{ "query": "to_entries" }

// Transform and reconstruct
{ "query": "to_entries | map({name: .key, data: .value}) | from_entries" }

// Modify all values
{ "query": "with_entries(.value *= 2)" }

// Get all paths
{ "query": "paths" }
```

### String Operations

#### Basic String Functions
- `tostring` - Convert to string
- `tonumber` - Convert to number
- `length` - String length
- `trim` - Remove whitespace
- `split(",")` - Split by delimiter
- `join(" ")` - Join array to string

#### Advanced String Operations
```json
// Case conversion
{ "query": ".name | ascii_upcase" }
{ "query": ".title | ascii_downcase" }

// String trimming
{ "query": ".url | ltrimstr(\"https://\")" }
{ "query": ".filename | rtrimstr(\".json\")" }

// Pattern matching
{ "query": ".email | contains(\"@\")" }
{ "query": ".url | startswith(\"https\")" }
{ "query": ".id | test(\"^[0-9]+$\")" }

// Regex capture
{ "query": ".version | match(\"(\\\\d+)\\\\.(\\\\d+)\\\\.(\\\\d+)\") | .captures" }
```

### Mathematical Operations

#### Arithmetic
```json
// Basic math
{ "query": ".price * 1.2" }  // 20% increase
{ "query": ".total / .count" }  // Average
{ "query": ".value % 10" }  // Modulo

// Aggregations
{ "query": ".numbers | add" }  // Sum
{ "query": ".values | add / length" }  // Average
{ "query": ".scores | min" }  // Minimum
{ "query": ".scores | max" }  // Maximum
```

#### Math Functions
- `floor` - Round down
- `ceil` - Round up
- `round` - Round to nearest
- `abs` - Absolute value
- `sqrt` - Square root (if available)

### Conditional Logic

#### If-Then-Else
```json
// Simple conditional
{ "query": "if .age >= 18 then \"adult\" else \"minor\" end" }

// Nested conditionals
{ "query": "if .score > 90 then \"A\" elif .score > 80 then \"B\" else \"C\" end" }

// Conditional with side effects
{ "query": "if .premium then .discount = 0.2 else .discount = 0.1 end | ." }
```

#### Boolean Logic
```json
// AND logic
{ "query": ".users | map(select(.active and .verified))" }

// OR logic
{ "query": ".users | map(select(.role == \"admin\" or .role == \"moderator\"))" }

// NOT logic
{ "query": ".users | map(select(not .banned))" }
```

#### Null Handling
```json
// Default values
{ "query": ".timeout // 30" }  // Use 30 if timeout is null

// Safe navigation
{ "query": ".user.profile.email?" }  // Don't error if path missing

// Try-catch
{ "query": "try .risky.operation catch \"failed\"" }

// Alternative operator
{ "query": ".primary // .secondary // \"default\"" }
```

## Write Operations

### Simple Updates
```json
// Update single field
{
  "file_path": "config.json",
  "query": ".version = \"2.0.0\"",
  "operation": "write",
  "in_place": true
}

// Update nested field
{
  "file_path": "settings.json",
  "query": ".database.host = \"localhost\"",
  "operation": "write",
  "in_place": true
}
```

### Array Modifications
```json
// Update array element
{
  "query": ".users[0].active = true",
  "operation": "write"
}

// Append to array
{
  "query": ".tags += [\"new-tag\"]",
  "operation": "write"
}

// Remove from array
{
  "query": ".items = .items | map(select(.id != \"remove-me\"))",
  "operation": "write"
}
```

### Bulk Updates
```json
// Update all items
{
  "query": ".products |= map(.price *= 1.1)",
  "operation": "write"
}

// Conditional bulk update
{
  "query": ".users |= map(if .lastLogin < \"2023-01-01\" then .active = false else . end)",
  "operation": "write"
}
```

## Practical Examples

### Configuration Management
```json
// Read API endpoint
{
  "file_path": "config/api.json",
  "query": ".endpoints.production",
  "output_format": "raw"
}

// Update environment settings
{
  "file_path": "env.json",
  "query": ".development.debug = true | .development.logLevel = \"verbose\"",
  "operation": "write",
  "backup": true
}
```

### Data Extraction
```json
// Extract user emails
{
  "file_path": "users.json",
  "query": ".users | map(.email)",
  "output_format": "json"
}

// Get statistics
{
  "file_path": "sales.json",
  "query": "{total: .orders | length, revenue: .orders | map(.amount) | add, average: .orders | map(.amount) | add / length}"
}
```

### Data Transformation
```json
// Restructure data
{
  "file_path": "data.json",
  "query": ".users | map({id: .user_id, name: .full_name, contact: {email: .email, phone: .phone}})"
}

// Flatten nested structure
{
  "file_path": "nested.json",
  "query": ".departments | map(.employees) | flatten | map(.name)"
}
```

### Validation and Testing
```json
// Check for required fields
{
  "file_path": "schema.json",
  "query": ".required | map(. as $field | if $data | has($field) then null else $field end) | map(select(. != null))"
}

// Validate data types
{
  "file_path": "data.json",
  "query": ".items | map(select(.price | type != \"number\")) | length"
}
```

## Best Practices

### Performance Tips
1. Use specific paths instead of recursive searches when possible
2. Filter early in the pipeline to reduce processing
3. Avoid unnecessary `to_entries`/`from_entries` conversions
4. Use `limit(n)` for large datasets when you only need first N results

### Safety Guidelines
1. Always test complex queries with `operation: "read"` first
2. Use `backup: true` for important write operations
3. Validate data structure before writes
4. Use atomic operations for critical updates

### Query Development
1. Build queries incrementally
2. Test with sample data first
3. Use raw output format for debugging
4. Comment complex queries in your code

## Common Pitfalls and Solutions

### Issue: Query returns null
**Solution**: Check if the path exists using `has()` or use safe navigation `?`

### Issue: Type errors
**Solution**: Use type checking or conversion functions like `tostring`, `tonumber`

### Issue: Empty results from filter
**Solution**: Test your conditions separately, check for type mismatches

### Issue: Unexpected array wrapping
**Solution**: Use `.[]` to unwrap single-element results or `first` to get first element

## Advanced Techniques

### Custom Functions (Simulated)
```json
// Reusable transformations
{
  "query": "def normalize: ascii_downcase | trim; .users | map(.email | normalize)"
}
```

### Complex Data Pipelines
```json
// Multi-stage processing
{
  "query": ".raw_data | group_by(.category) | map({category: .[0].category, items: map(.id), total: map(.value) | add}) | sort_by(.total) | reverse"
}
```

### Cross-referencing
```json
// Join-like operations
{
  "query": ".users as $users | .orders | map(. + {user: $users[.user_id]})"
}
```

## Migration from System jq

If you're used to system `jq`, here's how to migrate:

### Instead of:
```bash
cat config.json | jq '.version'
jq '.users | length' users.json
jq '.settings.timeout = 30' settings.json > settings.tmp && mv settings.tmp settings.json
```

### Use:
```json
{ "file_path": "config.json", "query": ".version", "output_format": "raw" }
{ "file_path": "users.json", "query": ".users | length" }
{ "file_path": "settings.json", "query": ".settings.timeout = 30", "operation": "write", "in_place": true }
```

## Summary

The `mcp__projectfiles__jq` tool provides a comprehensive, safe, and efficient way to work with JSON files in your project. It should be your primary choice for JSON manipulation, offering advantages over system tools while maintaining familiar jq syntax. Whether you're reading configuration, transforming data, or updating JSON files, this tool provides the functionality you need with built-in safety and project isolation.