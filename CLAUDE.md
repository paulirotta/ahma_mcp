# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Ahma MCP is a universal MCP (Model Context Protocol) server that dynamically adapts CLI tools for AI use. "Ahma" is Finnish for wolverine. The project enables creating intelligent agents from command-line tools using JSON configuration files.

**Core Mission**: Create agents from CLI tools with one JSON file, enabling true multi-threaded, async tool-use agentic AI workflows.

## Build and Test Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Test
cargo test                     # Run all tests
cargo test -p ahma_mcp         # Test specific crate
cargo test test_name           # Run specific test
cargo nextest run              # Parallel tests (faster)

# Quality checks (run before committing)
cargo fmt --all && cargo clippy --all-targets && cargo test

# Coverage
cargo llvm-cov --html

# MCP Inspector (interactive debugging)
./scripts/ahma-inspector.sh
```

## Workspace Structure

```
ahma_mcp/                     # Cargo workspace root
├── ahma_mcp/                 # Core library + binary (MAIN CRATE)
│   ├── src/
│   │   ├── adapter.rs        # CLI tool execution engine
│   │   ├── mcp_service/      # MCP ServerHandler implementation
│   │   ├── shell_pool.rs     # Pre-warmed shell process pool
│   │   ├── sandbox.rs        # Kernel-level sandboxing (Landlock/Seatbelt)
│   │   ├── config.rs         # MTDF (tool definition) models
│   │   └── shell/            # CLI entry points
│   └── tests/                # 159+ test files
├── ahma_http_bridge/         # HTTP-to-stdio bridge for web clients
├── ahma_http_mcp_client/     # HTTP MCP client with OAuth 2.0 + PKCE
├── ahma_validate/            # Tool config validator CLI
├── ahma_simplify/             # Code simplicity metrics aggregator CLI
├── generate_tool_schema/     # MTDF schema generator CLI
└── .ahma/tools/              # Tool JSON configurations
```

## Architecture

### Core Components

| Module | Purpose |
|--------|---------|
| `adapter` | Primary engine for executing external CLI tools (sync/async) |
| `mcp_service` | Implements `rmcp::ServerHandler` - handles `tools/list`, `tools/call` |
| `shell_pool` | Pre-warmed zsh/bash processes for 5-20ms command startup latency |
| `sandbox` | Kernel-level sandboxing (Landlock on Linux 5.13+, Seatbelt on macOS) |
| `operation_monitor` | Tracks background operations (progress, timeout, cancellation) |
| `config` | MTDF (Multi-Tool Definition Format) JSON config loading |

### Async-First Execution Model

Operations run asynchronously by default:
1. AI invokes tool -> Server immediately returns `operation_id`
2. Command executes in background via shell pool
3. On completion, result pushed via MCP `notifications/progress`
4. AI processes notification when it arrives (non-blocking)

**Execution mode inheritance** (highest to lowest priority):
1. `--sync` CLI flag
2. Subcommand `"synchronous": true/false`
3. Tool-level `"synchronous": true/false`
4. Default: async

### Built-in Tools (always available)

- `status` - Non-blocking progress check for async operations
- `await` - Blocking wait for operation completion
- `cancel` - Cancel running operations
- `sandboxed_shell` - Execute arbitrary shell commands within sandbox

## Key Patterns

### Error Handling
- Use `anyhow::Result` internally
- Convert to `rmcp::error::McpError` at MCP service boundary
- Include actionable context: `.with_context(|| "Failed to X because Y")`

### Async I/O
- **Never** use `std::fs` in async functions - use `tokio::fs`
- Test code is exempt from this rule

### Test File Isolation
All tests **must** use temporary directories via `tempfile` crate:
```rust
use tempfile::tempdir;
let temp_dir = tempdir().unwrap();
let test_file = temp_dir.path().join("test.txt");
```

### Binary Path Resolution in Tests
Use centralized helpers only:
```rust
use ahma_mcp::test_utils::cli::{get_binary_path, build_binary_cached};
```
Never manually access `CARGO_TARGET_DIR`.

## Usage Modes

```bash
# STDIO mode (IDE integration, default)
ahma_mcp --mode stdio --tools-dir .ahma/tools

# HTTP bridge mode
ahma_mcp --mode http --sandbox-scope /path/to/project

# CLI mode (single tool execution)
ahma_mcp cargo_build --working-directory . -- --release

# List tools mode
ahma_mcp --list-tools -- /path/to/ahma_mcp --tools-dir ./tools
```

## Security Model

The sandbox scope is set once at initialization and cannot change during the session. AI has full read/write access within the sandbox but zero write access outside.

- **Linux**: Uses Landlock (kernel 5.13+) for kernel-level FS sandboxing
- **macOS**: Uses `sandbox-exec` with Seatbelt profiles
- **Nested sandboxes**: Auto-detected (Cursor/VS Code/Docker) - use `--no-sandbox` when outer sandbox provides security

## Tool Definition (MTDF)

Tools are defined in JSON files in `.ahma/tools/`:
```json
{
  "name": "cargo",
  "description": "Rust's build tool",
  "command": "cargo",
  "subcommand": [
    {
      "name": "build",
      "description": "Compile the package",
      "options": [
        { "name": "release", "type": "boolean", "description": "Build in release mode" }
      ]
    }
  ]
}
```

Key fields:
- `"synchronous": true` for operations that modify config files (e.g., `cargo add`)
- `"format": "path"` on any path argument for security validation
- Schema: `docs/mtdf-schema.json`

## Reference Documentation

- **SPEC.md**: Single source of truth for architecture and requirements
- **AGENTS.md**: AI-specific development guidance and testing instructions
- **docs/USAGE_GUIDE.md**: Workflow patterns and CLI flag inheritance
