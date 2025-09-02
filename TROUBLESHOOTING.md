# AHMA MCP Troubleshooting Guide

This guide provides solutions for common issues encountered when using AHMA MCP, with particular focus on timeout handling, operation management, and development workflow optimization.

## Operation Timeout Issues

### Default Timeout Behavior

- **Default timeout**: 240 seconds (4 minutes) for wait operations
- **Timeout range**: 10 seconds (minimum) to 1800 seconds (30 minutes maximum)
- **Progress warnings**: Automatic warnings at 50%, 75%, and 90% of timeout duration
- **Remediation detection**: Automatic analysis of common timeout causes

### Common Timeout Scenarios

#### 1. Stale Lock Files

**Symptoms:**

- Operations hang indefinitely
- Build or package management operations fail to start
- Multiple processes appear to be waiting

**Detection:**
AHMA MCP automatically detects common lock file patterns:

- `Cargo.lock` (Rust projects)
- `package-lock.json` (Node.js projects)
- `yarn.lock` (Yarn package manager)
- `composer.lock` (PHP projects)
- `.cargo-lock` (Cargo process locks)
- `.lock` (generic lock files)

**Remediation Steps:**

1. Check for stale lock files: `find . -name "*.lock" -o -name ".cargo-lock" -o -name ".lock"`
2. Identify blocking processes: `lsof [lockfile_path]`
3. Safely terminate blocking processes or wait for completion
4. Remove stale lock files only if processes have terminated
5. Retry the operation

#### 2. Network Connectivity Issues

**Symptoms:**

- Package downloads timeout
- Git operations hang
- Repository access fails

**Detection:**
AHMA MCP identifies network-related operations and suggests connectivity checks.

**Remediation Steps:**

1. Test basic connectivity: `ping 8.8.8.8`
2. Check DNS resolution: `nslookup google.com`
3. Test HTTP/HTTPS access: `curl -I https://github.com`
4. For package managers, try offline mode:
   - Cargo: `cargo build --offline`
   - npm: `npm install --offline`
   - pip: `pip install --no-index`
5. Configure proxy settings if behind corporate firewall

#### 3. Resource Exhaustion

**Symptoms:**

- Operations slow down significantly
- System becomes unresponsive
- Build processes consume excessive resources

**Detection:**
AHMA MCP suggests resource monitoring for build operations.

**Remediation Steps:**

1. Monitor system resources: `top` or `htop`
2. Check disk space: `df -h .`
3. Monitor memory usage: `free -h` (Linux) or Activity Monitor (macOS)
4. Reduce concurrent jobs:
   - Cargo: `cargo build -j 1`
   - Make: `make -j1`
5. Close unnecessary applications

#### 4. Process Conflicts

**Symptoms:**

- Operations report "resource busy" errors
- Multiple instances of the same tool running
- File access conflicts

**Detection:**
AHMA MCP suggests checking for competing processes.

**Remediation Steps:**

1. Check for competing processes: `ps aux | grep [command_name]`
2. Identify file locks: `lsof | grep [working_directory]`
3. Terminate competing processes safely
4. Wait for exclusive file access
5. Retry with proper sequencing

### Advanced Timeout Configuration

#### Custom Timeout Examples

```bash
# Short timeout for quick operations
wait --timeout_seconds 30

# Extended timeout for complex builds
wait --timeout_seconds 900  # 15 minutes

# Tool-specific waiting
wait --tools cargo --timeout_seconds 600

# Status monitoring without timeout
status --tools cargo,npm
```

#### Progressive Warning Timeline

For a 240-second (4-minute) default timeout:

- **2 minutes (50%)**: First warning with current operation status
- **3 minutes (75%)**: Second warning with remediation suggestions
- **3.6 minutes (90%)**: Final warning with specific action items
- **4 minutes (100%)**: Timeout with detailed error report and partial results

## Development Workflow Issues

### Graceful Shutdown During Development

#### File Change Interruptions

**Problem:** Ongoing operations get abruptly terminated when files change during development.

**Solution:** AHMA MCP includes graceful shutdown handling:

1. **Signal Detection**: Automatically detects SIGTERM/SIGINT signals from cargo watch and file watchers
2. **Grace Period**: Provides 10-second window for operations to complete naturally
3. **Progress Feedback**: Shows operation completion status during shutdown
4. **Result Delivery**: Ensures completed operations deliver results before shutdown

#### Recommended Development Workflow

1. **Start long-running operations** (builds, tests) and continue working
2. **Monitor progress** with `status` tool without blocking
3. **Make file changes** - ongoing operations complete gracefully
4. **Receive automatic notifications** when operations finish
5. **Use wait tool sparingly** - only when you cannot proceed without results

### VS Code Integration Issues

#### MCP Connection Problems

**Symptoms:**

- Tools not appearing in VS Code
- "execution_failed" errors
- MCP server not starting

**Solutions:**

1. **Verify Binary Path**

   ```bash
   # Check binary exists and is executable
   ls -la /path/to/ahma_mcp/target/release/ahma_mcp
   chmod +x /path/to/ahma_mcp/target/release/ahma_mcp
   ```

2. **Validate MCP Configuration**

   - Use absolute paths (no `~` or environment variables)
   - Ensure tools directory exists: `ls -la /path/to/ahma_mcp/tools/`
   - Restart VS Code completely after configuration changes

3. **Check Developer Tools**
   - Open: Help → Toggle Developer Tools
   - Look for MCP-related errors in console
   - Check for path resolution issues

#### Performance Issues

**Symptoms:**

- Slow tool responses
- High CPU usage
- Delayed command execution

**Solutions:**

1. **Use Release Binary**

   ```bash
   # Always build and use release version
   cargo build --release
   # Use direct binary path, not cargo run
   ```

2. **Optimize Shell Pool**

   - Pre-warmed shells provide 10x performance improvement
   - Shells are reused within working directories
   - Pool automatically manages lifecycle

3. **Monitor Resource Usage**
   ```bash
   # Check if AHMA MCP is consuming excessive resources
   ps aux | grep ahma_mcp
   top -p $(pgrep ahma_mcp)
   ```

## Operation Management Best Practices

### Efficient Operation Monitoring

#### Use Status for Real-time Monitoring

```bash
# Check all operations
status

# Monitor specific operations
status --operation_id op_123

# Filter by tool type
status --tools cargo,npm
```

#### Strategic Wait Usage

**When to use wait:**

- ✅ Critical path operations that block further progress
- ✅ Final validation before deployment/submission
- ✅ Coordination between dependent operations

**When NOT to use wait:**

- ❌ Routine builds that can run in background
- ❌ Tests that don't affect current work
- ❌ Documentation generation
- ❌ Non-blocking operations

#### Concurrent Operation Patterns

**Effective Patterns:**

```bash
# Start multiple operations
cargo build        # Background build
cargo test         # Background tests
cargo doc --no-deps # Background docs

# Continue productive work while they run
# Edit code, review documentation, plan features

# Check status periodically
status --tools cargo

# Only wait if results are needed for next steps
wait --tools cargo --timeout_seconds 300
```

### Error Recovery Strategies

#### Partial Operation Recovery

When timeouts occur, AHMA MCP provides:

- Results from completed operations
- Detailed status of failed operations
- Specific remediation suggestions
- Operation history for debugging

#### Retry Strategies

1. **Immediate Retry**: For transient network issues
2. **Resource Cleanup**: Remove lock files, restart processes
3. **Reduced Scope**: Build specific packages instead of workspace
4. **Offline Mode**: Use cached dependencies when available
5. **Sequential Execution**: Reduce concurrency for resource conflicts

## Performance Optimization

### Shell Pool Configuration

The shell pool provides significant performance improvements:

- **Startup time**: 5-20ms vs 50-200ms (10x improvement)
- **Resource efficiency**: Shared shells per working directory
- **Automatic lifecycle**: Shells timeout after inactivity
- **Health monitoring**: Automatic restart of failed shells

### Concurrency Management

**Optimal Concurrency Patterns:**

- Start multiple independent operations
- Use status monitoring instead of blocking waits
- Let AHMA MCP handle coordination automatically
- Focus on productive work while operations run

**Resource-Aware Scaling:**

- Monitor system resources during concurrent operations
- Adjust parallel job counts based on available resources
- Use tool-specific concurrency limits when needed

## Diagnostic Commands

### System Health Check

```bash
# Verify AHMA MCP installation
./target/release/ahma_mcp --help

# Test basic functionality
./target/release/ahma_mcp --tools-dir tools &
echo '{"method":"tools/list"}' | nc localhost 3000

# Check system resources
df -h .                    # Disk space
free -h                    # Memory (Linux)
top -l 1 | head -20       # CPU/Memory (macOS)
```

### Operation Debugging

```bash
# Detailed operation status
status --operation_id op_123

# Wait with verbose feedback
wait --timeout_seconds 60 --tools cargo

# Monitor active processes
ps aux | grep -E "(cargo|npm|node)"
```

### Network Diagnostics

```bash
# Basic connectivity
ping -c 3 8.8.8.8

# DNS resolution
nslookup crates.io
nslookup registry.npmjs.org

# HTTP access
curl -I https://crates.io
curl -I https://registry.npmjs.org
```

This troubleshooting guide should be referenced whenever operations timeout, performance issues occur, or development workflows are interrupted. The key is to use AHMA MCP's built-in monitoring and remediation features rather than manual intervention.
