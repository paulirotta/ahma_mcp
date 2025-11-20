# Ahma MCP

_Create agents from your command line tools with one JSON file, then watch them complete your work faster with **true multi-threaded tool-use agentic AI workflows**._

| | |
| --- | ---: |
| [![CI](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml/badge.svg)](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![License: Apache: 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0) [![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/) | ![Ahma MCP Logo](./assets/ahma.png) |

`ahma_mcp` rapidly adapts command-line tools for AI consumption. By default, AI tool calls execute synchronously and return immediate results. For long-running operations, tools can be marked as asynchronous: the AI receives immediate confirmation with an operation ID and can continue working while the command executes in the background, receiving results via automatic notifications when complete.

**Ahma** (Finnish for wolverine) is a tenacious and agile tool, enabling efficient workflows that complete complex tasks faster than traditional synchronous-only approaches.

## Key Features

- **Sandboxed Execution**: Strict path validation ensures tools cannot access files outside the workspace.
- **Sync-First with Async Override**: Most tools return immediate results. Long-running operations (builds, tests) can be marked async for parallel execution.
- **Easy Tool Definition**: Add any command-line tool to your AI's arsenal by creating a single JSON file. No recompilation needed.
- **Sequence Tools**: Chain multiple commands into a single, powerful workflow (e.g., `rust_quality_check` runs format → lint → test → build).
- **Force Async Mode**: Use `--async` flag to make all tools execute asynchronously for maximum concurrency.

## Security

Ahma MCP implements a strict security model to protect your system:

1. **Path Validation**: All file paths are validated to ensure they are within the current working directory. Access to parent directories (`..`) or absolute paths outside the workspace is blocked.
2. **Command Sanitization**: Shell commands are checked for dangerous patterns and unauthorized path access.
3. **Sync-by-Default**: Tools run synchronously to prevent race conditions and ensure deterministic execution, unless explicitly configured otherwise.

### How it Works: AI-driven workflow

Here's an example of Claude Sonnet 3.5's workflow, where it uses tools and thinks concurrently:

> Now let me run the tests again using the MCP server:
>
> _Ran cargo_test ahma_mcp (MCP Server)_
>
> While the tests run, let me continue with updating the product requirements document:
>
> _Read product-requirements.md_
>
> Let me add the JSON schema requirements to the product requirements:
>
> _product-requirements.md+21-6_
>
> Now let me check on the test results:
>
> _Ran status ahma_mcp (MCP Server)_
>
> I can see the issue...

## Getting Started

### Project Structure

`ahma_mcp` is organized as a Cargo workspace with two main crates:

- **`ahma_core`**: Library crate containing all core functionality (tool execution, configuration, async orchestration, MCP service)
- **`ahma_shell`**: Binary crate providing the CLI interface and server startup logic

This modular architecture ensures clean separation of concerns and enables future extensions (e.g., web interface, authentication).

### Installation

1. **Clone and build the repository**:

    ```bash
    git clone https://github.com/paulirotta/ahma_mcp.git
    cd ahma_mcp
    cargo build --release
    cargo run --release -- --help
    ```

2. **Add the MCP definition**:

    In your global `mcp.json` file add the following (e.g., Mac: `~/Library/Application Support/Code/User/mcp.json` or `~/Library/Application Support/Cursor/User/mcp.json`, or Linux: `~/.config/Code/User/mcp.json` or `~/.config/Cursor/User/mcp.json`).

    Update paths as needed. `--async` in the example below is optional

    ```json
    {
        "servers": {
                "Ahma": {
                "type": "stdio",
                "cwd": "~/github/ahma_mcp/",
                "command": "~/github/ahma_mcp/target/release/ahma_mcp",
                "args": [
                    "--async",
                    "--tools-dir",
                    "~/github/ahma_mcp/.ahma/tools"
                ]
            }
        }
    }
    ```

3. **Run tests to verify installation**:

    ```bash
    cargo test
    ```

The compiled binary will be at `target/release/ahma_mcp`.

## Usage Modes

`ahma_mcp` supports three modes of operation:

### 1. STDIO Mode (Default)

Direct MCP server over stdio for integration with MCP clients:

```bash
ahma_mcp --mode stdio --tools-dir ./tools
```

### 2. HTTP Bridge Mode

HTTP server that proxies requests to the stdio MCP server:

```bash
# Start HTTP bridge on default port (3000)
ahma_mcp --mode http

# Custom port
ahma_mcp --mode http --http-port 8080 --http-host 127.0.0.1
```

Then send JSON-RPC requests to `http://localhost:3000/mcp`:

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

### 3. CLI Mode

Execute a single tool command:

```bash
ahma_mcp cargo_build --working-directory . -- --release
```

## VS Code MCP Integration

To use `ahma_mcp` with GitHub Copilot in VS Code:

1. **Enable MCP in VS Code Settings**:

    ```json
    "chat.mcp.enabled": true
    ```

2. **Configure the MCP Server** in your global `mcp.json` file (e.g., `~/Library/Application Support/Code/User/mcp.json` on macOS).

    ```jsonc
    {
        "servers": {
            "ahma_mcp": {
                "type": "stdio",
                "cwd": "${workspaceFolder}",
                "command": "/path/to/your/ahma_mcp/target/release/ahma_mcp", // Use absolute path
                "args": [
                    "--mode",
                    "stdio",
                    "--tools-dir",
                    "tools"
                ],
            }
        }
    }
    ```

    **Important:** Replace `/path/to/your/ahma_mcp` with the absolute path to the cloned repository.

3. **Restart VS Code** and start a chat with GitHub Copilot. You can now ask it to use `ahma_mcp` tools (e.g., "Use ahma_mcp to show the git status").

## Project Philosophy and Requirements

This project is guided by a clear set of principles and requirements.

- **`requirements.md`**: This is the **single source of truth** for the project. It details the core mission, architecture, and the workflow an AI maintainer must follow. All new tasks and changes are driven by this document.
- **`docs/CONSTITUTION.md`**: This document outlines the core development principles for human contributors, ensuring consistency and quality.

For a list of available tools and instructions on how to add your own, please refer to the `requirements.md` file and the examples in the `tools/` directory.

## Troubleshooting

- **MCP tools not working?** Ensure the `command` path in `mcp.json` is an absolute path to the compiled `ahma_mcp` binary.
- **"execution_failed" errors?** Verify file permissions (`chmod +x /path/to/binary`) and ensure you have run `cargo build --release`.
- **Operations timing out?** The default timeout is 4 minutes. Use the `status` tool to check on long-running operations.

## License

Licensed under either [Apache License 2.0](APACHE_LICENSE.txt) or [MIT License](MIT_LICENSE.txt).
