# ahma_validate Requirements

## Overview

`ahma_validate` is a command-line tool for validating MCP Tool Definition Format (MTDF) tool configurations against the schema and checking for inconsistencies.

## Functional Requirements

### Core Validation

1. **Tool Configuration Validation**: Validate tool JSON files against the `ToolConfig` schema from `ahma_core`
   - Required fields: `name`, `description`, `command`
   - Optional fields: `subcommand`, `input_schema`, `timeout_seconds`, `force_synchronous`, `hints`, `enabled`, `guidance_key`, `sequence`, `step_delay_ms`, `availability_check`, `install_instructions`

2. **Guidance Configuration Loading**: Load and validate guidance configuration from JSON
   - Required fields: `guidance_blocks`
   - Optional fields: `templates`, `legacy_guidance`

3. **Multiple Target Support**: Support validating:
   - Single file
   - Directory of JSON files
   - Comma-separated list of files/directories

### CLI Interface

| Argument | Default | Description |
|----------|---------|-------------|
| `validation_target` | `.ahma/tools` | Path to directory or comma-separated list of files |
| `--guidance-file` | `.ahma/tool_guidance.json` | Path to guidance configuration |
| `-d, --debug` | `false` | Enable debug logging |

### Exit Behavior

- Exit 0: All configurations are valid
- Exit non-zero: One or more configurations are invalid

## Non-Functional Requirements

### Testing Standards

- **Minimum Coverage**: 90% line coverage for `src/main.rs`
- **Test Organization**: Tests grouped by function/module
- **Test Patterns**:
  - Use `tempfile::TempDir` for filesystem isolation
  - Table-driven tests where appropriate
  - Test both success and failure paths
  - Test edge cases (empty directories, missing files, invalid JSON)

### Code Quality

- Pass `cargo clippy` with no warnings
- Pass `cargo fmt` formatting
- All code documented with `cargo doc`

## Test Coverage Summary

| Module | Coverage |
|--------|----------|
| `main.rs` | 94.30% |

### Untested Code Paths (Acceptable)

1. `main()` function - CLI entry point with global state
2. File read error path in validation loop (requires filesystem permission errors)

## Dependencies

- `ahma_core`: Core MCP service and schema validation
- `anyhow`: Error handling
- `clap`: CLI argument parsing
- `serde_json`: JSON parsing
- `tracing`: Structured logging

### Dev Dependencies

- `tempfile`: Temporary directory creation for tests
