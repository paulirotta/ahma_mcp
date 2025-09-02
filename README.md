# Ahma MCP

_Create agents from your command line tools with one JSON file, then watch them complete your work faster with **true multi-threaded tool-use agentic AI workflows**._

<img src="./assets/ahma.png" height="250" align="right" alt="Ahma MCP Logo"/>

[![CI](https://github.com/paulirotta/ahma_mcp/actions/workflows/rust.yml/badge.svg)](https://github.com/paulirotta/ahma_mcp/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache: 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)

`ahma_mcp` rapdily adapts command-line tools for AI consumption. AI calls a gets rapid confirmation the background process has started and can continue to plan and analyze, automatically receiving tool results as each operation completes. Multiple concurrent tool calls execute in parallel. Individual tools and subcommands can be tagged as **`synchronous: true`** to become a traditional blocking MCP tool call as need. Generally the AI does not need to `wait` for async tool completion as it gets the result when the tool completes, but it can when needed.

**Ahma** (Finnish for wolverine) is a tenacious and agile tool, speeding your workflow to rapidly consume complex tasks in less time than common synchonous tools.

## Overview

Ahma MCP turns any command‑line tool into an MCP-aware agent that lets the AI continue planning and reasoning while commands run. It implements the Model Context Protocol (MCP) and uses JSON tool definitions (see [tools/](./tools/) for examples).

By default, tool calls are asynchronous: the AI receives a push/callback when the operation completes so the AI is not blocked waiting for results. Tools that return quickly can be marked "synchronous: true" in the tool JSON.

**Example: Claude Sonnet 4 never stops, simultaneously using tools and thinking**

> Now let me run the tests again using the MCP server:
>
> *Ran cargo_test ahma_mcp (MCP Server)*
>
> While the tests run, let me continue with updating the product requirements document:
>
> *Read product-requirements.md*
>
> Let me add the JSON schema requirements to the product requirements:
>
> *product-requirements.md+21-6*
>
> Now let me check on the test results:
>
> *Ran status ahma_mcp (MCP Server)*
>
> I can see the issue...

### Features
Ahma MCP is the next-generation successor to `async_cargo_mcp`, providing:

- **Universal CLI Adaptation**: Works with any command-line tool, not just Cargo
- **Async-First Architecture**: Operations execute asynchronously by default with automatic result push back to the AI, reducint AI blocking downtime and enabling concurrent workflows
- **Multi-Tool Support**: Single server handles multiple CLI tools simultaneously, one JSON file per tool
- **AI-Optimized Guidance**: Tool descriptions you can edit in JSON include explicit suggestion to AI to encourage productive concurrent work

_Note: `ahma_mcp` is early stage and undergoing rapid development. It is mostly tested in VS Code. Issue reports and pull requests welcome._

## Quick Start

The fastest way to try ahma_mcp with VS Code MCP support.

```bash
# 1) Clone and build the release binary
git clone https://github.com/paulirotta/ahma_mcp.git
cd ahma_mcp
cargo build --release

# 2) Copy the `tools/`, `.vscode/` and `.gihub/chatmodes/` into your VS Code project and edit the paths, tools and AI guidance for tool use to taste.
```

Then copy the contents into your VS Code MCP configuration file (per-OS locations below), restart VS Code, and you’re ready.

## Key Features

- **Async-First Execution**: Operations execute asynchronously by default with automatic MCP progress notifications when complete, eliminating AI blocking and enabling concurrent workflows.
- **Dynamic Tool Adaptation**: Automatically creates an MCP tool schema by inspecting a command-line tool's help documentation. No pre-configuration needed.
- **High-Performance Shell Pool**: Pre-warmed shell processes provide 10x faster command startup (5-20ms vs 50-200ms), optimizing both synchronous and asynchronous operations.
- **AI Productivity Optimization**: Tool descriptions include explicit guidance instructing AI clients to continue productive work rather than waiting for results.
- **Selective Synchronous Override**: Fast operations (status, version) can be marked synchronous in JSON configuration for immediate results without notifications.
- **Unified Tool Interface**: Exposes a single, powerful MCP tool for each adapted command-line application, simplifying the AI's interaction model.
- **Automatic Result Push**: Eliminates the need for polling or waiting - results are automatically pushed to AI clients when operations complete.
- **Customizable Tool Hints**: Provides intelligent suggestions to AI clients about productive parallel work they can perform while operations execute.

## Advanced Features

### Operation Management and Monitoring

Ahma MCP provides sophisticated operation tracking and management capabilities:

- **Real-time Operation Monitoring**: Track the status of all running operations with the `status` tool
- **Intelligent Wait Functionality**: Use the `wait` tool to monitor operations with configurable timeouts (10-1800 seconds, default 240s)
- **Progressive Timeout Warnings**: Receive warnings at 50%, 75%, and 90% of timeout duration to track long-running operations
- **Automatic Error Remediation**: Get specific suggestions when operations timeout, including:
  - Detection of stale lock files (Cargo.lock, package-lock.json, yarn.lock, composer.lock, etc.)
  - Network connectivity checks and offline mode suggestions
  - Disk space and resource usage monitoring recommendations
  - Process conflict detection and resolution steps
- **Partial Result Recovery**: When timeouts occur, completed operations return their results while failed operations provide detailed error context

### Graceful Development Workflow

Enhanced for seamless development experience:

- **Signal-Aware Shutdown**: Handles SIGTERM/SIGINT signals gracefully during file changes and cargo watch restarts
- **Operation Completion Grace Period**: Provides 10-second window for ongoing operations to complete naturally before shutdown
- **Development-Friendly Restarts**: File changes during development don't abruptly terminate operations - they complete and deliver results first
- **Progress Feedback**: Visual progress indicators (🔄⏳⚠️) show operation status during shutdown sequences

## Getting Started

### Prerequisites

- **Rust 1.70+**: Install from [rustup.rs](https://rustup.rs/)
- **VS Code**: With MCP support (GitHub Copilot extension recommended)
- **Git**: For cloning the repository

### Installation

1.  **Clone and build the repository**:

    ```bash
    git clone https://github.com/paulirotta/ahma_mcp.git
    cd ahma_mcp
    cargo build --release
    ```

2.  **Verify installation**:
    ```bash
    ./target/release/ahma_mcp --help
    ```

### Tool Configuration

Ahma MCP comes with several pre-configured tools in the `tools/` directory:

- `git.json` - Git version control (22 subcommands)
- `cargo.json` - Rust package manager (11 subcommands)
- `ls.json` - File listing
- `cat.json` - File viewing
- `grep.json` - Text searching
- `sed.json` - Stream editing
- `echo.json` - Text output

To add your own tools, create a `tools/<tool_name>.json` file.

### Testing the Installation

Run the test suite to verify everything works:

```bash
cargo test
```

You should see output like:

```
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured
```

## VS Code MCP Integration

### Step 1: Enable MCP in VS Code

Add to your VS Code settings (`Ctrl/Cmd+,` → search "settings.json"):

```json
{
  "chat.mcp.enabled": true
}
```

### Step 2: Configure the MCP Server

Create or edit your global MCP configuration file:

**Location:**

- **macOS**: `~/Library/Application Support/Code/User/mcp.json`
- **Linux**: `~/.config/Code/User/mcp.json`
- **Windows**: `%APPDATA%\Code\User\mcp.json`

**Configuration content** (replace `/absolute/path/to/ahma_mcp`):

```jsonc
{
  "servers": {
    "ahma_mcp": {
      "type": "stdio",
      "cwd": "/absolute/path/to/ahma_mcp",
      "command": "/absolute/path/to/ahma_mcp/target/release/ahma_mcp",
      "args": ["--tools-dir", "/absolute/path/to/ahma_mcp/tools"]
    }
  },
  "inputs": []
}
```

**Cross-platform examples:**

**macOS/Linux:**

```jsonc
{
  "servers": {
    "ahma_mcp": {
      "type": "stdio",
      "cwd": "/home/username/projects/ahma_mcp",
      "command": "/home/username/projects/ahma_mcp/target/release/ahma_mcp",
      "args": ["--tools-dir", "/home/username/projects/ahma_mcp/tools"]
    }
  },
  "inputs": []
}
```

**Windows:**

```jsonc
{
  "servers": {
    "ahma_mcp": {
      "type": "stdio",
      "cwd": "C:\\Users\\username\\projects\\ahma_mcp",
      "command": "C:\\Users\\username\\projects\\ahma_mcp\\target\\release\\ahma_mcp.exe",
      "args": ["--tools-dir", "C:\\Users\\username\\projects\\ahma_mcp\\tools"]
    }
  },
  "inputs": []
}
```

### Step 3: Restart VS Code

After saving the `mcp.json` file, restart VS Code completely to activate the MCP server.

### Step 4: Verify Connection

1. Open VS Code and start a chat with GitHub Copilot
2. You should see "ahma_mcp" listed in the available MCP servers
3. Test with a simple command like: "Use ahma_mcp to check git status"

### Available Tools

Once connected, you'll have access to ~38 dynamically generated MCP tools:

**Git Operations:**

- `mcp_ahma_mcp_git_status` - Check working tree status
- `mcp_ahma_mcp_git_add` - Stage changes
- `mcp_ahma_mcp_git_commit` - Create commits
- `mcp_ahma_mcp_git_push` - Upload changes
- `mcp_ahma_mcp_git_pull` - Download changes
- And 17+ more git subcommands

**Cargo Operations:**

- `mcp_ahma_mcp_cargo_build` - Build projects
- `mcp_ahma_mcp_cargo_test` - Run tests
- `mcp_ahma_mcp_cargo_run` - Run binaries
- `mcp_ahma_mcp_cargo_check` - Check without building
- `mcp_ahma_mcp_cargo_doc` - Build docs
- `mcp_ahma_mcp_cargo_add` - Add dependencies
- `mcp_ahma_mcp_cargo_remove` - Remove dependencies
- `mcp_ahma_mcp_cargo_update` - Update dependencies
- `mcp_ahma_mcp_cargo_fetch` - Fetch dependencies
- `mcp_ahma_mcp_cargo_install` - Install binaries
- `mcp_ahma_mcp_cargo_search` - Search crates
- `mcp_ahma_mcp_cargo_tree` - Dependency tree
- `mcp_ahma_mcp_cargo_version` - Cargo version
- `mcp_ahma_mcp_cargo_rustc` - Custom rustc
- `mcp_ahma_mcp_cargo_metadata` - Package metadata
- Optional if installed: `clippy`, `nextest`, `fmt`, `audit`, `upgrade`, `bump_version`, `bench`

**File Operations:**

- `mcp_ahma_mcp_ls_run` - List files
- `mcp_ahma_mcp_cat_run` - View file contents
- `mcp_ahma_mcp_grep_run` - Search text patterns
- `mcp_ahma_mcp_sed_run` - Edit text streams
- `mcp_ahma_mcp_echo_run` - Output text

**Operation Management:**

- `mcp_ahma_mcp_status` - Check status of all operations (active, completed, failed)
- `mcp_ahma_mcp_wait` - Wait for operations to complete with configurable timeout (10-1800s, default 240s)

### Troubleshooting

**MCP tools not working?**

1. Verify the binary path exists: `ls -la /path/to/your/ahma_mcp/target/release/ahma_mcp`
2. Check the tools directory path: `ls -la /path/to/your/ahma_mcp/tools/`
3. Restart VS Code completely
4. Check VS Code Developer Tools (Help → Toggle Developer Tools) for MCP errors

**"execution_failed" errors?**

- Ensure all paths in `mcp.json` are absolute (no `~` or environment variables)
- Use the direct binary path, not `cargo run`
- Verify file permissions: `chmod +x /path/to/your/ahma_mcp/target/release/ahma_mcp`
- If you updated the repo, rebuild the release binary: `cargo build --release`

**Performance issues?**

- Always use the pre-built binary path (not `cargo run`)
- Use absolute paths to avoid lookup delays
- Ensure `cargo build --release` has been run

**Operations timing out?**

- Default timeout is 240 seconds (4 minutes) - sufficient for most operations
- Use the `status` tool to check which operations are still running
- Use the `wait` tool with custom timeout: timeout range is 10-1800 seconds (30 minutes max)
- Common timeout causes include network issues, locked files, or resource contention
- Check for stale lock files (Cargo.lock, package-lock.json, yarn.lock, etc.)
- Verify network connectivity for download operations
- Monitor system resources with `top` or Activity Monitor during long builds

**Development workflow interrupted?**

- Ahma MCP includes graceful shutdown handling for cargo watch and file change restarts
- When files change during development, ongoing operations get 10 seconds to complete naturally
- Signal handling (SIGTERM/SIGINT) allows clean shutdowns instead of abrupt termination
- Operations receive completion notifications even during shutdown sequences

### Performance: Pre-warmed shell pool

Ahma MCP uses a pre-warmed shell pool to minimize command startup latency. See [shell_pool.rs](https://github.com/paulirotta/ahma_mcp/blob/main/src/shell_pool.rs)

## Creating Custom Tool Configurations

Want to add your own CLI tools? Create a JSON file in the repo's tools/ directory. See examples and templates here: [tools/](./tools/) (GitHub: https://github.com/paulirotta/ahma_mcp/tree/main/tools)

There is a [`docs/tool-schema-guide.md`](docs/tool-schema-guide.md)

- Complete documentation
- **Example Tools**: See real tool configurations in `tools/` (e.g., `cargo.json`, `python3.json`, `wait.json`)
- **IDE Support**: Schema enables autocompletion in development environments

## License

Licensed under either [Apache License 2.0](APACHE_LICENSE.txt) or [MIT License](MIT_LICENSE.txt).
