# AHMA MCP Usage Guide

This guide outlines recommended practices for using AHMA MCP, focusing on efficient concurrent operations, graceful shutdown handling, and productive usage patterns.

## Core Philosophy

AHMA MCP is designed around **async-first, non-blocking workflows** that enable maximum productivity through intelligent concurrency and graceful operation management.

### Key Principles

1. **Start operations and continue working** - Don't `await` for results unless absolutely necessary.
2. **Use status monitoring** - Check progress without blocking your workflow.
3. **Trust graceful shutdown** - File changes won't abruptly terminate long-running operations.
4. **Monitor, don't await** - Use the `status` tool instead of the `await` tool for better productivity.

## Synchronous Setting Inheritance

The execution mode (sync vs async) follows a clear inheritance hierarchy:

```text
┌─────────────────────────────────────────────────────────────────┐
│                    EXECUTION MODE RESOLUTION                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. CLI Flag (highest priority)                                  │
│     └── --sync flag forces ALL tools to run synchronously        │
│                                                                  │
│  2. Subcommand Config                                            │
│     └── "synchronous": true/false in subcommand definition       │
│         Overrides tool-level setting for this subcommand         │
│                                                                  │
│  3. Tool Config                                                  │
│     └── "synchronous": true/false at tool level                  │
│         Inherited by subcommands that don't specify their own    │
│                                                                  │
│  4. Default (lowest priority)                                    │
│     └── ASYNC - operations run in background by default          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

                    Resolution Flow
                    ═══════════════

   ┌──────────┐     ┌───────────────┐     ┌─────────────┐
   │ --sync   │ ──▶ │ Subcommand    │ ──▶ │ Tool        │ ──▶ ASYNC
   │ flag?    │ No  │ synchronous?  │ No  │ synchronous?│ No  (default)
   └──────────┘     └───────────────┘     └─────────────┘
        │ Yes              │ Yes                │ Yes
        ▼                  ▼                    ▼
      SYNC           Value from           Value from
                     subcommand           tool config
```

### Examples

**Tool with mixed sync/async subcommands:**

```json
{
  "name": "cargo",
  "synchronous": false, // Tool default: async
  "subcommand": [
    { "name": "build" }, // Inherits async from tool
    { "name": "add", "synchronous": true }, // Override: sync (config changes)
    { "name": "check" } // Inherits async from tool
  ]
}
```

**Force all sync with CLI flag:**

```bash
ahma_mcp --sync --tools-dir .ahma/tools
# All operations run synchronously regardless of config
```

## Optimal Usage Workflow

### 1. Project Setup Phase

When starting work on a project, initiate all foundational operations concurrently:

```bash
# Start comprehensive project analysis (all async)
cargo build --release    # Background build
cargo test               # Background test suite
cargo doc --no-deps      # Background documentation
cargo clippy             # Background linting

# Continue with productive work while operations run
# - Review code structure
# - Plan feature implementation
# - Update documentation
# - Analyze dependencies
```

### 2. Active Development Phase

During active development, leverage graceful shutdown for a seamless workflow. When you save a file, `cargo-watch` will trigger a restart, but AHMA MCP ensures a graceful shutdown.

```bash
# Start relevant operations
cargo check             # Quick syntax/type checking
cargo test -- unit      # Specific test subset

# Make file changes freely
# AHMA MCP handles graceful shutdown:
# ✓ File changes detected
# ✓ 10-second grace period provided
# ✓ Operations complete naturally
# ✓ Results delivered automatically
# ✓ New operations start with updated code
```

### 3. Monitoring and Coordination

Use status monitoring for real-time awareness of background tasks.

```bash
# Check all active operations
status

# Monitor specific operation types
status --tools cargo

# Check specific operation progress
status --operation_id op_123
```

### 4. Strategic Waiting

Use the `await` tool only when results are critical for your next step.

```bash
# ✅ Good use cases for await:
await --tools cargo --timeout_seconds 120    # Before deployment
await --operation_id op_build --timeout_seconds 60  # Before a dependent task

# ❌ Avoid waiting for:
await --tools cargo                           # Routine builds
await --timeout_seconds 300                   # Long timeouts that block work
```

## Advanced Development Patterns

### Concurrent Feature Development

#### Pattern: Parallel Feature Validation

```bash
# Start comprehensive validation suite
cargo build --all-features     # Feature compatibility
cargo test --all-targets       # Comprehensive testing
cargo clippy --all-targets     # Complete linting
cargo doc --all-features       # Full documentation

# Work on implementation while validation runs
# Monitor progress with: status --tools cargo
# Only await if deployment/PR submission is next step
```

### Code Quality Workflow

#### Pattern: Continuous Quality Monitoring

```bash
# Establish quality baseline
cargo fmt --check              # Format checking
cargo clippy -- -W clippy::all # Enhanced linting
cargo audit                    # Security scanning

# Continue development with real-time feedback
# Operations complete in background
# Results automatically delivered when ready
```

### Testing Strategy

#### Pattern: Layered Test Execution

```bash
# Layer 1: Fast feedback loop
cargo test --lib                # Unit tests
cargo check                    # Type checking

# Layer 2: Comprehensive validation
cargo test --all-targets       # Integration tests
cargo test --release           # Release mode testing

# Layer 3: Extended validation
cargo test --ignored           # Expensive tests
cargo bench                    # Performance benchmarks
```

## Graceful Shutdown Patterns

### File Change Handling

AHMA MCP provides sophisticated graceful shutdown during development:

#### Automatic Grace Period

- **Detection**: SIGTERM/SIGINT signals from cargo watch, file watchers
- **Grace Period**: 10-second window for natural completion
- **Progress Monitoring**: Real-time feedback during shutdown sequence
- **Result Delivery**: Completed operations deliver results before shutdown

#### Visual Feedback During Shutdown

```sh
🔄 Graceful shutdown initiated...
⏳ Allowing 10 seconds for operations to complete...
📊 2 operations running: cargo_build, cargo_test
✅ cargo_build completed successfully (3.2s)
⏳ cargo_test still running... (7.8s remaining)
✅ cargo_test completed successfully (9.1s)
🏁 All operations completed. Shutting down gracefully.
```

### Development Server Integration

**cargo watch Integration:**

```bash
# In terminal 1: Start development server with graceful shutdown
cargo watch -x 'run --bin server'

# In terminal 2: Work with AHMA MCP
cargo build &             # Background build
cargo test &              # Background tests

# Make changes to source files
# Operations complete gracefully before server restart
# No abrupt termination of ongoing work
```

**VS Code Integration:**

- File changes trigger automatic recompilation
- AHMA MCP operations complete before restart
- No manual coordination needed
- Seamless development experience

## Performance Optimization Patterns

### Shell Pool Utilization

AHMA MCP's pre-warmed shell pool provides 10x performance improvement:

**Optimal Usage:**

- Operations in same directory reuse shells (5-20ms startup)
- Shells automatically managed and cleaned up
- No manual shell management required

**Performance Monitoring:**

```bash
# Check shell pool efficiency
status    # Shows concurrency analysis and await time metrics

# Example output:
# Operations status: 2 active, 5 completed (total: 7)
# Concurrency Analysis:
# ✅ High concurrency efficiency: 5.2% of execution time spent waiting
```

### Resource-Aware Concurrency

**Adaptive Concurrency Patterns:**

```bash
# High-resource operations: limit concurrency
cargo test --test-threads 4         # Limit test parallelism

# Low-resource operations: maximize concurrency
cargo check &                       # Multiple concurrent checks
cargo clippy &
cargo fmt --check &
```

### Timeout Strategy Optimization

**Progressive Timeout Management:**

- **Default (240s)**: Sufficient for most operations
- **Short (30-60s)**: Quick validation operations
- **Extended (900-1800s)**: Complex builds, comprehensive test suites

**Timeout Configuration Examples:**

```bash
# Quick feedback operations
await --timeout_seconds 30 --tools cargo_check

# Standard development operations
await --timeout_seconds 240 --tools cargo

# Complex release operations
await --timeout_seconds 900 --tools cargo_build,cargo_test
```

## Error Recovery and Debugging

### Timeout Handling

AHMA MCP provides intelligent timeout remediation:

**Automatic Detection:**

- Stale lock files (Cargo.lock, package-lock.json, etc.)
- Network connectivity issues
- Resource exhaustion
- Process conflicts

**Progressive Warning System:**

- 50% timeout: Status update with current progress
- 75% timeout: Warning with remediation suggestions
- 90% timeout: Final warning with specific actions
- 100% timeout: Detailed error report with partial results

### Debugging Workflow Issues

**Common Issues and Solutions:**

1. **Operations Not Completing:**

   ```bash
   status --tools cargo                    # Check operation status
   await --timeout_seconds 60 --tools cargo # Short timeout for debugging
   ```

2. **Resource Conflicts:**

   ````bash
   ps aux | grep cargo                     # Check competing processes
   lsof | grep $(pwd)                      # Check file locks
   ```3. **Network Issues:**

   ```bash
   cargo build --offline                   # Use offline mode
   ping 8.8.8.8                          # Test connectivity
   ````

## Best Practices Summary

### ✅ Recommended Patterns

1. **Async-First Development:**

   - Start operations and continue working
   - Use status for monitoring, not await for blocking
   - Trust graceful shutdown during file changes

2. **Strategic Operation Management:**

   - Group related operations for concurrent execution
   - Use appropriate timeouts based on operation complexity
   - Monitor progress with status rather than blocking waits

3. **Resource-Aware Execution:**
   - Monitor system resources during concurrent operations
   - Adjust parallelism based on available resources
   - Use shell pool efficiency feedback for optimization

### ❌ Anti-Patterns to Avoid

1. **Blocking Workflows:**

   - Waiting for routine builds that don't block progress
   - Using long timeouts that prevent productive work
   - Manual coordination instead of trusting graceful shutdown

2. **Resource Waste:**

   - Running excessive concurrent operations on limited resources
   - Ignoring timeout warnings and remediation suggestions
   - Not utilizing status monitoring for optimization

3. **Manual Process Management:**
   - Manual shell spawning instead of using shell pool
   - Manual cleanup instead of trusting automatic lifecycle management
   - Manual timeout calculation instead of using progressive warnings

## Multi-Step Workflows (Preferred)

In this repo, the preferred way to run multi-step workflows is to execute a single shell pipeline via `sandboxed_shell`.

Most other tool configs in `.ahma/tools/` (especially those marked `"enabled": false`) are considered deprecated.

### Rust Quality Check

Run a comprehensive Rust quality pipeline via `sandboxed_shell`:

```bash
ahma_mcp sandboxed_shell --working_directory . -- \
   "cargo fmt --all && cargo clippy --all-targets && cargo test"
```

If you use `nextest`, replace the test step with:

```bash
ahma_mcp sandboxed_shell --working_directory . -- \
   "cargo fmt --all && cargo clippy --all-targets && cargo nextest run"
```

## Integration with Development Tools

### Git Workflow Integration

```bash
# Pre-commit validation (all async)
cargo fmt --check &
cargo clippy -- -D warnings &
cargo test &

# Continue with git operations
git add .
git status

# Only await if validation is required for commit
await --tools cargo --timeout_seconds 120
git commit -m "feature: implement new functionality"
```

### CI/CD Pipeline Preparation

```bash
# Simulate CI environment locally
cargo build --release &
cargo test --release &
cargo clippy --all-targets --all-features -- -D warnings &
cargo audit &

# Prepare deployment while validation runs
# Monitor with: status --tools cargo
# Wait only for final validation before deployment
```

### IDE Integration Optimization

**VS Code + AHMA MCP:**

- Configure .vscode/mcp.json with absolute paths
- Use release binary for optimal performance
- Enable graceful shutdown for seamless file change handling
- Monitor operations through GitHub Copilot interface

## HTTP Mode Configuration

### Sandbox Scope in HTTP Mode

When running AHMA MCP in HTTP mode, the sandbox scope is bound **per client session** via the MCP `roots/list` protocol.
Each IDE connection (session) gets its own subprocess and its own immutable sandbox scope.

#### Default Scope Priority (fallback only)

1. **Command-line argument** (highest priority): `--sandbox-scope /path/to/project`
2. **Environment variable**: `AHMA_SANDBOX_SCOPE=/path/to/project`
3. **Current working directory** (default): Where you run the server from

#### Starting the HTTP Server

**Using the script (recommended):**

```bash
# From your project directory - sandbox is set to current directory
cd /path/to/your/project
/path/to/ahma_mcp/scripts/ahma-http-server.sh

# Or specify the project directory explicitly
/path/to/ahma_mcp/scripts/ahma-http-server.sh /path/to/your/project

# Or via environment variable
export AHMA_SANDBOX_SCOPE=/path/to/your/project
/path/to/ahma_mcp/scripts/ahma-http-server.sh
```

**Using the binary directly:**

```bash
# Explicit sandbox scope (recommended for scripts)
ahma_mcp --mode http --sandbox-scope /path/to/your/project

# Via environment variable
export AHMA_SANDBOX_SCOPE=/path/to/your/project
ahma_mcp --mode http

# From project directory (uses $PWD as sandbox)
cd /path/to/your/project
ahma_mcp --mode http
```

#### Multi-Project Development

Multi-project usage is supported via per-session isolation: a single HTTP server can safely serve multiple IDE workspaces at once.

#### Security Considerations

- **Local development only**: HTTP mode is designed for local development. Do not expose the HTTP server to untrusted networks.
- **Immutable sandbox**: Once set for a session, the sandbox scope cannot be changed. This prevents the AI from escaping the sandbox.
- **Per-session isolation**: Each client session gets its own subprocess and sandbox scope derived from `roots/list`.

### STDIO Mode vs HTTP Mode

| Feature               | STDIO Mode                           | HTTP Mode                       |
| --------------------- | ------------------------------------ | ------------------------------- |
| Sandbox scope         | Set by IDE via `cwd` in mcp.json     | Set per session via `roots/list`|
| Per-project isolation | Automatic (IDE spawns per workspace) | Automatic (per session).        |
| Configuration         | `mcp.json` in IDE                    | CLI args or env vars            |
| Use case              | Standard IDE integration             | Debugging, advanced setups      |

For most users, **STDIO mode is recommended** because the IDE automatically handles sandbox configuration via the `cwd` setting in `mcp.json`.

This development workflow maximizes productivity by leveraging AHMA MCP's async-first architecture, graceful shutdown capabilities, and intelligent operation management. The key is to embrace concurrent operations while maintaining awareness through status monitoring rather than blocking waits.
