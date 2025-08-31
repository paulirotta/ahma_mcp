//! # Logging Initialization
//!
//! This module provides a centralized function for initializing the application's
//! logging infrastructure. It uses the `tracing` ecosystem to provide structured,
//! configurable logging.
//!
//! ## Core Functionality
//!
//! - **`init_logging()`**: This is the main function of the module. It is designed to
//!   be called once at the start of the application's lifecycle. It uses a `std::sync::Once`
//!   to ensure that the initialization logic is executed only a single time, even if
//!   the function is called multiple times.
//!
//! ## Logging Configuration
//!
//! The function sets up a multi-layered logging system:
//!
//! 1.  **Environment Filter (`EnvFilter`)**: It configures the logging verbosity based on
//!     the `RUST_LOG` environment variable. If `RUST_LOG` is not set, it defaults to a
//!     sensible configuration: `info` for most crates, but `debug` for the `ahma_mcp`
//!     crate itself.
//!
//! 2.  **File Logging**: It attempts to create a daily rolling log file in the appropriate
//!     user-specific cache directory (determined by the `directories` crate). This is
//!     the preferred logging target, as it preserves log history without cluttering the
//!     console. It uses `tracing_appender` to handle the file rotation and non-blocking
//!     I/O.
//!
//! 3.  **Stderr Fallback**: If the project's cache directory cannot be determined (e.g.,
//!     in a sandboxed or unusual environment), the logger gracefully falls back to writing
//!     logs to `stderr`.
//!
//! ## Usage
//!
//! To enable logging, simply call `ahma_mcp::utils::logging::init_logging()` at the
//! beginning of the `main` function.

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
