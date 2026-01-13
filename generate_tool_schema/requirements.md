# Ahma Generate Tool Schema Requirements

Technical specification for the `generate_tool_schema` utility, which provides the authoritative JSON Schema for MTDF.

---

## 1. Core Mission

`generate_tool_schema` is a build-time utility designed to generate the JSON Schema definitions for the Multi-Tool Definition Format (MTDF). It ensures that the schema used by developers and documentation is always in sync with the underlying Rust types in `ahma_core`.

## 2. Functional Requirements

### 2.1. Schema Generation

- **Type-Safe Generation**: Extract the JSON Schema from the `ToolConfig` Rust struct using `schemars`.
- **MTDF Authority**: Serve as the "Source of Truth" for valid tool configuration structures.

### 2.2. Output & Verification

- **File Output**: Automatically write the generated schema to `mtdf-schema.json`.
- **Target Directories**: Support a default output path (`docs/`) and allow overrides via CLI arguments.
- **Visual Preview**: Display a 10-line preview of the generated schema to the terminal for immediate verification.

### 2.3. Workflow Integration

- **Directory Creation**: Automatically create the target output directory if it does not already exist.
- **Graceful Overwrites**: Securely overwrite existing schema files to ensure they remain current.

## 3. Technical Stack

- **Core Definition**: `ahma_core` for the canonical `ToolConfig` definition.
- **Schema Engine**: `schemars` for Rust type to JSON Schema conversion.
- **Serialization**: `serde_json` for final file formatting.

## 4. Constraints & Rules

### 4.1. Implementation Standards

- **Sync Invariant**: The utility must be run whenever `ToolConfig` changes to prevent documentation drift.
- **Format Consistency**: Use standard JSON-RPC 2.0 and MCP naming conventions in the generated schema.

### 4.2. Testing Philosophy

- **Minimum Coverage**: Maintain ≥80% line coverage for the generation and file handling logic.
- **Filesystem Safety**: Use `tempfile::tempdir()` for all integration tests involving file operations.
- **Structural Validation**: Verify the output JSON structure for schema-specific markers (e.g., `$schema`, `required` fields).

## 5. User Journeys / Flows

### 5.1. Documentation Update Flow

1. Architect modifies `ToolConfig` in `ahma_core` to add a new parameter.
2. Architect runs `generate_tool_schema`.
3. The file `docs/mtdf-schema.json` is automatically updated.
4. Other tools (like `ahma_validate`) and developers now have the updated specification.

## 6. Known Limitations

- **Content vs. Structure**: Tests emphasize structural validity rather than verifying every specific field value, as field values are derived from `ahma_core`.
