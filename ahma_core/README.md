# Ahma Core

Core library for the Ahma MCP server, providing tool execution, configuration management, and async orchestration.

## Overview

`ahma_core` is the foundational library that powers the Ahma MCP server. It handles:

- **Tool Execution**: Synchronous and asynchronous command execution with timeout management
- **MCP Service**: Complete Model Context Protocol server implementation
- **Configuration**: Tool definitions via JSON files
- **Async Orchestration**: Background operation management with status tracking
- **Shell Pool**: Efficient subprocess management for command execution

## Features

- **Sync-First with Async Override**: Tools execute synchronously by default; mark long-running operations as async for parallel execution
- **Tool Sequencing**: Chain multiple commands into powerful workflows
- **Safe Scoping**: Commands are safely scoped to project working directories
- **Operation Monitoring**: Track async operations with unique IDs and status queries
- **Flexible Configuration**: Define tools with JSON schemas, no recompilation needed

## Architecture

The crate is organized into several key modules:

- `mcp_service.rs` - MCP protocol server implementation
- `adapter.rs` - Tool execution and command orchestration
- `config.rs` - Tool configuration and JSON schema management
- `operation_monitor.rs` - Async operation tracking and status
- `shell_pool.rs` - Subprocess management and pooling
- `callback_system.rs` - Event notification system

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
ahma_core = { path = "../ahma_core" }
```

Basic example:

```rust
use ahma_core::{McpService, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load("./tools")?;
    let service = McpService::new(config);
    
    // Use with rmcp transport
    // service.serve(transport).await?;
    
    Ok(())
}
```

## License

MIT OR Apache-2.0

