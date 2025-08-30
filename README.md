# Ahma MCP

`ahma_mcp` is a fast and ferocious tool for adapting existing command-line tools and web services for AI consumption. AI calls the tool, gets rapid confirmation and can continue to plan and analyze, getting a callback when the tool completes. Mutiple concurrent tool calls can be active with optional mutex blocking by `ahma_mcp` if needed. "Ahma" is Finnish for "wolverine."

## Overview

Ahma MCP is a Model Context Protocol server that dynamically adapts any command-line tool for asynchronous and concurrent use by AI assistants. Unlike tools designed for a single application (like `cargo`), Ahma discovers a tool's capabilities at runtime by parsing its `mytooltoadapt --help` output. A single configuration file (`tools/*.toml`) can optionally override `ahma_mcp`'s default tool availability and instructions to the calling AI.

AI can now use any command line interface (CLI) tool efficiently, queuing multiple commands and thinking productively while one or more tools execute in the background.

## Key Features

- **Dynamic Tool Adaptation**: Automatically creates an MCP tool schema by inspecting a command-line tool's help documentation. No pre-configuration needed.
- **Asynchronous by Default**: Enables concurrent execution of multiple tool commands, allowing the AI to continue working without blocking.
- **Optional Synchronous Mode**: Supports a `--synchronous` flag for simpler, blocking execution when needed.
- **Unified Tool Interface**: Exposes a single, powerful MCP tool for each adapted command-line application, simplifying the AI's interaction model.
- **Customizable Tool Hints**: Provides intelligent suggestions to the AI on what to think about while waiting for slow operations to complete, and allows users to customize these hints in a simple TOML configuration file.
- **Automatic Configuration Updates**: Keeps the tool configuration file (`tools/*.toml`) up-to-date with discovered commands and options, providing a clear and current reference for users.

## Getting Started

1.  **Installation**:

    ```bash
    git clone https://github.com/paulirotta/ahma_mcp.git
    cd ahma_mcp
    cargo build --release
    ```

2.  **Configuration**:
    Create a `tools/<tool_name>.toml` file. For example, to adapt `cargo`:

    ```toml
    # tools/cargo.toml
    command = "cargo"
    ```

3.  **Run the Server**:
    ```bash
    ./target/release/ahma_mcp --config tools/cargo.toml
    ```

## IDE Integration (VSCode with GitHub Copilot)

1.  Enable MCP in VSCode settings (`settings.json`):

    ```json
    {
      "chat.mcp.enabled": true
    }
    ```

2.  Add the server configuration using `Ctrl/Cmd+Shift+P` → "MCP: Add Server":

    ```json
    {
      "servers": {
        "ahma_mcp": {
          "type": "stdio",
          "cwd": "${workspaceFolder}",
          "command": "cargo",
          "args": [
            "run",
            "--release",
            "--bin",
            "ahma_mcp",
            "--",
            "--config",
            "tools/cargo.toml"
          ]
        }
      },
      "inputs": []
    }
    ```

3.  Restart VSCode to activate the server.

## License

Licensed under either [Apache License 2.0](APACHE_LICENSE.txt) or [MIT License](MIT_LICENSE.txt).
