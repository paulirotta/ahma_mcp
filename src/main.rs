//! The main entry point for the Ahma MCP server.

use ahma_mcp::logging;

fn main() {
    // For now, just initialize logging.
    // We will build this out as we implement the other modules.
    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }
}
