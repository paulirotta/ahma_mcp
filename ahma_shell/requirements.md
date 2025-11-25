# ahma_shell Requirements

## Overview

`ahma_shell` is the main entry point binary for the ahma_mcp MCP server. It handles command-line argument parsing, initialization, and server startup.

## Testing Strategy

### Coverage Goals

- **Unit tests**: All pure helper functions must have comprehensive test coverage
- **Integration tests**: Server modes are tested via integration tests in the parent workspace
- **Target**: >80% coverage on testable functions

### Tested Functions

The following pure helper functions have comprehensive unit tests:

1. **`find_matching_tool`** - Finds the best matching tool configuration by prefix
   - Exact match when tool name matches key
   - Longest prefix match with multiple tools
   - Disabled tools are ignored
   - Error handling when no tool matches

2. **`find_tool_config`** - Finds tool configuration by key or name
   - Find by exact key match
   - Find by name when key differs
   - Key match takes precedence over name match
   - Returns None when not found

3. **`parse_env_list`** - Parses comma-separated environment variable lists
   - Returns None when env var not set
   - Parses single and multiple items
   - Trims whitespace and lowercases
   - Filters empty entries

4. **`should_skip`** - Checks if a value should be skipped based on a set
   - Returns false when set is None
   - Case-insensitive matching
   - Handles empty sets

5. **`resolve_cli_subcommand`** - Resolves CLI subcommand from tool name
   - Default subcommand resolution
   - Explicit subcommand resolution
   - Nested subcommand resolution
   - Disabled subcommand handling
   - Override parameter support

### Functions Not Unit Tested (Integration Test Coverage)

These async/complex functions are tested via integration tests:

- `run_cli_sequence` - Executes a sequence of CLI commands
- `run_http_bridge_mode` - Runs HTTP bridge server
- `run_server_mode` - Runs stdio MCP server
- `run_cli_mode` - Runs single CLI command
- `main` - Entry point (tested via binary invocation)

## Code Quality Requirements

1. **Formatting**: All code must pass `cargo fmt`
2. **Linting**: All code must pass `cargo clippy` without warnings
3. **Documentation**: All public functions must have doc comments
4. **Safety**: Unsafe blocks must have SAFETY comments explaining why they're safe

## Development Workflow

1. Write tests first (TDD)
2. Implement code to make tests pass
3. Run quality checks: `cargo fmt && cargo clippy --fix --allow-dirty && cargo test`
4. Generate coverage report: `cargo llvm-cov --package ahma_shell --html`

## Version History

- **v0.6.1**: Added comprehensive unit tests for 5 helper functions (27 tests)
