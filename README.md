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

Ahma MCP speeds and simplifies AI-driven workflows by letting the AI continue planning while CLI tools run. Key benefits:

- **FAST**: Async-first execution: multiple tool operations **while** AI is also working reduces wall clock time to complete tasks.
- **EASY TOOL DEFINITION**: add a single JSON to `.ahma/tools/` to make command line tools available to AI. See [MTDF schema guide](./docs/mtdf-schema-guide.md).
- **SCOPED TOOL USE**: Safely expose tools since file paths can not be outside the working directory.
- **GUIDE AI TO SUCCESSFUL TOOL USE**: Guidance helps AI understand how to use your tools effectively and concurrently.

## Getting Started

### Installation

1.  **Clone and build the repository**:

    ```bash
    git clone https://github.com/paulirotta/ahma_mcp.git
    cd ahma_mcp
    cargo build --release
    ./target/release/ahma_mcp --help
    ```

2. **Choose Tools**:

    ```bash
    cp -r .ahma/ ~/.ahma/
    ```

Delete any tool JSON files you don't need from `~/.ahma/tools/`.

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
