//! Sandbox test environment helpers.
//!
//! Utilities for testing real sandbox behavior by ensuring spawned
//! `ahma_mcp` processes don't inherit environment variables that
//! enable permissive test mode.

use std::process::Command;

/// Environment variables that enable permissive test mode in ahma_mcp.
///
/// When spawning ahma_mcp processes for tests that verify real sandbox behavior,
/// these env vars must be cleared to prevent the process from auto-enabling
/// a permissive test mode that bypasses sandbox validation.
///
/// See AGENTS.md for the full explanation of this failure mode.
pub const SANDBOX_BYPASS_ENV_VARS: &[&str] = &[
    "NEXTEST",
    "NEXTEST_EXECUTION_MODE",
    "CARGO_TARGET_DIR",
    "RUST_TEST_THREADS",
];

/// Helper to configure a `Command` for sandbox-isolated testing.
///
/// Removes all environment variables that could trigger permissive test mode
/// in the spawned ahma_mcp process, ensuring real sandbox behavior is tested.
pub struct SandboxTestEnv;

impl SandboxTestEnv {
    /// Configure a Command to test real sandbox behavior by removing bypass env vars.
    pub fn configure(cmd: &mut Command) -> &mut Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    /// Configure a tokio Command to test real sandbox behavior.
    pub fn configure_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    /// Get a list of key=value pairs for the env vars that would bypass sandbox.
    /// Useful for debugging which vars are set in the current environment.
    pub fn current_bypass_vars() -> Vec<String> {
        SANDBOX_BYPASS_ENV_VARS
            .iter()
            .filter_map(|var| {
                std::env::var(var)
                    .ok()
                    .map(|val| format!("{}={}", var, val))
            })
            .collect()
    }

    /// Check if any bypass env vars are currently set.
    pub fn is_bypass_active() -> bool {
        SANDBOX_BYPASS_ENV_VARS
            .iter()
            .any(|var| std::env::var(var).is_ok())
    }
}
