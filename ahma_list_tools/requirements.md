# ahma_list_tools Requirements

## Overview

A CLI utility to dump all MCP tool information from an MCP server to the terminal. Useful for testing, development, and verifying MCP server tool configurations.

## Supported Connection Methods

1. **stdio** - Connect via command-line arguments to a local MCP server binary
2. **mcp.json** - Connect using a configuration file (supports server selection)
3. **HTTP** - Connect to an HTTP-based MCP server endpoint

## Output Formats

- **text** (default) - Human-readable format with server info, tool names, descriptions, and parameters
- **json** - Machine-readable JSON output for automation

## Testing Standards

### Coverage Goals

- **Target**: ≥90% line coverage for `main.rs`
- **Current**: ~90% (as of 2025-11-25)

### Test Categories

1. **Unit Tests** (`src/main.rs` - `mod tests`)
   - Test helper functions in isolation
   - Cover edge cases and error paths
   - Use table-driven approach for input variations

2. **Integration Tests** (`tests/integration_test.rs`)
   - Test CLI with actual MCP server binaries
   - Verify output format correctness
   - Test configuration file parsing

### Testing Guidelines

- Use `tempfile` crate for temporary test files
- Test error conditions (missing files, invalid JSON, missing servers)
- Test serialization/deserialization roundtrips for config types
- Output functions should be tested for non-panic behavior (actual output captured in integration tests)

### Uncovered Areas (by design)

The following require live server connections and are tested via integration tests:

- `main()` - async runtime entry point
- `list_tools_stdio()` - requires live MCP server process
- `list_tools_http()` - requires running HTTP server
- `list_tools_from_config()` - delegates to stdio
- `convert_tool_to_output()` - tested implicitly via integration tests

## Dependencies

- `rmcp` - MCP protocol client
- `clap` - CLI argument parsing
- `serde`/`serde_json` - JSON serialization
- `reqwest` - HTTP client
- `tokio` - Async runtime
- `dirs` - Home directory expansion
- `ahma_core` - Logging utilities

## CLI Usage Examples

```bash
# Connect via command-line arguments
ahma_list_tools -- /path/to/ahma_mcp --tools-dir ./tools

# Connect via mcp.json
ahma_list_tools --mcp-config /path/to/mcp.json --server Ahma

# Connect to HTTP server
ahma_list_tools --http http://localhost:3000

# JSON output
ahma_list_tools --format json -- /path/to/server
```
