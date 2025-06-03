# TODO

## Session Resume Point
- Completed Phases 1-5 of JQ feature implementation:
  - Phase 1: Core Array Functions (add, min, max, unique, sort, sort_by, reverse) ✅
  - Phase 2: Advanced Array Operations (flatten, group_by, array slicing, indices) ✅  
  - Phase 3: Object Manipulation (has, del, with_entries, paths/leaf_paths) ✅
  - Phase 4: String Enhancements (test, match, ltrimstr/rtrimstr) ✅
  - Phase 5: Math Functions (floor, ceil, round, abs, modulo) ✅
- Phase 6: Debugging & Variables - PARTIALLY COMPLETE (empty and error implemented; variable binding pending)
- Also pending: Tool consistency improvements (parameter naming)

## JQ Feature Implementation Plan

### Summary of Completed Work
- Implemented 6 phases of JQ functionality with 52 passing tests
- Full support for array operations (add, min, max, unique, reverse, flatten, group_by, indices)
- Object manipulation (has, del, with_entries, paths, leaf_paths)
- String functions (test, match, ltrimstr, rtrimstr, trim, split, join, contains, startswith, endswith, ascii_upcase, ascii_downcase, tostring, tonumber)
- Math operations (floor, ceil, round, abs, modulo operator %)
- Conditionals (if-then-else, boolean operators and/or/not, alternative operator //, optional operator ?, try-catch)
- Debugging functions (empty, error)
- Variable binding (`as $var`) is the only pending feature requiring significant executor changes

### Phase 1: Core Array Functions
- [x] Implement `add` function for summing arrays
- [x] Implement `min/max` functions
- [x] Implement `unique` function
- [x] Implement `sort_by` function
- [x] Implement `reverse` function
- [x] Add tests for array aggregation functions

### Phase 2: Advanced Array Operations
- [x] Implement `flatten` function
- [x] Implement `group_by` function
- [x] Implement array slicing syntax `[start:end]`
- [x] Implement `indices` function
- [x] Add tests for advanced array operations

### Phase 3: Object Manipulation
- [x] Implement `has` function
- [x] Implement `del` function
- [x] Implement `with_entries` function
- [x] Implement `paths` and `leaf_paths`
- [x] Add tests for object manipulation

### Phase 4: String Enhancements
- [x] Implement `test` function for regex matching
- [x] Implement `match` function with capture groups
- [x] Implement `ltrimstr/rtrimstr` functions
- [x] Add tests for string functions

### Phase 5: Math Functions
- [x] Implement `floor/ceil/round` functions
- [x] Implement `abs` function
- [x] Implement modulo operator `%`
- [x] Add tests for math functions

### Phase 6: Debugging & Variables
- [x] Implement `empty` function
- [ ] Implement variable binding with `as $var` (requires significant executor changes)
- [x] Implement `error` function
- [x] Add tests for debugging features

## Tool Consistency Improvements
- [ ] Rename `show_line_numbers` parameter in grep tool to `linenumbers` for consistency with read tool
- [ ] Make case-insensitive parameters consistent: rename `pattern_case_insensitive` in read tool to `case_insensitive` to match grep tool (prefer shorter names)
- [ ] Change `skip_binary_check` parameter in read tool to `binary_check` (inverse logic) for clearer intent - ensure description is updated and adequate tests exist
- [ ] Fix edit tool output message "(1 change at line TBD)" - either show actual line numbers or remove the TBD aspect
- [ ] Consider adding line number context to edit tool error messages when string not found - show surrounding lines to help identify why match failed

## Notes
- Start with Phase 1 as these are the most commonly needed features
- Each function should handle edge cases gracefully
- Maintain consistency with existing error handling patterns
- Update tool description after each phase

## Completed in Previous Session
- [x] Updated Edit tool description to include warnings about exact string matching, whitespace sensitivity, and line number prefixes
- [x] Added `linenumbers` option to Read tool (defaults to true, set to false for clean content without line numbers)
- [x] Updated Read tool description to mention the linenumbers option
- [x] Added comprehensive tests for linenumbers functionality
- [x] Started implementing Phase 1 array functions:
  - [x] Implemented `add` function (sum numbers or concatenate strings)
  - [x] Implemented `min` function (find minimum value)
  - [x] Implemented `max` function (find maximum value)
  - [x] Implemented `unique` function (remove duplicates)
  - [x] Implemented `reverse` function (reverse array order)
  - [x] Implemented `sort` and `sort_by` functions with helper functions
- [ ] Still need to write tests for these implementations