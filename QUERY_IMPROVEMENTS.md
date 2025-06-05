# Query Engine Improvements

## Overview
This document captures findings from comprehensive testing of the jq, yq, and tomlq tools after migration to a unified query engine. Tests were performed using complex data structures with nested objects, arrays, and various data types.

## Common Issues Across All Tools

### 1. Mathematical Division Operations
**Issue**: Cannot perform division operations like `add / length`
**Affected**: jq, yq, tomlq
**Example**: `.metrics.performance.response_times | add / length` fails with type error
**Impact**: Common statistical calculations (averages) require workarounds

### 2. Length Function in Object Construction
**Issue**: Using `length` in object construction returns literal "length" string
**Affected**: jq, tomlq (yq works correctly)
**Example**: `group_by(.active) | map({active: .[0].active, count: length})` returns `count: "length"`
**Impact**: Cannot create summary statistics in transformed objects

### 3. Write Operation Requirements
**Issue**: Files must be read before writing (even though tools read internally)
**Affected**: All tools
**Impact**: Extra step required for simple updates

## Tool-Specific Issues

### jq Issues
1. **map(select()) behavior**: Returns nulls instead of filtering them out
2. **Complex expression parsing**: Some complex expressions with variables fail
3. **Type context requirements**: String operations need explicit context

### yq Issues
1. **Array iterator syntax**: `.users[].name` doesn't work (must use `.users | map(.name)`)
2. **Array slicing**: `[start:end]` syntax not supported
3. **Limited iterator support**: `[]` syntax has limitations compared to standard jq

### tomlq Issues
1. **Raw output limitations**: Arrays cannot be output in raw format
2. **String operations on iterations**: `.users[].name | ascii_upcase` fails
3. **Error handling**: Invalid queries sometimes return the query string instead of error
4. **TOML limitations**: null values convert to "null" string

## Inconsistencies Between Tools

1. **group_by + length behavior**:
   - yq: Works correctly
   - jq/tomlq: Returns literal "length" string

2. **Array iterator syntax**:
   - jq: Supports `.array[]` syntax
   - yq/tomlq: More limited support

3. **Error messages**:
   - Quality and clarity vary between tools
   - tomlq has less robust error handling

## Recommendations

### High Priority
1. **Fix arithmetic operations**: Enable division and other math operations on aggregated values
2. **Fix length function evaluation**: Ensure `length` evaluates correctly in all contexts
3. **Standardize array iteration**: Support `.array[]` syntax consistently across all tools
4. **Improve error handling**: Consistent, clear error messages for all tools

### Medium Priority
1. **Array slicing support**: Add `[start:end]` syntax to all tools
2. **Raw output improvements**: Better handling of complex types in raw format
3. **Relax write requirements**: Consider removing "must read first" requirement
4. **String operation context**: More intuitive handling of string operations on arrays

### Low Priority
1. **Documentation**: Create migration guide for standard jq users
2. **Performance testing**: Benchmark against standard jq/yq/tomlq
3. **Extended function support**: Add more jq built-in functions
4. **Type conversion helpers**: Better handling of type mismatches

## Positive Findings

1. **Recursive search** (`..field`): Works excellently across all tools
2. **Basic operations**: Core functionality is solid
3. **Complex transformations**: Most advanced features work well
4. **Output formats**: Multiple format support is consistent
5. **Write operations**: Safe with backup support
6. **Unified engine benefits**: Consistent behavior across data formats

## Testing Methodology

Tests were performed using complex data files containing:
- Nested objects and arrays
- Mixed data types (strings, numbers, booleans, dates)
- Multiple levels of nesting
- Various array sizes
- Real-world data structures (users, projects, metrics)

Each tool was tested with:
- Basic queries
- Array operations
- Filtering and selection
- String manipulation
- Mathematical operations
- Recursive searches
- Complex transformations
- Conditional logic
- Object manipulation
- Write operations
- Different output formats
- Error scenarios

## Conclusion

The unified query engine provides good consistency across jq, yq, and tomlq tools, with most core functionality working well. However, several issues need addressing to achieve parity with standard implementations and provide a seamless experience for users familiar with these tools.