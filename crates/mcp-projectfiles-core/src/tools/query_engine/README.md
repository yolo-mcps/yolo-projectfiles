# Query Engine

A shared query execution engine for jq, yq, and tomlq tools. This module provides a unified implementation of query operations that can be used across different data formats.

## Architecture

### Core Components

1. **QueryExecutor Trait** (`executor.rs`)
   - Defines the interface for query execution
   - Provides both read and write operations
   - Format-agnostic design

2. **QueryEngine** (`executor.rs`)
   - Generic implementation of the QueryExecutor trait
   - Dispatches queries to appropriate handlers
   - Manages the execution flow

3. **QueryParser** (`parser.rs`)
   - Parses query strings into structured representations
   - Handles value parsing, expressions, and path parsing
   - Supports complex path navigation with array/object access

4. **Operations** (`operations.rs`)
   - Implements core query operations:
     - Conditional expressions (if-then-else)
     - Try-catch error handling
     - Pipe operations
     - Alternative operator (//)
     - Array and object construction
     - Path queries and navigation
     - Arithmetic and logical expressions

5. **Functions** (`functions.rs`)
   - Built-in function library:
     - Array functions: map, select, sort, unique, flatten, etc.
     - String functions: split, join, contains, startswith, etc.
     - Math functions: floor, ceil, round, abs
     - Type functions: keys, values, type, length
     - Conversion functions: tostring, tonumber

6. **Errors** (`errors.rs`)
   - Unified error types for query operations
   - Descriptive error messages for debugging

## Usage

To use the query engine in a tool:

```rust
use crate::tools::query_engine::{QueryEngine, QueryError};

let engine = QueryEngine::new();
let data = serde_json::json!({"name": "test", "value": 42});

// Read operation
let result = engine.execute(&data, ".name")?;

// Write operation
let mut data = data;
engine.execute_write(&mut data, ".status = \"active\"")?;
```

## Supported Operations

### Basic Queries
- `.field` - Access object field
- `.nested.field` - Nested field access
- `.array[0]` - Array index access
- `.users[*]` - All array elements
- `.[2:5]` - Array slicing

### Advanced Operations
- `map(.name)` - Transform array elements
- `select(.age > 18)` - Filter elements
- `sort_by(.date)` - Sort by field
- `group_by(.category)` - Group elements
- `.field // "default"` - Alternative operator
- `if .x then .y else .z end` - Conditionals
- `try .risky catch "failed"` - Error handling

### String Operations
- `split(",")` - Split string
- `join(" ")` - Join array to string
- `contains("test")` - Check substring
- `test("^[0-9]+$")` - Regex matching

### Arithmetic
- `.x + .y` - Addition
- `.price * 1.1` - Multiplication
- `.a % .b` - Modulo

## Integration Status

- ✅ Query engine infrastructure created
- ✅ Core operations implemented
- ✅ Function library implemented
- ⏳ Integration with tomlq pending
- ⏳ Integration with jq pending
- ⏳ Integration with yq pending

## Future Enhancements

1. **Performance Optimizations**
   - Query compilation/caching
   - Streaming support for large datasets
   - Parallel execution for independent operations

2. **Additional Features**
   - Custom function registration
   - Query validation before execution
   - Query optimization passes
   - Extended regex support

3. **Format-Specific Extensions**
   - TOML-specific date/time handling
   - YAML anchor/alias support
   - JSON Schema validation

## Testing

The query engine should be thoroughly tested with:
- Unit tests for each operation
- Integration tests with real queries
- Performance benchmarks
- Edge cases and error conditions