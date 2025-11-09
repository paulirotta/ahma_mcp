# Ahma MCP

_Create agents from your command line tools with one JSON file, then watch them complete your work faster with **true multi-threaded tool-use agentic AI workflows**._

| | |
| --- | ---: |
| [![CI](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml/badge.svg)](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![License: Apache: 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0) [![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/) | ![Ahma MCP Logo](./assets/ahma.png) |

`ahma_mcp` rapidly adapts command-line tools for AI consumption. By default, AI tool calls execute synchronously and return immediate results. For long-running operations, tools can be marked as asynchronous: the AI receives immediate confirmation with an operation ID and can continue working while the command executes in the background, receiving results via automatic notifications when complete.

**Ahma** (Finnish for wolverine) is a tenacious and agile tool, enabling efficient workflows that complete complex tasks faster than traditional synchronous-only approaches.

## Key Features

- **Sync-First with Async Override**: Most tools return immediate results. Long-running operations (builds, tests) can be marked async for parallel execution.
- **Easy Tool Definition**: Add any command-line tool to your AI's arsenal by creating a single JSON file. No recompilation needed.
- **Sequence Tools**: Chain multiple commands into a single, powerful workflow (e.g., `rust_quality_check` runs format → lint → test → build).
- **Safe & Scoped**: Tools are safely scoped to the project's working directory.
- **Force Async Mode**: Use `--asynchronous` flag to make all tools execute asynchronously for maximum concurrency.

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
    ```

2. **Run tests to verify installation**:

    ```bash
    cargo test
    ```

The compiled binary will be at `target/release/ahma_mcp`.

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
                    "--server",
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
