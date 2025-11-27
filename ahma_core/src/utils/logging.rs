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
//! 2.  **File Logging (Default)**: By default (`log_to_file = true`), it creates a daily
//!     rolling log file in the user-specific cache directory (determined by the `directories`
//!     crate). This preserves log history without cluttering the console. It uses
//!     `tracing_appender` to handle file rotation and non-blocking I/O. ANSI colors are
//!     disabled for file output.
//!
//! 3.  **Stderr Logging (Opt-in)**: When `log_to_file = false`, all logs are written to
//!     `stderr` with ANSI color codes enabled for better readability on Mac/Linux terminals.
//!     Error messages appear in red, warnings in yellow, etc. This mode is useful for
//!     debugging and development with tools like MCP Inspector.
//!
//! 4.  **Stderr Fallback**: If file logging is requested but the project's cache directory
//!     cannot be determined (e.g., in a sandboxed or unusual environment), the logger
//!     gracefully falls back to writing logs to `stderr` with colors enabled.
//!
//! ## Usage
//!
//! To enable logging, call `ahma_mcp::utils::logging::init_logging(log_level, log_to_file)`
//! at the beginning of the `main` function.
//!
//! For terminal debugging: `init_logging("debug", false)` (logs to stderr with colors)
//! For production: `init_logging("info", true)` (logs to file without colors)

use anyhow::Result;
use directories::ProjectDirs;
use std::{io::stderr, sync::Once};
use tracing_subscriber::{EnvFilter, fmt::layer, prelude::*};

static INIT: Once = Once::new();

pub fn init_test_logging() {
    init_logging("trace", false).expect("Failed to initialize test logging");
}

/// Initializes the logging system.
///
/// This function sets up a global tracing subscriber. It can be configured to
/// log to stderr or to a daily rolling file in the project's cache directory.
///
/// When logging to stderr, ANSI colors are enabled for better readability.
/// When logging to file, ANSI colors are disabled.
///
/// # Errors
///
/// Returns an error if the project directories cannot be determined.
pub fn init_logging(log_level: &str, log_to_file: bool) -> Result<()> {
    INIT.call_once(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("{log_level},ahma_mcp=debug")));

        // Attempt to log to a file, fall back to stderr.
        if log_to_file {
            if let Some(proj_dirs) = ProjectDirs::from("com", "AhmaMcp", "ahma_mcp") {
                let log_dir = proj_dirs.cache_dir();

                // Try to create the log directory first
                let dir_created = std::fs::create_dir_all(log_dir).is_ok();

                // Try to create the file appender, fall back to stderr if it fails
                // Use catch_unwind to handle panics from tracing_appender
                let file_appender_result = if dir_created {
                    std::panic::catch_unwind(|| {
                        tracing_appender::rolling::daily(log_dir, "ahma_mcp.log")
                    })
                } else {
                    Err(Box::new("Failed to create log directory") as Box<dyn std::any::Any + Send>)
                };

                match file_appender_result {
                    Ok(file_appender) => {
                        // Successfully created file appender
                        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

                        tracing_subscriber::registry()
                            .with(env_filter)
                            .with(layer().with_writer(non_blocking).with_ansi(false))
                            .init();
                        // The guard is intentionally leaked to ensure logs are flushed on exit.
                        Box::leak(Box::new(_guard));
                    }
                    Err(_) => {
                        // Fallback to stderr if file appender creation fails or panics
                        // This handles permission denied, sandboxing issues, etc.
                        // Enable ANSI colors for terminal output
                        tracing_subscriber::registry()
                            .with(env_filter)
                            .with(layer().with_writer(stderr).with_ansi(true))
                            .init();
                    }
                }
            } else {
                // Fallback to stderr if project directory is not available.
                // Enable ANSI colors for terminal output
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(layer().with_writer(stderr).with_ansi(true))
                    .init();
            }
        } else {
            // Log to stderr with ANSI colors enabled for terminal output
            tracing_subscriber::registry()
                .with(env_filter)
                .with(layer().with_writer(stderr).with_ansi(true))
                .init();
        }
    });

    Ok(())
}
