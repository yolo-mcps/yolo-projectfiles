# JQ Tool Complex Path Assignment Troubleshooting

## Issue Summary

After implementing complex path parsing fixes for the JQ tool, **complex path reading works perfectly** but **complex path assignment creates literal keys instead of updating nested paths**.

## What Works ✅

1. **Complex Path Reading**: All these work correctly
   - `.users[0].roles[1]` → returns `"user"`
   - `.users[0].profile.name` → returns `"Alice"`
   - `.users[0].profile.preferences.theme` → returns `"dark"`
   - `.config.database.credentials.username` → returns `"admin"`

2. **String Assignment Improvements**: Fixed double-quoting issues
   - Unquoted strings: `.field = hello` → stores as `"hello"` (not `"\"hello\""`)
   - Quoted strings: `.field = "test"` → stores as `"test"`
   - Booleans/numbers work correctly

3. **Error Messages**: Now helpful with examples
   - `.users | map(.name)` → shows "Pipe operations not supported" with examples
   - `invalid syntax` → shows "Query must start with '.'"

4. **Edge Cases**: Out-of-bounds indices return null instead of errors

## What Doesn't Work ❌

**Complex Path Assignment**: Creates literal keys instead of updating nested paths

### Example Problem
```bash
# Command:
.users[0].profile.email = "newemail@example.com"

# Expected: Update users[0].profile.email to "newemail@example.com"
# Actual Result: Creates literal key "users[0]" with nested structure:
{
  "users[0]": {
    "profile": {
      "email": "newemail@example.com"
    }
  }
}
# And users[0].profile.email remains unchanged: "alice@example.com"
```

## Troubleshooting Steps Taken

### 1. Verified Implementation Structure
- ✅ `parse_assignment()` correctly extracts path `.users[0].profile.email`
- ✅ `apply_assignment()` correctly calls `set_complex_path()` with `users[0].profile.email`
- ❌ `set_complex_path()` creates literal keys instead of navigating nested paths

### 2. Implementation Details
**File**: `crates/mcp-projectfiles-core/src/tools/jq.rs`

**Key Methods**:
- `parse_assignment()` - Works correctly, extracts path and value
- `apply_assignment()` - Calls `set_complex_path()` correctly  
- `set_complex_path()` - **This is where the bug is**

### 3. Code Changes Made
1. **Fixed**: Updated `apply_assignment()` to use `set_complex_path()` instead of simple dot-splitting
2. **Fixed**: Removed old `set_nested_value()` method (was unused, caused dead code warning)
3. **Implemented**: New `set_complex_path()` method mirroring the logic from `parse_complex_path()`

### 4. Testing Results
```bash
# Fresh file test - same issue occurs
mcp__projectfiles__copy source="test-files/complex-test.json" destination="test-files/jq-test-fresh.json"
mcp__projectfiles__jq file_path="test-files/jq-test-fresh.json" query=".users[0].profile.email = \"newemail@example.com\"" operation="write"

# Result: Still creates "users[0]" literal key
```

### 5. Debugging Observations
- **No compilation errors** after removing old methods
- **Simple assignments work**: `.simple_field = "test"` ✅  
- **Only complex paths with arrays fail**: anything with `[index]` ❌
- **Both fresh and existing files show same issue**: not a persistence problem
- **Real nested values remain unchanged**: confirms assignment logic isn't reaching the correct location

## Root Cause Hypothesis

The `set_complex_path()` method has a logical error in parsing array access patterns. Suspected issues:

1. **Array access parsing**: May not correctly handle `users[0]` → should parse as "users" field + [0] array access
2. **Path navigation**: May be creating object fields instead of navigating to array elements
3. **Fallback logic**: There may be hidden fallback code that creates literal keys when `set_complex_path()` fails

## Next Steps for Resolution

1. **Debug `set_complex_path()` step by step**:
   - Add debug prints to trace exactly what happens with path `"users[0].profile.email"`
   - Verify it correctly parses "users" → [0] → "profile" → "email"

2. **Compare with working `parse_complex_path()`**:
   - The read logic works perfectly, so mirror that exact logic for writes
   - Ensure identical character-by-character parsing

3. **Check for hidden fallback logic**:
   - Search codebase for any other assignment logic that might kick in
   - Verify no multiple assignment calls happening

4. **Test incremental complexity**:
   - `.users[0] = {...}` (array element assignment)
   - `.users[0].id = 999` (simple field in array element)  
   - `.users[0].profile.email = "test"` (nested field in array element)

## Files to Review
- `crates/mcp-projectfiles-core/src/tools/jq.rs` (lines 283-377: `set_complex_path()` method)
- `test-files/complex-test.json` (test data)
- `test-jq-improvements.md` (original test plan)

## Fix Applied

The issue was in `set_complex_path()` method. The key problem was that it was treating segments before array brackets as object fields and trying to create them if they didn't exist. 

**Solution**: Added lookahead logic to check if a segment is followed by `[` to determine if it's an array field. When it is, we use `get_mut()` to access the existing array instead of `entry().or_insert()` which was creating new object fields.

## Test Commands for Next Session
```bash
# 1. Rebuild after fixes
cargo build

# 2. RESTART SESSION to load the new compiled code

# 3. Test simple array assignment
mcp__projectfiles__jq file_path="test-files/complex-test.json" query=".users[0].id = 999" operation="write" in_place=true

# 4. Test nested array assignment  
mcp__projectfiles__jq file_path="test-files/complex-test.json" query=".users[0].profile.email = \"test@example.com\"" operation="write" in_place=true

# 5. Verify actual changes
mcp__projectfiles__jq file_path="test-files/complex-test.json" query=".users[0].id" operation="read" output_format="raw"
mcp__projectfiles__jq file_path="test-files/complex-test.json" query=".users[0].profile.email" operation="read" output_format="raw"

# 6. Run full test suite from test-jq-improvements.md
```