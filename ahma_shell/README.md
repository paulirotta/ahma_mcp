# Ahma Shell

Command-line interface and server launcher for the Ahma MCP system.

## Overview

`ahma_shell` is the main binary crate that provides the CLI and server modes for Ahma MCP. It supports three distinct modes of operation:

1. **STDIO Mode** - MCP server over stdin/stdout for direct client integration
2. **HTTP Bridge Mode** - HTTP server that proxies to the stdio MCP server
3. **CLI Mode** - Direct command execution for single tool invocations

## Usage

### STDIO Mode (Default)

For integration with MCP clients like VS Code, Cursor, or Claude Desktop:

```bash
ahma_mcp --mode stdio --tools-dir ./tools
```

### HTTP Bridge Mode

For HTTP clients and web applications:

```bash
# Start on default port (3000)
ahma_mcp --mode http

# Custom port and host
ahma_mcp --mode http --http-port 8080 --http-host 127.0.0.1
```

### CLI Mode

Execute a single tool command:

```bash
ahma_mcp cargo_build --working-directory . -- --release
ahma_mcp git_status --working-directory .
```

## Command-Line Options

### Mode Selection
- `--mode <MODE>` - Server mode: 'stdio' (default) or 'http'

### HTTP Options
- `--http-port <PORT>` - HTTP server port (default: 3000)
- `--http-host <HOST>` - HTTP server host (default: 127.0.0.1)

### Common Options
- `--tools-dir <DIR>` - Tool configuration directory (default: .ahma/tools)
- `--guidance-file <FILE>` - Guidance JSON file (default: .ahma/tool_guidance.json)
- `--timeout <SECONDS>` - Command timeout (default: 300)
- `-d, --debug` - Enable debug logging
- `--async` - Force asynchronous mode for all operations

## Building

```bash
cargo build --release
```

The binary will be at `target/release/ahma_mcp`.

## License

MIT OR Apache-2.0

