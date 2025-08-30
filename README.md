# Ahma MCP

[![CI](https://github.com/paulirotta/ahma_mcp/actions/workflows/rust.yml/badge.svg)](https://github.com/paulirotta/ahma_mcp/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache: 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)

`ahma_mcp` is a fast and ferocious tool for adapting existing command-line tools and web services for AI consumption. AI calls the tool, gets rapid confirmation and can continue to plan and analyze, getting a callback when the tool completes. Mutiple concurrent tool calls can be active with optional mutex blocking by `ahma_mcp` if needed. "Ahma" is Finnish for "wolverine."

## Overview

Ahma MCP is a Model Context Protocol server that dynamically adapts any command-line tool for asynchronous and concurrent use by AI assistants. Unlike tools designed for a single application (like `cargo`), Ahma discovers a tool's capabilities at runtime by parsing its `mytooltoadapt --help` output. A single configuration file (`tools/*.toml`) can optionally override `ahma_mcp`'s default tool availability and instructions to the calling AI.

AI can now use any command line interface (CLI) tool efficiently, queuing multiple commands and thinking productively while one or more tools execute in the background.

### Evolution from async_cargo_mcp

Ahma MCP is the next-generation successor to `async_cargo_mcp`, providing:

- **Universal CLI Adaptation**: Works with any command-line tool, not just Cargo
- **Dynamic Discovery**: Automatically parses help output to generate tool schemas
- **Multi-Tool Support**: Single server handles multiple CLI tools simultaneously
- **Enhanced Configuration**: Rich TOML-based configuration with AI hints
- **Better Performance**: Optimized MCP protocol implementation
- **Comprehensive Testing**: 76+ tests ensuring reliability

_Note: `async_cargo_mcp` is now deprecated in favor of this universal approach._

## Quick Start

The fastest way to try ahma_mcp with VS Code MCP support.

```bash
# 1) Clone and build the release binary
git clone https://github.com/paulirotta/ahma_mcp.git
cd ahma_mcp
cargo build --release

# 2) Run tests (optional but recommended)
cargo test

# 3) Create a minimal MCP config adjacent to the repo for copy/paste
cat > mcp_config_example.json << 'JSON'
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
JSON

# 4) Open the file and replace /absolute/path/to/ahma_mcp with your path
open mcp_config_example.json
```

Then copy the contents into your VS Code MCP configuration file (per-OS locations below), restart VS Code, and you’re ready.

## Key Features

- **Dynamic Tool Adaptation**: Automatically creates an MCP tool schema by inspecting a command-line tool's help documentation. No pre-configuration needed.
- **Asynchronous by Default**: Enables concurrent execution of multiple tool commands, allowing the AI to continue working without blocking.
- **Optional Synchronous Mode**: Supports a `--synchronous` flag for simpler, blocking execution when needed.
- **Unified Tool Interface**: Exposes a single, powerful MCP tool for each adapted command-line application, simplifying the AI's interaction model.
- **Customizable Tool Hints**: Provides intelligent suggestions to the AI on what to think about while waiting for slow operations to complete, and allows users to customize these hints in a simple TOML configuration file.
- **Automatic Configuration Updates**: Keeps the tool configuration file (`tools/*.toml`) up-to-date with discovered commands and options, providing a clear and current reference for users.

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

- `git.toml` - Git version control (22 subcommands)
- `cargo.toml` - Rust package manager (11 subcommands)
- `ls.toml` - File listing
- `cat.toml` - File viewing
- `grep.toml` - Text searching
- `sed.toml` - Stream editing
- `echo.toml` - Text output

To add your own tools, create a `tools/<tool_name>.toml` file:

```toml
# tools/my_tool.toml
tool_name = "my_tool"
command = "my_tool"
enabled = true
timeout_seconds = 300

[hints]
primary = "Brief description of what this tool does"
usage = "Common usage examples: my_tool --option value"
```

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
- `mcp_ahma_mcp_cargo_add` - Add dependencies
- And 8+ more cargo subcommands

**File Operations:**

- `mcp_ahma_mcp_ls_run` - List files
- `mcp_ahma_mcp_cat_run` - View file contents
- `mcp_ahma_mcp_grep_run` - Search text patterns
- `mcp_ahma_mcp_sed_run` - Edit text streams
- `mcp_ahma_mcp_echo_run` - Output text

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

### Performance: Pre-warmed shell pool

Ahma MCP includes an experimental pre-warmed shell pool inspired by `async_cargo_mcp` to reduce command startup latency. It keeps a small pool of ready shells per working directory and reuses them for subsequent operations. See `ShellPoolConfig` in `src/shell_pool.rs`.

## Creating Custom Tool Configurations

Want to add your own CLI tools? Create a TOML configuration file in the `tools/` directory:

### Basic Configuration

```toml
# tools/docker.toml
tool_name = "docker"
command = "docker"
enabled = true
timeout_seconds = 300
```

### Advanced Configuration with Hints

```toml
# tools/npm.toml
tool_name = "npm"
command = "npm"
enabled = true
timeout_seconds = 600
verbose = false

[hints]
primary = "Node.js package manager for JavaScript/TypeScript projects"
usage = "npm install, npm run build, npm test, npm publish"
wait_hint = "Consider reviewing package.json or planning next development steps"
build = "Review dependencies and build output for optimization opportunities"
test = "Analyze test results and consider additional test coverage"
default = "Use this time to plan next steps or review code"

[hints.custom]
install = "Installing dependencies - review package.json for security and updates"
audit = "Security audit running - prepare to address vulnerabilities"
publish = "Publishing package - verify version and changelog"

[hints.parameters]
"--save-dev" = "Add to development dependencies only"
"--global" = "Install package globally for system-wide access"

[overrides.test]
timeout_seconds = 900
synchronous = false
hint = "Tests running - review test output patterns and coverage"
default_args = ["--verbose"]
```

### Configuration Options

- **`tool_name`**: Name of the tool (required)
- **`command`**: Actual command to execute (defaults to `tool_name`)
- **`enabled`**: Whether to load this tool (default: `true`)
- **`timeout_seconds`**: Default timeout for operations (default: 300)
- **`verbose`**: Enable verbose logging (default: `false`)
- **`hints`**: AI guidance for different operations
- **`overrides`**: Subcommand-specific settings
- **`[hints.custom]`**: Custom hints for specific subcommands
- **`[hints.parameters]`**: Descriptions for command-line parameters

After adding new tool configurations, restart VS Code to load them.

## License

Licensed under either [Apache License 2.0](APACHE_LICENSE.txt) or [MIT License](MIT_LICENSE.txt).
