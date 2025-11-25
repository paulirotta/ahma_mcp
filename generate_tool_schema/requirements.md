# Generate Tool Schema - Requirements

## Purpose

Generate JSON Schema definitions for MTDF (Multi-Tool Definition Format) tool configurations.

## Functionality

1. **Schema Generation**: Generate JSON Schema from `ToolConfig` type using `schemars`
2. **File Output**: Write schema to `mtdf-schema.json` in specified directory (default: `docs/`)
3. **Preview**: Display first 10 lines of generated schema for verification

## Command Line Interface

```bash
# Default output to docs/
generate_tool_schema

# Custom output directory
generate_tool_schema /custom/output/path
```

## Testing Standards

### Coverage Goals

- **Target**: ≥80% line coverage
- **Current**: ~91% line coverage

### Test Categories

1. **Unit Tests**: Test individual functions in isolation
   - Schema generation produces valid JSON
   - Argument parsing handles various input cases
   - File writing creates directories and handles overwrites
   - Preview generation respects line limits

2. **Integration Tests**: Test end-to-end workflow
   - Full workflow from generation to file output

### Test Patterns

- Use `tempfile::tempdir()` for filesystem tests
- Table-driven tests for input/output variations (e.g., `parse_output_dir`)
- Assert meaningful behavior, not just function calls
- Verify JSON schema structure, not exact content

## Dependencies

### Runtime

- `ahma_core`: Core library with `ToolConfig` definition
- `schemars`: JSON Schema generation
- `serde_json`: JSON serialization

## Code Quality

- All code passes `cargo clippy` without warnings
- Code is formatted with `cargo fmt`
- Documentation builds without errors
