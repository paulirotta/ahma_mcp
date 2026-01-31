// Binary entry point for ahma_mcp
// This is a thin wrapper that delegates to the library implementation

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = ahma_mcp::shell::run().await {
        eprintln!("ahma_mcp fatal error: {:#}", e);
        return Err(e);
    }
    Ok(())
}
