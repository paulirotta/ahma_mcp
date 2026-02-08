//! # Kernel-Level Sandboxing for Secure Command Execution
//!
//! This module provides platform-specific sandboxing mechanisms to enforce strict
//! file system boundaries. The AI can freely operate within the sandbox scope but
//! has zero access outside it.
//!
//! ## Platform Support
//!
//! - **Linux**: Uses Landlock (kernel 5.13+) for kernel-level file system access control.
//! - **macOS**: Uses sandbox-exec with Seatbelt profiles for file system access control.
//!
//! ## Architecture
//!
//! The `Sandbox` struct encapsulates the security context (allowed roots, strictness, temp file policy).
//! It is passed to the `Adapter` to validate paths and wrap commands.

mod command;
pub(crate) mod core;
mod error;
mod landlock;
mod prerequisites;
mod scopes;
#[cfg(target_os = "macos")]
mod seatbelt;
mod types;

pub use core::Sandbox;
pub use error::SandboxError;
pub use landlock::enforce_landlock_sandbox;
pub use prerequisites::{
    check_sandbox_prerequisites, exit_with_sandbox_error, test_sandbox_exec_available,
};
pub use scopes::normalize_path_lexically;
pub use types::{SandboxMode, ScopesGuard};
