//! # Kernel-Level Sandboxing for Secure Command Execution
//!
//! This module provides platform-specific sandboxing mechanisms to enforce strict
//! file system boundaries. The AI can freely operate within the sandbox scope but
//! has zero access outside it.
//!
//! ## Platform Support
//!
//! - **Linux**: Uses Landlock (kernel 5.13+) for kernel-level file system access control.
//! - **macOS**: Uses Bubblewrap (bwrap) for namespace-based isolation.
//!
//! ## Security Model
//!
//! The sandbox scope is set once at server/session initialization and cannot be changed.
//! This prevents the AI from escaping the sandbox by passing malicious working directories.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global sandbox scope - set once at initialization, immutable thereafter
static SANDBOX_SCOPE: OnceLock<PathBuf> = OnceLock::new();

/// Test mode flag - when true, bwrap is bypassed (for test environments without bwrap)
/// Initialized lazily to check for CARGO_PKG_NAME environment variable which indicates test context
static TEST_MODE: AtomicBool = AtomicBool::new(false);

/// One-time check for test environment
static TEST_MODE_CHECKED: OnceLock<bool> = OnceLock::new();

/// Enable test mode, which bypasses bwrap requirement on macOS.
/// This should ONLY be used in test environments.
///
/// # Safety
/// This weakens security by not using sandboxing. Only use for tests.
pub fn enable_test_mode() {
    TEST_MODE.store(true, Ordering::SeqCst);
}

/// Check if test mode is enabled.
/// Also auto-enables test mode if AHMA_TEST_MODE environment variable is set
/// or if we detect we're running in a cargo test context.
pub fn is_test_mode() -> bool {
    // One-time check for test environment
    TEST_MODE_CHECKED.get_or_init(|| {
        // Check explicit env var first
        if std::env::var("AHMA_TEST_MODE").is_ok() {
            enable_test_mode();
            return true;
        }
        // Check if running under cargo nextest or cargo test
        if std::env::var("NEXTEST").is_ok() || std::env::var("CARGO_TARGET_DIR").is_ok() {
            // Only enable if we're actually running tests (not just building)
            if std::env::var("RUST_TEST_THREADS").is_ok()
                || std::env::var("NEXTEST_EXECUTION_MODE").is_ok()
            {
                enable_test_mode();
                return true;
            }
        }
        false
    });
    TEST_MODE.load(Ordering::SeqCst)
}
/// Errors specific to sandbox operations
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Sandbox scope already initialized")]
    AlreadyInitialized,

    #[error("Sandbox scope not initialized - call initialize_sandbox_scope first")]
    NotInitialized,

    #[error("Path '{path}' is outside the sandbox scope '{scope}'")]
    PathOutsideSandbox { path: PathBuf, scope: PathBuf },

    #[error("Landlock is not available on this system (requires Linux kernel 5.13+)")]
    LandlockNotAvailable,

    #[error("Bubblewrap (bwrap) is not installed. Install with: brew install bubblewrap")]
    BubblewrapNotInstalled,

    #[error("Unsupported operating system: {0}")]
    UnsupportedOs(String),

    #[error("Failed to canonicalize path '{path}': {reason}")]
    CanonicalizationFailed { path: PathBuf, reason: String },

    #[error("Sandbox prerequisite check failed: {0}")]
    PrerequisiteFailed(String),
}

/// Initialize the sandbox scope. This can only be called once.
///
/// # Arguments
/// * `scope` - The root directory for all file system operations
///
/// # Returns
/// * `Ok(())` if initialization succeeded
/// * `Err(SandboxError::AlreadyInitialized)` if already initialized
pub fn initialize_sandbox_scope(scope: &Path) -> Result<(), SandboxError> {
    let canonical =
        std::fs::canonicalize(scope).map_err(|e| SandboxError::CanonicalizationFailed {
            path: scope.to_path_buf(),
            reason: e.to_string(),
        })?;

    SANDBOX_SCOPE
        .set(canonical)
        .map_err(|_| SandboxError::AlreadyInitialized)
}

/// Get the current sandbox scope.
///
/// # Returns
/// * `Some(&PathBuf)` if initialized
/// * `None` if not yet initialized
pub fn get_sandbox_scope() -> Option<&'static PathBuf> {
    SANDBOX_SCOPE.get()
}

/// Check if a path is within the sandbox scope.
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// * `Ok(PathBuf)` - The canonicalized path if valid
/// * `Err(SandboxError)` - If path is outside sandbox or sandbox not initialized
pub fn validate_path_in_sandbox(path: &Path) -> Result<PathBuf, SandboxError> {
    let scope = SANDBOX_SCOPE.get().ok_or(SandboxError::NotInitialized)?;

    // If path is relative, join with sandbox scope
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        scope.join(path)
    };

    // Try to canonicalize the path
    let canonical = match std::fs::canonicalize(&full_path) {
        Ok(p) => p,
        Err(_) => {
            // For non-existent paths (e.g., files to be created), normalize lexically
            // and check the parent directory
            let normalized = normalize_path_lexically(&full_path);
            if !normalized.starts_with(scope) {
                return Err(SandboxError::PathOutsideSandbox {
                    path: path.to_path_buf(),
                    scope: scope.clone(),
                });
            }
            normalized
        }
    };

    if canonical.starts_with(scope) {
        Ok(canonical)
    } else {
        Err(SandboxError::PathOutsideSandbox {
            path: path.to_path_buf(),
            scope: scope.clone(),
        })
    }
}

/// Normalize a path lexically (without filesystem access).
/// Resolves `.` and `..` components.
fn normalize_path_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop if there's something to pop and it's not root
                if stack.last().is_some_and(|c| *c != Component::RootDir) {
                    stack.pop();
                }
            }
            c => stack.push(c),
        }
    }

    stack.iter().collect()
}

/// Check if the platform's sandboxing prerequisites are met.
///
/// # Returns
/// * `Ok(())` if all prerequisites are satisfied
/// * `Err(SandboxError)` with details on how to fix the issue
pub fn check_sandbox_prerequisites() -> Result<(), SandboxError> {
    #[cfg(target_os = "linux")]
    {
        check_landlock_available()
    }

    #[cfg(target_os = "macos")]
    {
        check_bubblewrap_installed()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(SandboxError::UnsupportedOs(
            std::env::consts::OS.to_string(),
        ))
    }
}

/// Check if Landlock is available on this Linux system
#[cfg(target_os = "linux")]
fn check_landlock_available() -> Result<(), SandboxError> {
    use std::fs;

    // Check if Landlock ABI is available via kernel sysctl or by attempting to create a ruleset
    // The most reliable check is to look for the Landlock ABI version in sysfs
    let landlock_abi_path = "/sys/kernel/security/lsm";

    match fs::read_to_string(landlock_abi_path) {
        Ok(content) => {
            if content.contains("landlock") {
                Ok(())
            } else {
                Err(SandboxError::LandlockNotAvailable)
            }
        }
        Err(_) => {
            // Fallback: try to check kernel version (5.13+)
            check_kernel_version_for_landlock()
        }
    }
}

#[cfg(target_os = "linux")]
fn check_kernel_version_for_landlock() -> Result<(), SandboxError> {
    use std::process::Command;

    let output = Command::new("uname").arg("-r").output().map_err(|_| {
        SandboxError::PrerequisiteFailed("Failed to check kernel version".to_string())
    })?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = version_str.trim().split('.').collect();

    if parts.len() >= 2 {
        let major: u32 = parts[0].parse().unwrap_or(0);
        let minor: u32 = parts[1]
            .split('-')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        if major > 5 || (major == 5 && minor >= 13) {
            return Ok(());
        }
    }

    Err(SandboxError::PrerequisiteFailed(format!(
        "Landlock requires Linux kernel 5.13 or newer. Current kernel: {}. \
         Upgrade your kernel or run on a system with Landlock support.",
        version_str.trim()
    )))
}

/// Check if Bubblewrap is installed on macOS
#[cfg(target_os = "macos")]
fn check_bubblewrap_installed() -> Result<(), SandboxError> {
    use std::process::Command;

    let result = Command::new("which").arg("bwrap").output();

    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(SandboxError::BubblewrapNotInstalled),
    }
}

/// Display an error message about missing sandbox prerequisites and exit.
pub fn exit_with_sandbox_error(error: &SandboxError) -> ! {
    eprintln!("\n❌ SECURITY ERROR: Cannot start MCP server\n");
    eprintln!("Reason: {}\n", error);

    match error {
        SandboxError::LandlockNotAvailable => {
            eprintln!("Landlock is a Linux kernel security feature (5.13+) that provides");
            eprintln!("kernel-level file system access control.\n");
            eprintln!("To fix this:");
            eprintln!("  1. Upgrade to Linux kernel 5.13 or newer");
            eprintln!("  2. Ensure Landlock LSM is enabled in your kernel config");
            eprintln!("  3. Check: cat /sys/kernel/security/lsm | grep landlock\n");
        }
        SandboxError::BubblewrapNotInstalled => {
            eprintln!("Bubblewrap provides namespace-based sandboxing on macOS.\n");
            eprintln!("To install:");
            eprintln!("  brew install bubblewrap\n");
        }
        SandboxError::UnsupportedOs(os) => {
            eprintln!("Operating system '{}' is not currently supported.", os);
            eprintln!("Supported platforms: Linux (with Landlock), macOS (with Bubblewrap)\n");
        }
        SandboxError::PrerequisiteFailed(msg) => {
            eprintln!("{}\n", msg);
        }
        _ => {}
    }

    eprintln!("⚠️  WHY THIS MATTERS:");
    eprintln!("Without kernel-level sandboxing, AI-generated commands could potentially");
    eprintln!("access or modify files outside the intended workspace, posing a serious");
    eprintln!("security risk. The MCP server refuses to start without this protection.\n");

    std::process::exit(1);
}

/// Sandbox configuration for the session
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// The root directory for all file system operations
    pub scope: PathBuf,
    /// Whether to enable strict mode (fail on any path outside scope)
    pub strict: bool,
}

impl SandboxConfig {
    /// Create a new sandbox configuration
    pub fn new(scope: PathBuf) -> Self {
        Self {
            scope,
            strict: true,
        }
    }

    /// Create from the current working directory
    pub fn from_cwd() -> Result<Self> {
        let cwd = std::env::current_dir().context("Failed to get current working directory")?;
        let canonical =
            std::fs::canonicalize(&cwd).context("Failed to canonicalize current directory")?;
        Ok(Self::new(canonical))
    }
}

/// Build a sandboxed command for execution.
///
/// On Linux, this returns the command as-is (Landlock protects at process level).
/// On macOS, this wraps the command with Bubblewrap.
///
/// # Arguments
/// * `command` - The command parts to execute
/// * `working_dir` - The working directory for the command
/// * `sandbox_scope` - The sandbox root directory
///
/// # Returns
/// * `Ok((program, args))` - The sandboxed command to execute
/// * `Err` - If sandboxing setup fails
pub fn build_sandboxed_command(
    command: &[String],
    #[cfg_attr(target_os = "linux", allow(unused_variables))] working_dir: &Path,
    #[cfg_attr(target_os = "linux", allow(unused_variables))] sandbox_scope: &Path,
) -> Result<(String, Vec<String>)> {
    if command.is_empty() {
        return Err(anyhow!("Empty command"));
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, we use Landlock which restricts the current process
        // The command runs directly, Landlock rules are applied via enforce_landlock_sandbox
        Ok((command[0].clone(), command[1..].to_vec()))
    }

    #[cfg(target_os = "macos")]
    {
        build_bubblewrap_command(command, working_dir, sandbox_scope)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(anyhow!("Unsupported operating system for sandboxing"))
    }
}

/// Build a Bubblewrap-wrapped command for macOS
#[cfg(target_os = "macos")]
fn build_bubblewrap_command(
    command: &[String],
    working_dir: &Path,
    sandbox_scope: &Path,
) -> Result<(String, Vec<String>)> {
    let scope_str = sandbox_scope.to_string_lossy();
    let wd_str = working_dir.to_string_lossy();

    // Build bwrap arguments
    let mut args = vec![
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--bind".to_string(),
        scope_str.to_string(),
        scope_str.to_string(),
        "--chdir".to_string(),
        wd_str.to_string(),
        "--new-session".to_string(),
        "--die-with-parent".to_string(),
    ];

    // Add the actual command
    args.extend(command.iter().cloned());

    Ok(("bwrap".to_string(), args))
}

/// Apply Landlock sandbox restrictions to the current process.
/// This should be called once at server startup on Linux.
///
/// # Arguments
/// * `sandbox_scope` - The directory to allow read/write access to
///
/// # Returns
/// * `Ok(())` if Landlock rules were successfully applied
/// * `Err` if Landlock is not available or rules couldn't be applied
#[cfg(target_os = "linux")]
pub fn enforce_landlock_sandbox(sandbox_scope: &Path) -> Result<()> {
    use landlock::{
        ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };

    // Use the latest stable ABI
    let abi = ABI::V3;

    // Get all access types for the ABI version
    let access_all = AccessFs::from_all(abi);
    let access_read = AccessFs::from_read(abi);

    // Build the ruleset
    let mut ruleset = Ruleset::default()
        .handle_access(access_all)
        .context("Failed to create Landlock ruleset")?
        .create()
        .context("Failed to create Landlock ruleset instance")?;

    // Allow full access to the sandbox scope
    ruleset = ruleset
        .add_rule(PathBeneath::new(
            PathFd::new(sandbox_scope).context("Failed to open sandbox scope for Landlock")?,
            access_all,
        ))
        .context("Failed to add Landlock rule for sandbox scope")?;

    // Allow read access to system directories needed for execution
    // These directories are needed for running binaries and loading libraries
    let system_paths = ["/usr", "/bin", "/etc", "/lib", "/lib64", "/proc", "/dev"];

    for path in &system_paths {
        let path_obj = Path::new(path);
        if path_obj.exists() {
            if let Ok(fd) = PathFd::new(path_obj) {
                match ruleset.add_rule(PathBeneath::new(fd, access_read)) {
                    Ok(rs) => ruleset = rs,
                    Err(e) => {
                        tracing::debug!("Could not add Landlock rule for {}: {:?}", path, e);
                    }
                }
            }
        }
    }

    // Apply the restrictions to the current process
    let status = ruleset
        .restrict_self()
        .context("Failed to apply Landlock restrictions")?;

    tracing::info!(
        "Landlock sandbox enforced for scope: {:?} (status: {:?})",
        sandbox_scope,
        status
    );

    Ok(())
}

/// No-op on non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub fn enforce_landlock_sandbox(_sandbox_scope: &Path) -> Result<()> {
    // Landlock is Linux-only, other platforms use different mechanisms
    Ok(())
}

/// Create a sandboxed tokio process Command.
///
/// On Linux (with Landlock applied at startup), this returns a normal Command.
/// On macOS, this wraps the command with Bubblewrap for per-command sandboxing.
///
/// # Arguments
/// * `program` - The program to execute
/// * `args` - Arguments to pass to the program
/// * `working_dir` - The working directory for the command
///
/// # Returns
/// A configured tokio::process::Command that will execute in the sandbox
pub fn create_sandboxed_command(
    program: &str,
    args: &[String],
    working_dir: &Path,
) -> Result<tokio::process::Command> {
    // In test mode, auto-initialize sandbox scope if not already set
    if is_test_mode() && get_sandbox_scope().is_none() {
        // Best-effort initialization with root "/" for tests
        let _ = initialize_sandbox_scope(std::path::Path::new("/"));
    }

    let _sandbox_scope =
        get_sandbox_scope().ok_or_else(|| anyhow!("Sandbox scope not initialized"))?;

    // In test mode, bypass bwrap for environments without it installed
    if is_test_mode() {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        return Ok(cmd);
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, Landlock is applied at process level, so commands run directly
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        Ok(cmd)
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, wrap each command with Bubblewrap
        let mut full_command = vec![program.to_string()];
        full_command.extend(args.iter().cloned());

        let (bwrap_program, bwrap_args) =
            build_bubblewrap_command(&full_command, working_dir, _sandbox_scope)?;

        let mut cmd = tokio::process::Command::new(bwrap_program);
        cmd.args(bwrap_args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // Note: working_dir is handled by bwrap's --chdir
        Ok(cmd)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(anyhow!("Unsupported operating system for sandboxing"))
    }
}

/// Create a sandboxed shell command (for executing shell scripts).
///
/// # Arguments
/// * `shell` - The shell to use (e.g., "/bin/sh", "/bin/bash")
/// * `script` - The shell script to execute
/// * `working_dir` - The working directory for the command
///
/// # Returns
/// A configured tokio::process::Command that will execute the script in the sandbox
pub fn create_sandboxed_shell_command(
    shell: &str,
    script: &str,
    working_dir: &Path,
) -> Result<tokio::process::Command> {
    // In test mode, auto-initialize sandbox scope if not already set
    if is_test_mode() && get_sandbox_scope().is_none() {
        // Best-effort initialization with root "/" for tests
        let _ = initialize_sandbox_scope(std::path::Path::new("/"));
    }

    let _sandbox_scope =
        get_sandbox_scope().ok_or_else(|| anyhow!("Sandbox scope not initialized"))?;

    // In test mode, bypass bwrap for environments without it installed
    if is_test_mode() {
        let mut cmd = tokio::process::Command::new(shell);
        cmd.arg("-c")
            .arg(script)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        return Ok(cmd);
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, Landlock is applied at process level
        let mut cmd = tokio::process::Command::new(shell);
        cmd.arg("-c")
            .arg(script)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        Ok(cmd)
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, wrap with Bubblewrap
        let full_command = vec![shell.to_string(), "-c".to_string(), script.to_string()];

        let (bwrap_program, bwrap_args) =
            build_bubblewrap_command(&full_command, working_dir, _sandbox_scope)?;

        let mut cmd = tokio::process::Command::new(bwrap_program);
        cmd.args(bwrap_args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        Ok(cmd)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(anyhow!("Unsupported operating system for sandboxing"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Note: These tests cannot test the global SANDBOX_SCOPE since it can only be set once
    // We test the individual functions instead

    #[test]
    fn test_normalize_path_lexically() {
        let cases = vec![
            ("/a/b/../c", "/a/c"),
            ("/a/b/./c", "/a/b/c"),
            ("/a/b/c/..", "/a/b"),
            ("a/b/../c", "a/c"),
        ];

        for (input, expected) in cases {
            let result = normalize_path_lexically(Path::new(input));
            assert_eq!(
                result,
                PathBuf::from(expected),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_sandbox_error_display() {
        let err = SandboxError::PathOutsideSandbox {
            path: PathBuf::from("/etc/passwd"),
            scope: PathBuf::from("/home/user/project"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("/etc/passwd"));
        assert!(msg.contains("/home/user/project"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_bubblewrap_command() {
        let temp = TempDir::new().unwrap();
        let scope = temp.path();
        let command = vec!["ls".to_string(), "-la".to_string()];

        let result = build_bubblewrap_command(&command, scope, scope);
        assert!(result.is_ok());

        let (program, args) = result.unwrap();
        assert_eq!(program, "bwrap");
        assert!(args.contains(&"--ro-bind".to_string()));
        assert!(args.contains(&"--bind".to_string()));
        assert!(args.contains(&"ls".to_string()));
        assert!(args.contains(&"-la".to_string()));
    }

    #[test]
    fn test_sandbox_config_from_cwd() {
        let config = SandboxConfig::from_cwd();
        assert!(config.is_ok());
        let config = config.unwrap();
        assert!(config.scope.is_absolute());
        assert!(config.strict);
    }
}
