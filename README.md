# Ahma MCP

_Create agents from your command line tools with one JSON file, then watch them complete your work faster with **true multi-threaded tool-use agentic AI workflows**._

<img src="./assets/ahma.png" height="250" align="right" alt="Ahma MCP Logo"/>

[![CI](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml/badge.svg)](https://github.com/paulirotta/ahma_mcp/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache: 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)

`ahma_mcp` rapdily adapts command-line tools for AI consumption. AI calls a gets rapid confirmation the background process has started and can continue to plan and analyze, automatically receiving tool results as each operation completes. Multiple concurrent tool calls execute in parallel. Individual tools and subcommands can be tagged as **`synchronous: true`** to become a traditional blocking MCP tool call as need. Generally the AI does not need to `await` for async tool completion as it gets the result when the tool completes, but it can when needed.

**Ahma** (Finnish for wolverine) is a tenacious and agile tool, speeding your workflow to rapidly consume complex tasks in less time than common synchonous tools.

## Overview

Ahma MCP turns any command‑line tool into an MCP-aware agent that lets the AI continue planning and reasoning while commands run. It implements the Model Context Protocol (MCP) and uses the **MCP Tool Definition Format (MTDF)** - JSON tool definitions that enable zero-compilation tool integration (see [tools/](./tools/) for examples).

By default, tool calls are asynchronous: the AI receives a push/callback when the operation completes so the AI is not blocked waiting for results. Tools that return quickly can be marked `"synchronous": true` in the tool JSON configuration.

### MTDF: MCP Tool Definition Format

MTDF is ahma_mcp's JSON-based tool configuration format that enables dynamic tool loading without code changes:

- **Dynamic Tool Registration**: Add new tools by creating JSON files in `tools/` directory
- **Zero-Compilation Integration**: No Rust code changes needed to add CLI tools
- **Recursive Subcommand Support**: Model complex CLI tools with nested subcommands (e.g., `cargo nextest run`, `gh cache delete`)
- **Clean Tool Naming**: "Default" subcommand pattern provides clean tool names (e.g., `cargo_fmt` instead of `cargo_fmt_default`)
- **Modular Architecture**: Optional tools in separate JSON files for improved maintainability
- **Centralized Guidance System**: Reusable AI guidance blocks via `tool_guidance.json`
- **Schema Validation**: Built-in MtdfValidator ensures configuration correctness
- **Synchronous/Asynchronous Control**: Fine-grained execution mode control per subcommand
- **JSON Schema Export**: Auto-generated schema at [`docs/mtdf-schema.json`](./docs/mtdf-schema.json) for validation and IDE support

**Example: Claude Sonnet 4 never stops, simultaneously using tools and thinking**

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

**Example: Gemini 2.5 Pro**

> I've fixed the compilation errors in adapter_coverage_improvement_test.rs. Now, I'll rerun the tests to check for further issues.
>
> _Ran cargo_nextest ahma_mcp (MCP Server)_
>
> I've addressed the compilation issues in adapter_coverage_improvement_test.rs and re-initiated the test suite. I'll now await the results.
>
> _Ran await ahma_mcp (MCP Server)_
>
> The fix for adapter_coverage_improvement_test.rs worked.

Tool-calling AIs work. Note that Grok Code Fast 1 currently appears lazy with tool use.

### Features

Ahma MCP is the next-generation successor to `async_cargo_mcp`, providing:

- **Universal CLI Adaptation**: Works with any command-line tool via MTDF JSON configurations
- **Async-First Architecture**: Operations execute asynchronously by default with automatic result push back to the AI, reducing AI blocking downtime and enabling concurrent workflows
- **Multi-Tool Support**: Single server handles multiple CLI tools simultaneously, one MTDF JSON file per tool
- **Test-Driven Development**: Comprehensive test coverage using `cargo nextest` with over 70 tests ensuring reliability
- **Code Coverage Integration**: Built-in `cargo llvm-cov` support for detailed test coverage analysis and reporting
- **Clean Tool Naming**: "Default" subcommand pattern provides intuitive tool names without redundant suffixes
- **Modular Tool Architecture**: Optional tools in separate JSON files for improved maintainability and organization
- **Centralized Guidance System**: Reusable guidance blocks in `tool_guidance.json` ensure consistent AI instructions across tools with `guidance_key` references
- **MtdfValidator**: Built-in schema validation ensures MTDF configuration correctness and prevents common errors
- **AI-Optimized Guidance**: Tool descriptions include explicit suggestions encouraging productive concurrent work

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
- **Dynamic Tool Adaptation**: Uses MTDF (MCP Tool Definition Format) to automatically create MCP tool schemas from JSON configurations. Zero-compilation tool integration.
- **High-Performance Shell Pool**: Pre-warmed shell processes provide 10x faster command startup (5-20ms vs 50-200ms), optimizing both synchronous and asynchronous operations.
- **AI Productivity Optimization**: Centralized guidance system with reusable blocks instructs AI clients to continue productive work rather than waiting for results.
- **Selective Synchronous Override**: Fast operations (status, version, check) can be marked `"synchronous": true` in JSON configuration for immediate results without notifications.
- **Unified Tool Interface**: Exposes MCP tools for each CLI application subcommand, simplifying the AI's interaction model with consistent naming patterns.
- **Automatic Result Push**: Eliminates the need for polling or waiting - results are automatically pushed to AI clients when operations complete via MCP notifications.
- **Customizable Tool Hints**: Provides intelligent suggestions to AI clients about productive parallel work they can perform while operations execute.

## Advanced Features

### Operation Management and Monitoring

Ahma MCP provides sophisticated operation tracking and management capabilities:

- **Real-time Operation Monitoring**: Track the status of all running operations with the `status` tool
- **Intelligent Wait Functionality**: Use the `await` tool to monitor operations with configurable timeouts (1-1800 seconds, default 240s)
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

Ahma MCP uses **MTDF** (MCP Tool Definition Format) JSON files in the `tools/` directory to define CLI tool integrations:

**Internal tools** (implemented in ahma_mcp):

- `await.json` - Operation coordination, pauses until one or more tools complete
- `status.json` - Ongoing and recently completed operation(s) information

**External tool definitions** for command line tools are included in `.ahma/tools/`. Copy and edit those you want, or add your own:

**Rust Ecosystem:**

- `cargo.json` - Rust package manager with recursive subcommands (13 subcommands)
- `cargo_audit.json` - Audit Cargo.lock for security vulnerabilities (2 subcommands)
- `cargo_bench.json` - Run benchmarks for a Rust project (1 subcommand)
- `cargo_clippy.json` - Enhanced linting and code quality checks for Rust (1 subcommand)
- `cargo_edit.json` - Tools for editing Cargo.toml files (4 subcommands)
- `cargo_fmt.json` - Formats Rust code according to style guidelines (1 subcommand)
- `cargo_llvm_cov.json` - LLVM source-based code coverage for Rust projects (8 subcommands)
- `cargo_nextest.json` - Next-generation test runner for Rust (1 subcommand)

**Version Control & CI/CD:**

- `git.json` - Git version control system (10 subcommands)
- `gh.json` - GitHub CLI with nested operations like `cache delete` and `run cancel` (10 subcommands)

**General Development:**

- `python3.json` - Python interpreter and module execution (7 subcommands)
- `gradlew.json` - Gradle wrapper for Android/Java projects (47 subcommands)
- `shell_async.json` - Execute shell commands asynchronously (1 subcommand)
- `long_running_async.json` - Sleep utility for testing async behavior (1 subcommand)
- `test_coverage.json` - Run test coverage scripts (1 subcommand)

**File Operations:**

- `cat.json` - View file contents (single command)
- `echo.json` - Output text (single command)
- `grep.json` - Text search with regex support (single command)
- `pwd.json` - Current directory (1 subcommand)
- `sed.json` - Stream editor for filtering and transforming text (single command)

Each MTDF file can reference guidance blocks from `.ahma/tool_guidance.json` using `guidance_key` fields, eliminating guidance duplication and ensuring consistency. For detailed schema information and validation, see [`docs/mtdf-schema.json`](./docs/mtdf-schema.json).

### Testing the Installation

Run the comprehensive test suite to verify everything works:

```bash
# Run all tests with nextest (faster test runner)
cargo nextest run

# Run tests with coverage analysis
cargo llvm-cov nextest --html --open

# Alternative: standard cargo test
cargo test
```

You should see output like:

```
✓ Finished running 76 tests across 45 binaries (2.34s)
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

````jsonc
{
    "servers": {
        "ahma_mcp": {
            "type": "stdio",
            "cwd": "${workspaceFolder}",
            "command": "target/release/ahma_mcp",
            "args": [
                "--server",
                "--tools-dir",
                "tools"
            ],
        }
    },
    "inputs": []
}```

### Step 3: Restart VS Code

After saving the `mcp.json` file, restart VS Code to activate the MCP server.

### Step 4: Verify Connection

1. Open VS Code and start a chat with GitHub Copilot
2. You should see "ahma_mcp" listed in the available MCP servers
3. Ask your AI to use the `ahma_mcp` tool, for example: "Use ahma_mcp to show the git status"

### Available Tools

Once connected, you'll have access to ~44 dynamically generated MCP tools:

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
- `mcp_ahma_mcp_cargo_audit_audit` - Audit dependencies for vulnerabilities
- `mcp_ahma_mcp_cargo_bench_bench` - Run benchmarks
- `mcp_ahma_mcp_cargo_clippy_clippy` - Lint code with clippy
- `mcp_ahma_mcp_cargo_edit_add` - Add dependency with cargo-edit
- `mcp_ahma_mcp_cargo_edit_remove` - Remove dependency with cargo-edit
- `mcp_ahma_mcp_cargo_edit_upgrade` - Upgrade dependencies with cargo-edit
- `mcp_ahma_mcp_cargo_edit_bump_version` - Bump package version with cargo-edit
- `mcp_ahma_mcp_cargo_fmt_fmt` - Format code with rustfmt
- `mcp_ahma_mcp_cargo_nextest_run` - Run tests with nextest
- `mcp_ahma_mcp_cargo_llvm_cov_test` - Run tests with LLVM coverage
- `mcp_ahma_mcp_cargo_llvm_cov_run` - Run binary with LLVM coverage
- `mcp_ahma_mcp_cargo_llvm_cov_report` - Generate coverage reports
- `mcp_ahma_mcp_cargo_llvm_cov_show_env` - Show coverage environment
- `mcp_ahma_mcp_cargo_llvm_cov_clean` - Clean coverage data
- `mcp_ahma_mcp_cargo_llvm_cov_nextest` - Run nextest with LLVM coverage
- Optional if installed: `clippy`, `nextest`, `fmt`, `audit`, `upgrade`, `bump_version`, `bench`

**File Operations (core set):**

- `mcp_ahma_mcp_cat_run` - View file contents
- `mcp_ahma_mcp_grep_run` - Search text patterns
- `mcp_ahma_mcp_sed_run` - Edit text streams
- `mcp_ahma_mcp_echo_run` - Output text

Optional (add if needed): a listing tool (`ls.json`) can be reintroduced; tests no longer assume its presence.

**Operation Management:**

- `mcp_ahma_mcp_status` - Check status of all operations (active, completed, failed)
- `mcp_ahma_mcp_wait` - Wait for operations to complete with configurable timeout (1-1800s, default 240s)

### Troubleshooting

**MCP tools not working?**

1. Verify the binary path exists: `ls -la /path/to/your/ahma_mcp/target/release/ahma_mcp`
2. Check the tools directory path: `ls -la /path/to/your/ahma_mcp/.ahma/tools/`
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
- Use the `await` tool with custom timeout: timeout range is 1-1800 seconds (30 minutes max)
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

Want to add your own CLI tools? Create an MTDF (MCP Tool Definition Format) JSON file in the `tools/` directory:

### Quick Start Template

```json
{
    "name": "your_tool",
    "description": "Brief description of the tool",
    "command": "your_command",
    "enabled": true,
    "subcommand": [
        {
            "name": "subcommand_name",
            "guidance_key": "sync_behavior",
            "synchronous": true,
            "description": "What this subcommand does",
            "options": [
                {
                    "name": "option_name",
                    "type": "string",
                    "description": "Option description"
                }
            ]
        }
    ]
}
```

### Guidance System

Use the centralized guidance system to maintain consistent AI instructions:

- Reference guidance blocks: `"guidance_key": "async_behavior"` for long-running operations
- Reference guidance blocks: `"guidance_key": "sync_behavior"` for fast operations
- Reference guidance blocks: `"guidance_key": "coordination_tool"` for await/status tools
- Custom guidance: Define reusable blocks in `tool_guidance.json`

### Documentation and Examples

- **Complete MTDF Specification**: [`docs/tool-schema-guide.md`](docs/tool-schema-guide.md)
- **Real Examples**: See live tool configurations in [`tools/`](./tools/) (e.g., `cargo.json`, `python3.json`, `gh.json`)
- **Schema Validation**: Built-in MtdfValidator provides helpful error messages and suggestions
- **IDE Support**: JSON schema enables autocompletion in development environments

## License

Licensed under either [Apache License 2.0](APACHE_LICENSE.txt) or [MIT License](MIT_LICENSE.txt).
````
