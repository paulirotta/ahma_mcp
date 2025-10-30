# Status Polling Anti-Pattern Prevention

## Problem

LLMs (including this conversation) were repeatedly calling the `status` tool instead of using `await`, demonstrating an inefficient polling anti-pattern that wastes resources and time.

## Root Cause Analysis

1. **Historical Context**: The codebase had polling detection in `operation_monitor.rs` (line 177), but it was removed/simplified
2. **Template Exists**: `STATUS_POLLING_HINT_TEMPLATE` constant exists in `constants.rs` but wasn't being actively used
3. **Weak Guidance**: Tool descriptions for `status` and `await` didn't strongly discourage the anti-pattern

## Solution Implemented

### 1. Enhanced Tool Descriptions (src/mcp_service.rs)

#### `await` Tool Description - BEFORE

```
"Wait for previously started asynchronous operations to complete. **WARNING:** This is a blocking tool and makes you inefficient. **ONLY** use this if you have NO other tasks and cannot proceed until completion. It is **ALWAYS** better to perform other work and let results be pushed to you."
```

#### `await` Tool Description - AFTER

```
"Wait for previously started asynchronous operations to complete. **WARNING:** This is a blocking tool and makes you inefficient. **ONLY** use this if you have NO other tasks and cannot proceed until completion. It is **ALWAYS** better to perform other work and let results be pushed to you. **IMPORTANT:** Operations automatically notify you when complete - you do NOT need to check status repeatedly. Use this tool only when you genuinely cannot make progress without the results."
```

#### `status` Tool Description - BEFORE

```
"Query the status of operations without blocking. Shows active and completed operations."
```

#### `status` Tool Description - AFTER

```
"Query the status of operations without blocking. Shows active and completed operations. **IMPORTANT:** Results are automatically pushed to you when operations complete - you do NOT need to poll this tool repeatedly! If you find yourself calling 'status' multiple times for the same operation, you should use 'await' instead. Repeated status checks are an anti-pattern that wastes resources."
```

### 2. Added Test Coverage (tests/status_polling_anti_pattern_test.rs)

Created comprehensive tests to validate:

- The `STATUS_POLLING_HINT_TEMPLATE` exists and contains proper guidance
- Tool descriptions provide the right messaging
- Design intent is captured for future enhancements

## Key Improvements

1. **Explicit Anti-Pattern Warning**: Status tool description now explicitly calls out "anti-pattern" language
2. **Clear Alternative**: Tells LLMs exactly what to do instead ("use 'await'")
3. **Explains Automatic Notifications**: Emphasizes that results are pushed automatically
4. **Stronger Discouragement**: Uses "do NOT" instead of softer language
5. **Educational**: Explains WHY polling is bad ("wastes resources")

## Future Enhancements (Not Implemented Yet)

The following were considered but not implemented to keep changes minimal and focused:

1. **Active Polling Detection**: Track status calls per operation and inject warnings after 2-3 rapid calls
2. **Response Augmentation**: Dynamically add warning messages to status responses when polling is detected
3. **Counter Reset Logic**: Reset polling counters after await is called
4. **Time-Based Detection**: Use timestamps to distinguish polling from legitimate periodic checks

These can be added later if the enhanced descriptions prove insufficient.

## Testing Strategy

- TDD approach: Tests written first (status_polling_anti_pattern_test.rs)
- Tests validate constants and design intent
- Can be extended to test actual polling detection if implemented later

## Expected Impact

LLMs reading these enhanced descriptions should:

1. Understand that status polling is explicitly discouraged
2. Know that results come automatically via notifications
3. Recognize when they're falling into the anti-pattern
4. Choose `await` when they actually need to block for results
5. Do other work while operations run in the background

## Validation

Run the test suite to ensure:

```bash
cargo nextest run status_polling_anti_pattern_test
```

All tests should pass, confirming that:

- The STATUS_POLLING_HINT_TEMPLATE is properly defined
- Tool descriptions contain the expected guidance
- No existing functionality is broken
