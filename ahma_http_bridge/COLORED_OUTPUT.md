# Colored Terminal Output

The HTTP bridge supports colored terminal output for debugging purposes. This feature is automatically enabled when running in debug mode (non-release builds).

## How It Works

When the bridge detects it's running a debug build of the MCP server, it will echo all STDIN, STDOUT, and STDERR communications to the terminal with color coding:

- **STDIN** (messages sent to the MCP server): **Cyan** with `→` prefix
- **STDOUT** (responses from the MCP server): **Green** with `←` prefix  
- **STDERR** (error messages): **Red** with `⚠` prefix

## Enabling/Disabling

### Automatic Detection

The feature is automatically enabled when:

1. The `ahma_http_bridge` binary detects a local debug binary at `target/debug/ahma_mcp`
2. The `ahma_shell` (ahma_mcp) binary is built with `cargo build` (debug mode)

### Manual Control

You can manually control colored output by setting the `enable_colored_output` field in `BridgeConfig`:

```rust
use ahma_http_bridge::{BridgeConfig, start_bridge};

let config = BridgeConfig {
    bind_addr: "127.0.0.1:3000".parse().unwrap(),
    server_command: "ahma_mcp".to_string(),
    server_args: vec!["--log-to-stderr".to_string()],
    enable_colored_output: true, // Enable colored output
};

start_bridge(config).await?;
```

## Release Builds

In release builds (`cargo build --release`), colored output is **disabled by default** to avoid cluttering production logs and to maintain maximum performance.

## Example Output

When running the MCP Inspector with debug mode:

```
→ STDIN: {"jsonrpc":"2.0","method":"initialize","id":1}
← STDOUT: {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05"}}
⚠ STDERR: [WARN] Some warning message
```

## Testing

To test the colored output feature, run:

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

Then interact with the MCP server through the web interface or HTTP requests.
