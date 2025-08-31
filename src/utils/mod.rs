//! # Utility Modules
//!
//! This module serves as a container for various utility sub-modules that provide
//! common, cross-cutting functionality used throughout the application.
//!
//! ## Sub-modules
//!
//! - **`logging`**: Contains functions for initializing and configuring the application's
//!   logging infrastructure using the `tracing` crate.
//!
//! - **`timestamp`**: Provides helpers for generating and formatting timestamps, which
//!   can be useful for logging, creating unique identifiers, or tracking event times.

pub mod logging;
pub mod timestamp;
