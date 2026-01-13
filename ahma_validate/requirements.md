# Ahma Validate Requirements

Technical specification for the `ahma_validate` tool, which ensures MTDF tool configurations are correct and consistent.

---

## 1. Core Mission

`ahma_validate` is a specialized CLI utility designed to validate MCP Tool Definition Format (MTDF) JSON configurations against the unified schema. Its purpose is to provide fast, reliable feedback to tool developers, ensuring that definitions are safe, structurally sound, and ready for deployment.

## 2. Functional Requirements

### 2.1. Core Validation Logic

- **MTDF Schema Compliance**: Validate tool JSON definitions against the `ToolConfig` schema provided by `ahma_core`.
- **Field Integrity**: Verify presence of required fields (`name`, `description`, `command`) and correct formatting of optional fields (e.g., `timeout_seconds`, `synchronous`).
- **Semantic Consistency**: Check for internal inconsistencies, such as duplicate subcommand names or invalid `format: "path"` markers.

### 2.2. Batch & Target Support

- **Flexible Targets**: Support validation of single files, entire directories of JSON files, or comma-separated lists of both.
- **Recursive Discovery**: (Optional/Future) Ability to traverse subdirectories to find tool definitions.

### 2.3. CLI Experience

- **Clear Exit Codes**: Exit `0` for successful validation of all targets; exit non-zero if any configuration fails.
- **Actionable Errors**: Provide specific, human-readable error messages pointing to the line or field causing the validation failure.
- **Debug Mode**: Support `-d, --debug` for detailed diagnostic logging during the validation process.

## 3. Technical Stack

- **Core Engine**: `ahma_core` for shared schema and configuration types.
- **CLI Framework**: `clap` for robust argument parsing.
- **JSON Handling**: `serde_json` for parsing and schema-based validation.
- **Error Propagation**: `anyhow` for descriptive internal error reporting.
- **Logging**: `tracing` for structured output.

## 4. Constraints & Rules

### 4.1. Implementation Standards

- **Schema Single Source of Truth**: Always use the schema defined in `ahma_core` to prevent drift.
- **Performance**: Validation must be nearly instantaneous for typical tool directories (< 100ms).

### 4.2. Testing Philosophy

- **Mandatory Coverage**: Maintain at least 90% line coverage for the validation logic in `main.rs`.
- **Filesystem Isolation**: All tests involving file reads must use `tempfile::TempDir`.
- **Exhaustive Cases**: Tests must cover success paths, malformed JSON, missing files, and invalid schema instances.

## 5. User Journeys / Flows

### 5.1. Tool Development Feedback Loop

1. Developer creates or modifies a tool configuration in `.ahma/tools/`.
2. Developer runs `ahma_validate` (or it is triggered by a git hook).
3. `ahma_validate` scans the directory and reports any schema violations immediately.
4. Developer fixes the reported issues before proceeding to full server testing.

## 6. Known Limitations

- **Entry Point Coverage**: The `main()` function entry point and global state are excluded from strict unit test coverage targets.
- **Static Analysis Only**: Does not verify if the `command` actually exists or is executable (this is handled by `ahma_core` at runtime).
