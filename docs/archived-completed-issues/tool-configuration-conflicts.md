# Tool Configuration Conflict Resolution

## Issue: Hardcoded Tools Overridden by JSON Files

### Problem Description

The ahma_mcp service has certain tools that are **hardcoded** in the MCP service (`await`, `status`, `cancel`) but JSON configuration files were created for these tools in the `tools/` directory. This created a conflict where:

1. **Expected Behavior**: These tools should be handled by special hardcoded logic in `src/mcp_service.rs`
2. **Actual Behavior**: The presence of JSON files caused them to be processed as regular command-line tools
3. **Consequence**: The system tried to execute non-existent `await`, `status`, `cancel` shell commands, causing failures and "Canceled: Canceled" messages

### Solution

**Removed the conflicting JSON files:**

- `tools/await.json` → `tools_backup/await.json`
- `tools/status.json` → `tools_backup/status.json`
- `tools/cancel.json` → `tools_backup/cancel.json`

### Prevention Guidelines

#### Tools That Must NOT Have JSON Files

These tools are hardcoded in the MCP service and should **never** have JSON configuration files:

1. **`await`** - Hardcoded in `src/mcp_service.rs` `call_tool()` method
2. **`status`** - Hardcoded in `src/mcp_service.rs` `call_tool()` method
3. **`cancel`** - Hardcoded in `src/mcp_service.rs` `call_tool()` method

#### Why These Tools Are Hardcoded

These tools need special handling because they:

- Operate directly on the operation monitor and internal state
- Don't execute external commands
- Need immediate synchronous responses
- Require access to MCP service internals

#### Tool Configuration Rules

1. **Hardcoded Tools**: No JSON files allowed - handled entirely in Rust code
2. **Regular Tools**: Must have JSON files in `tools/` directory
3. **Validation**: Use `--validate tools/` to check for conflicts

### Testing the Fix

After removing the JSON files, verify:

```bash
# Test status tool
mcp_ahma_mcp_status

# Test with background operation
mcp_ahma_mcp_long_running_async --duration 10
mcp_ahma_mcp_await --timeout_seconds 15

# Should work without "Canceled: Canceled" messages
```

### Code Review Checklist

When reviewing tool-related changes:

- [ ] No JSON files created for `await`, `status`, or `cancel` tools
- [ ] Hardcoded tool logic remains in `src/mcp_service.rs`
- [ ] New tools (that aren't hardcoded) have proper JSON configurations
- [ ] Validation passes: `./target/release/ahma_mcp --validate tools/`

### Future Enhancements

Consider adding runtime validation to detect and warn about this conflict:

```rust
// In tool loading code
if ["await", "status", "cancel"].contains(&tool_name) {
    warn!("JSON file found for hardcoded tool '{}' - this will cause conflicts", tool_name);
}
```

This issue demonstrates the importance of clear separation between hardcoded and configured tools.
