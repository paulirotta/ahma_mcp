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
//!
//! - **`time`**: Offers functionality for working with time-related tasks, building
//!   upon the `chrono` crate to provide date and time manipulation features.

pub mod logging;
pub mod time;
pub mod timestamp;
