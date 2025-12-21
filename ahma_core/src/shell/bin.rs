// Binary entry point for ahma_mcp
// This is a thin wrapper that delegates to the library implementation

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    ahma_core::shell::run().await
}
