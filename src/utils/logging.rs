//! Logging initialization for the server.

use anyhow::Result;
use directories::ProjectDirs;
use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

static INIT: Once = Once::new();

/// Initializes the logging system.
///
/// This function sets up a global tracing subscriber. It can be configured to
/// log to stderr or to a daily rolling file in the project's cache directory.
///
/// # Errors
///
/// Returns an error if the project directories cannot be determined.
pub fn init_logging() -> Result<()> {
    INIT.call_once(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,ahma_mcp=debug"));

        // Attempt to log to a file, fall back to stderr.
        if let Some(proj_dirs) = ProjectDirs::from("com", "AhmaMcp", "ahma_mcp") {
            let log_dir = proj_dirs.cache_dir();
            let file_appender = tracing_appender::rolling::daily(log_dir, "ahma_mcp.log");
            let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
                .init();
            // The guard is intentionally leaked to ensure logs are flushed on exit.
            Box::leak(Box::new(_guard));
        } else {
            // Fallback to stderr if project directory is not available.
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(std::io::stderr))
                .init();
        }
    });
    Ok(())
}
