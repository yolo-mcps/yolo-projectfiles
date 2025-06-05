# Query Engine Integration Plan

## Overview
This document outlines the plan for integrating the shared query engine with tomlq, jq, and yq tools.

## Current Status

### Completed
- ✅ Created query engine infrastructure with modular design
- ✅ Implemented core operations (conditionals, pipes, expressions, etc.)
- ✅ Implemented comprehensive function library
- ✅ Updated tomlq documentation to reflect current capabilities
- ✅ Added comprehensive tests for tomlq
- ✅ Documented query engine architecture

### Pending
- ⏳ Integrate query engine with tomlq
- ⏳ Integrate query engine with jq
- ⏳ Integrate query engine with yq
- ⏳ Remove duplicated code from individual tools

## Integration Steps

### Phase 1: tomlq Integration
1. Replace current `simple_path_query` and related methods with query engine
2. Update error handling to use query engine errors
3. Implement TOML-specific type conversions
4. Test all existing functionality
5. Add tests for new capabilities

### Phase 2: jq Integration
1. Replace JsonQueryExecutor with query engine
2. Map existing error types to query engine errors
3. Preserve JSONPath support if needed
4. Ensure all tests pass
5. Remove duplicated code

### Phase 3: yq Integration
1. Replace YamlQueryExecutor with query engine
2. Map existing error types to query engine errors
3. Preserve YAML-specific features
4. Ensure all tests pass
5. Remove duplicated code

### Phase 4: Optimization
1. Add caching for parsed queries
2. Optimize common operations
3. Add benchmarks
4. Profile and improve performance

## Benefits of Integration

1. **Code Reuse**: Eliminate ~2000 lines of duplicated code
2. **Consistency**: Same behavior across all three tools
3. **Maintainability**: Fix bugs and add features in one place
4. **Testing**: Shared test suite for core functionality
5. **Performance**: Optimize once, benefit everywhere

## Risk Mitigation

1. **Incremental Integration**: Start with tomlq (simplest), then jq/yq
2. **Comprehensive Testing**: Ensure all existing tests pass
3. **Feature Flags**: Allow switching between old/new implementation
4. **Backwards Compatibility**: Preserve all existing functionality

## Future Enhancements

Once integration is complete:
1. Add streaming support for large files
2. Implement query compilation/caching
3. Add custom function registration
4. Support for more data formats (XML, CSV, etc.)
5. Query optimization passes