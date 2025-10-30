# Project Requirements

This document outlines the requirements for the project and for the AI assistant working on it.

## Part 1: AI Development Requirements

### 1.1. Core Functionality

* The system must run from the command line with command line help and up-to-date version information.
* Error messages must be clear and informative, checking for common issues and suggesting solutions before beginning work.

### 1.2. Technical Stack

* **Language:** Rust stable
* **Libraries:** select from the `crates.io` registry, prioritizing well-maintained and widely-used crates.

### 1.3. Local Services & QA

* **MCP:** Always prefer and use `ahma_mcp` for supported actions including command line requests.
* All code **must** be validated as defined in `.vscode/tasks.json` under the label "Rust Quality Check".
* If ahma_mcp is available, you **must** spawn all requests one by one in the sequence specified, ending with the command 'await'.

### 1.4. AI Development Rules

* You **must** adhere to all requirements in this document.
* You **must** write tests for all new functionality before implementation (TDD).
* Test coverage for new code **must** be at least 90%.
* You **must** not use any deprecated or obsolete libraries or techniques.
* When I give you a new request, you must first cross-reference it with this file.
* If my request implies a **change, addition, or conflict** with these requirements, you must **stop and ask for confirmation** of your solution to update this file *before* writing any code.
* After generating code, your final step is to automatically run the "Run Quality Check (ahma_mcp)" task.

---

## Part 2: Application Functional Requirements

### 2.1. Project Goal

The `ahma_mcp` (AI-Human Master Control Program) project is a command-line tool designed to act as a secure and efficient intermediary between a large language model (LLM) AI assistant and a local development environment. Its primary goal is to orchestrate development tasks, manage the project workspace, and enforce quality standards, enabling safe and effective AI-driven software engineering.

### 2.2. Data Sources

* **Local File System:** The primary data source is the project's directory on the local file system. The tool will read, write, create, and delete files and directories as instructed.
* **Project Configuration:** Reads project-specific configuration from files like `requirements.md`, `Cargo.toml`, and `.vscode/tasks.json` to maintain context and enforce rules.
* **Standard I/O:** Receives commands and instructions from the AI assistant via standard input (stdin) and returns results, logs, and errors via standard output (stdout) and standard error (stderr).

### 2.3. Core Functionality

* **Task Queue Management:** Maintain a queue of commands received from the AI. Commands are executed sequentially upon receiving an `await` command.
* **File System Operations:** Provide a safe subset of file system commands (`read`, `write`, `list`, `create`, `delete`) that are restricted to the project's root directory.
* **Command Execution:** Execute shell commands for tasks like building, testing, and linting. This includes running predefined quality checks.
* **Contextual Awareness:** Provide the AI with information about the project structure, file contents, and task outcomes.
* **Self-Update Capability:** The tool must be able to participate in its own update process, guided by the AI, by applying changes to its source code and triggering a rebuild.

### 2.4. Command-Line Options

The `ahma_mcp` tool will support the following command-line interface:

```sh
ahma_mcp [SUBCOMMAND]

SUBCOMMANDS:
  read <PATH>              # Reads the content of a file and prints it to stdout.
  write <PATH>             # Writes content from stdin to a file.
  list [PATH]              # Lists the contents of a directory. Defaults to project root.
  run <COMMAND> [ARGS...]  # Executes a shell command and its arguments.
  check                    # Runs the predefined "Rust Quality Check" task.
  await                    # Executes all queued commands in sequence.
  help                     # Prints this message or the help of the given subcommand(s).
```

### 2.5. Command-Line Utilities

* **Schema Generation:** The tool must support a `generate_schema` command to output JSON schemas for tool configurations.

### 2.6. Quality Assurance Standards

* **Test Coverage:** All new code must maintain at least 90% test coverage.
* **Code Quality:** All code must pass without warnings:
  * `cargo fmt` - Code formatting
  * `cargo clippy` - Linting with auto-fix
  * `cargo test` (or `cargo nextest run`) - All tests passing
  * `cargo build` - Clean compilation

### 2.7. MCP Tool Usage Requirements

**CRITICAL:** AI assistants working with this project **MUST ALWAYS** use ahma_mcp MCP tools for ALL cargo operations.

**Prohibited:** NEVER use `run_in_terminal` or direct shell commands for: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`, `cargo check`, or any other cargo operations.

**Required MCP Tools:**

* `mcp_ahma_mcp_cargo` - For build, check, and other cargo commands
* `mcp_ahma_mcp_cargo_clippy` - For linting with clippy
* `mcp_ahma_mcp_cargo_nextest` - For running tests with nextest
* `mcp_ahma_mcp_cargo_fmt` - For code formatting
* `mcp_ahma_mcp_rust_quality_check` - For comprehensive quality checks (format, lint, test, build in sequence)

**Rationale:** Using MCP tools ensures:

* Consistent command execution across development environments
* Proper integration with the MCP protocol
* Self-hosting capability (the tool managing itself)
* Better error handling and progress reporting

**Error Handling:** If MCP tools fail, debug the MCP server issue - don't fall back to terminal commands. This maintains consistency and helps identify configuration or connectivity problems.
