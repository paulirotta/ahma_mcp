# AHMA MCP Development Workflow Guide

This guide outlines recommended development practices when using AHMA MCP, focusing on efficient concurrent operations, graceful shutdown handling, and productive development patterns.

## Core Development Philosophy

AHMA MCP is designed around **async-first, non-blocking workflows** that enable maximum productivity through intelligent concurrency and graceful operation management.

### Key Principles

1. **Start operations and continue working** - Don't wait for results unless absolutely necessary
2. **Use status monitoring** - Check progress without blocking your workflow
3. **Trust graceful shutdown** - File changes won't abruptly terminate operations
4. **Monitor, don't wait** - Use status tool instead of wait tool for productivity

## Optimal Development Workflow

### 1. Project Setup Phase

When starting work on a project, initiate all foundational operations concurrently:

```bash
# Start comprehensive project analysis (all async)
cargo build --release    # Background build
cargo test               # Background test suite
cargo doc --no-deps      # Background documentation
cargo clippy            # Background linting

# Continue with productive work while operations run
# - Review code structure
# - Plan feature implementation
# - Update documentation
# - Analyze dependencies
```

### 2. Active Development Phase

During active development, leverage graceful shutdown for seamless workflow:

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

Use status monitoring for real-time awareness:

```bash
# Check all active operations
status

# Monitor specific operation types
status --tools cargo

# Check specific operation progress
status --operation_id op_123
```

### 4. Strategic Waiting

Use wait tool only when results are critical path:

```bash
# ✅ Good use cases for wait:
wait --tools cargo --timeout_seconds 120    # Before deployment
wait --operation_id op_build --timeout_seconds 60  # Before dependent task

# ❌ Avoid waiting for:
wait --tools cargo                           # Routine builds
wait --timeout_seconds 300                   # Long timeouts that block work
```

## Advanced Development Patterns

### Concurrent Feature Development

**Pattern: Parallel Feature Validation**

```bash
# Start comprehensive validation suite
cargo build --all-features     # Feature compatibility
cargo test --all-targets       # Comprehensive testing
cargo clippy --all-targets     # Complete linting
cargo doc --all-features       # Full documentation

# Work on implementation while validation runs
# Monitor progress with: status --tools cargo
# Only wait if deployment/PR submission is next step
```

### Code Quality Workflow

**Pattern: Continuous Quality Monitoring**

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

**Pattern: Layered Test Execution**

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

```
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
status    # Shows concurrency analysis and wait time metrics

# Example output:
# Operations status: 2 active, 5 completed (total: 7)
# Concurrency Analysis:
# ✅ High concurrency efficiency: 5.2% of execution time spent waiting
```

### Resource-Aware Concurrency

**Adaptive Concurrency Patterns:**

```bash
# High-resource operations: limit concurrency
cargo build -j 2                    # Limit parallel jobs
cargo test --test-threads 2         # Limit test parallelism

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
wait --timeout_seconds 30 --tools cargo_check

# Standard development operations
wait --timeout_seconds 240 --tools cargo

# Complex release operations
wait --timeout_seconds 900 --tools cargo_build,cargo_test
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
   wait --timeout_seconds 60 --tools cargo # Short timeout for debugging
   ```

2. **Resource Conflicts:**

   ```bash
   ps aux | grep cargo                     # Check competing processes
   lsof | grep $(pwd)                      # Check file locks
   ```

3. **Network Issues:**
   ```bash
   cargo build --offline                   # Use offline mode
   ping 8.8.8.8                          # Test connectivity
   ```

## Best Practices Summary

### ✅ Recommended Patterns

1. **Async-First Development:**

   - Start operations and continue working
   - Use status for monitoring, not wait for blocking
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

# Only wait if validation is required for commit
wait --tools cargo --timeout_seconds 120
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

This development workflow maximizes productivity by leveraging AHMA MCP's async-first architecture, graceful shutdown capabilities, and intelligent operation management. The key is to embrace concurrent operations while maintaining awareness through status monitoring rather than blocking waits.
