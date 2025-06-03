# TODO

## jq Tool Enhancement - Features for LLM/MCP Usage

### Phase 1: Essential Features (High Priority)

#### 1. **Conditional Logic & Error Handling**
- [ ] `if-then-else` conditionals - Essential for conditional transformations
- [ ] `and`/`or`/`not` boolean operators - Required for complex filtering
- [ ] `//` alternative operator (null coalescing) - Handle missing fields gracefully
- [ ] `?` optional operator (`.field?`) - Prevent errors on missing paths
- [ ] `try-catch` - Graceful error handling for robust queries

#### 2. **Array Operations**
- [ ] `add` - Sum array elements (common for aggregations)
- [ ] `unique` - Remove duplicates from results
- [ ] `sort`/`sort_by` - Order results for consistent output
- [ ] Array slicing `.[1:3]`, `.[-2:]` - Extract array portions
- [ ] `flatten` - Simplify nested array structures
- [ ] `reverse` - Reverse array order
- [ ] `first`/`last` - Quick array element access

#### 3. **Object & Key Operations**
- [ ] `has` - Check if key exists before accessing
- [ ] `del` - Remove fields from objects
- [ ] `with_entries` - Transform object key-value pairs
- [ ] Assignment operators: `+=`, `|=` - Modify values in place

#### 4. **Data Validation & Type Checking**
- [ ] `empty` - Produce no output (useful for filtering)
- [ ] `error` - Raise errors with messages
- [ ] Type predicates: `isnumber`, `isstring`, `isarray`, `isobject`, `isboolean`
- [ ] `paths`/`leaf_paths` - Enumerate all paths in structure

### Phase 2: Text Processing & Formatting (Medium Priority)

#### 5. **Advanced String Operations**
- [ ] `test`/`match` - Regex pattern matching
- [ ] `sub`/`gsub` - Find and replace with regex
- [ ] String interpolation `"Value: \(.field)"` - Build formatted strings
- [ ] `ltrimstr`/`rtrimstr` - Trim specific prefixes/suffixes

#### 6. **Output Formatting**
- [ ] `@base64`/`@base64d` - Encode/decode base64
- [ ] `@csv`/`@tsv` - Export to CSV/TSV format
- [ ] `@uri` - URL encoding
- [ ] `format` - Printf-style formatting

### Phase 3: Advanced Features (Lower Priority)

#### 7. **Aggregation & Transformation**
- [ ] `group_by` - Group elements by criteria
- [ ] `min`/`max`/`min_by`/`max_by` - Find extremes
- [ ] `reduce` - Fold/accumulate operations
- [ ] Variable binding `. as $x | ...` - Reuse values in complex queries

#### 8. **Utility Functions**
- [ ] `range` - Generate number sequences
- [ ] `to_entries`/`from_entries` enhancements - Better object manipulation
- [ ] `indices`/`index` - Find positions of elements
- [ ] `any`/`all` - Boolean aggregations over arrays

### Implementation Notes

#### Why These Features Matter for LLM/MCP Usage:

1. **Error Resilience**: Features like `?`, `//`, and `try-catch` prevent queries from failing on missing data, crucial when processing varied JSON structures.

2. **Data Extraction**: Array slicing, `has`, and conditional logic enable precise data extraction from complex nested structures.

3. **Data Transformation**: `if-then-else`, `with_entries`, and assignment operators allow in-place data modifications without full rewrites.

4. **Result Formatting**: String interpolation and format functions help create human-readable outputs or prepare data for other tools.

5. **Validation**: Type checking functions enable data validation before processing, preventing downstream errors.

6. **Aggregation**: `add`, `group_by`, and `reduce` support common data analysis tasks.

### Testing Strategy

- Create test files with diverse JSON structures (nested objects, arrays, mixed types)
- Test error handling with missing fields and type mismatches
- Verify performance with large datasets
- Ensure compatibility with existing jq syntax where possible

### Success Criteria

- All implemented features should match standard jq behavior
- Error messages should be clear and actionable
- Performance should handle files up to 100MB efficiently
- Documentation should include examples for each feature