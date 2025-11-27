# Colored Terminal Output Feature

## Summary

Added colored terminal output for the HTTP bridge when running in debug mode (non-release builds). This feature helps developers debug MCP server communication by color-coding STDIN, STDOUT, and STDERR streams.

## Changes Made

### 1. Added `owo-colors` Dependency

**File:** `ahma_http_bridge/Cargo.toml`

Added the `owo-colors` crate for terminal color support:

```toml
owo-colors = "4.1.0"
```

### 2. Extended `BridgeConfig`

**File:** `ahma_http_bridge/src/bridge.rs`

Added a new configuration field:

```rust
pub struct BridgeConfig {
    pub bind_addr: SocketAddr,
    pub server_command: String,
    pub server_args: Vec<String>,
    pub enable_colored_output: bool,  // NEW
}
```

### 3. Modified Process Management

**File:** `ahma_http_bridge/src/bridge.rs`

Updated the `manage_process` function to:

- Capture STDERR when colored output is enabled (instead of inheriting)
- Echo STDIN messages in **cyan** with `→` prefix
- Echo STDOUT messages in **green** with `←` prefix
- Echo STDERR messages in **red** with `⚠` prefix

### 4. Auto-Detection in Standalone Bridge

**File:** `ahma_http_bridge/src/main.rs`

The standalone HTTP bridge binary automatically enables colored output when it detects a local debug binary:

```rust
if let Some(local_binary) = detect_local_debug_binary(&cwd) {
    config.server_command = local_binary;
    config.enable_colored_output = true;  // Auto-enable in debug mode
}
```

### 5. Debug Mode Detection in ahma_mcp

**File:** `ahma_shell/src/main.rs`

The main `ahma_mcp` binary enables colored output when built in debug mode:

```rust
let enable_colored_output = cfg!(debug_assertions);
```

### 6. Updated MCP Inspector Script

**File:** `scripts/ahma-inspector.sh`

Updated the script to:

- Correctly indicate it builds in debug mode (not release)
- Document the colored output feature in the help text

## Color Scheme

| Stream | Color | Prefix | Description |
|--------|-------|--------|-------------|
| STDIN  | Cyan  | `→`    | Messages sent to the MCP server |
| STDOUT | Green | `←`    | Responses from the MCP server |
| STDERR | Red   | `⚠`    | Error messages from the MCP server |

## Usage

### Automatic (Recommended)

Simply build and run in debug mode:

```bash
# Build in debug mode
cargo build

# Run the MCP Inspector
./scripts/ahma-inspector.sh
```

Or run the HTTP bridge directly:

```bash
./target/debug/ahma_http_bridge
```

### Manual Control

You can manually enable/disable colored output:

```rust
use ahma_http_bridge::{BridgeConfig, start_bridge};

let config = BridgeConfig {
    bind_addr: "127.0.0.1:3000".parse().unwrap(),
    server_command: "ahma_mcp".to_string(),
    server_args: vec![],
    enable_colored_output: true,  // Manually enable
};

start_bridge(config).await?;
```

## Example Output

When running with colored output enabled:

```
→ STDIN: {"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}
← STDOUT: {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{}}}
→ STDIN: {"jsonrpc":"2.0","method":"tools/list","id":2}
← STDOUT: {"jsonrpc":"2.0","id":2,"result":{"tools":[...]}}
⚠ STDERR: [WARN] Some warning message from the server
```

## Release Builds

In release builds (`cargo build --release`), colored output is **disabled by default** to:

- Avoid cluttering production logs
- Maintain maximum performance
- Prevent ANSI escape codes in log files

## Testing

All existing tests pass with the new feature:

```bash
cargo test --package ahma_http_bridge --lib
```

Result: **27 tests passed**

## Benefits

1. **Better Debugging**: Easily distinguish between requests, responses, and errors
2. **Visual Clarity**: Color coding makes it easier to follow the conversation flow
3. **Non-Intrusive**: Only enabled in debug mode, no impact on production
4. **Automatic**: No manual configuration needed for typical development workflow

## Documentation

See `ahma_http_bridge/COLORED_OUTPUT.md` for detailed documentation.
