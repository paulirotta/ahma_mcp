use std::process::Command;

pub const SANDBOX_BYPASS_ENV_VARS: &[&str] = &[
    "NEXTEST",
    "NEXTEST_EXECUTION_MODE",
    "CARGO_TARGET_DIR",
    "RUST_TEST_THREADS",
];

pub struct SandboxTestEnv;

impl SandboxTestEnv {
    pub fn configure(cmd: &mut Command) -> &mut Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    pub fn configure_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

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

    pub fn is_bypass_active() -> bool {
        SANDBOX_BYPASS_ENV_VARS
            .iter()
            .any(|var| std::env::var(var).is_ok())
    }
}
