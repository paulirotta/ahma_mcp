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

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

/// Errors specific to sandbox operations
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error(
        "Path '{path}' is outside the sandbox root{} (this usually means your MCP session is scoped to a different workspace root; reconnect from the intended workspace or use a multi-root workspace)",
        format_scopes(.scopes)
    )]
    PathOutsideSandbox { path: PathBuf, scopes: Vec<PathBuf> },

    #[error("Landlock is not available on this system (requires Linux kernel 5.13+)")]
    LandlockNotAvailable,

    #[error("macOS sandbox-exec is not available")]
    MacOSSandboxNotAvailable,

    #[error("Unsupported operating system: {0}")]
    UnsupportedOs(String),

    #[error("Failed to canonicalize path '{path}': {reason}")]
    CanonicalizationFailed { path: PathBuf, reason: String },

    #[error("Sandbox prerequisite check failed: {0}")]
    PrerequisiteFailed(String),

    #[error("Path '{path}' is blocked by high-security mode (no-temp-files)")]
    HighSecurityViolation { path: PathBuf },

    #[error(
        "Nested sandbox detected - running inside another sandbox (e.g., Cursor, VS Code, Docker)"
    )]
    NestedSandboxDetected,
}

/// Format sandbox scopes for error messages
fn format_scopes(scopes: &[PathBuf]) -> String {
    if scopes.is_empty() {
        " (none configured)".to_string()
    } else if scopes.len() == 1 {
        format!(" '{}'", scopes[0].display())
    } else {
        let scope_list: Vec<String> = scopes
            .iter()
            .map(|s| format!("'{}'", s.display()))
            .collect();
        format!("s [{}]", scope_list.join(", "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Strict,
    Test,
}

/// The security context for the Ahma session.
#[derive(Debug, Clone)]
pub struct Sandbox {
    scopes: Vec<PathBuf>,
    mode: SandboxMode,
    no_temp_files: bool,
}

impl Sandbox {
    /// Create a new Sandbox with the given scopes.
    pub fn new(scopes: Vec<PathBuf>, mode: SandboxMode, no_temp_files: bool) -> Result<Self> {
        let mut canonicalized = Vec::with_capacity(scopes.len());
        for scope in scopes {
            // Best-effort canonicalization; if it fails (e.g. doesn't exist), we might still want to track it
            // but strictly speaking, a sandbox root that doesn't exist is useless.
            // We'll enforce existence via canonicalize.
            let canonical = std::fs::canonicalize(&scope).map_err(|e| {
                anyhow!(
                    "Failed to canonicalize sandbox scope '{}': {}",
                    scope.display(),
                    e
                )
            })?;
            canonicalized.push(canonical);
        }

        Ok(Self {
            scopes: canonicalized,
            mode,
            no_temp_files,
        })
    }

    /// Create a sandbox in Test mode (bypasses restrictions).
    pub fn new_test() -> Self {
        Self {
            scopes: vec![PathBuf::from("/")],
            mode: SandboxMode::Test,
            no_temp_files: false,
        }
    }

    /// Check if the sandbox is in test mode.
    pub fn is_test_mode(&self) -> bool {
        self.mode == SandboxMode::Test
    }

    /// Check if no-temp-files mode is enabled.
    pub fn is_no_temp_files(&self) -> bool {
        self.no_temp_files
    }

    /// Get the allowed scopes.
    pub fn scopes(&self) -> &[PathBuf] {
        &self.scopes
    }

    /// Check if a path is within any of the sandbox scopes.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        // In Test mode, we only bypass validation if the sandbox is explicitly unrestricted (root scope or no scopes).
        // If specific scopes are provided, we enforce them logically even in Test mode, while still bypassing
        // the kernel-level sandboxing in create_command.
        if self.mode == SandboxMode::Test
            && (self.scopes.is_empty() || self.scopes.iter().any(|s| s == Path::new("/")))
        {
            return std::fs::canonicalize(path).or_else(|_| Ok(path.to_path_buf()));
        }

        if self.scopes.is_empty() {
            return Err(anyhow!("No sandbox scopes configured"));
        }

        // We assume the first scope is the "primary" one for relative path resolution
        let first_scope = self.scopes.first().ok_or_else(|| anyhow!("No scopes"))?;

        // If path is relative, join with first sandbox scope
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            first_scope.join(path)
        };

        // Try to canonicalize the path.
        // If the path doesn't exist, try canonicalizing its parent directory
        // to handle symlinks correctly (especially important on macOS where /var is a symlink).
        let canonical = match std::fs::canonicalize(&full_path) {
            Ok(p) => p,
            Err(_) => {
                if let Some(parent) = full_path.parent() {
                    if let Ok(parent_canonical) = std::fs::canonicalize(parent) {
                        if let Some(name) = full_path.file_name() {
                            parent_canonical.join(name)
                        } else {
                            parent_canonical
                        }
                    } else {
                        normalize_path_lexically(&full_path)
                    }
                } else {
                    normalize_path_lexically(&full_path)
                }
            }
        };

        // Check if canonical path is within any scope
        // Note: In LANDLOCK (Linux) mode, the kernel enforces this too, but we check here for helpful errors.
        if self.scopes.iter().any(|scope| canonical.starts_with(scope)) {
            // High security mode: block temp directories and /dev even if they are within scope
            if self.no_temp_files {
                let path_str = canonical.to_string_lossy();
                if path_str.starts_with("/tmp")
                    || path_str.starts_with("/var/folders")
                    || path_str.starts_with("/private/tmp")
                    || path_str.starts_with("/private/var/folders")
                    || path_str.starts_with("/dev")
                {
                    return Err(SandboxError::HighSecurityViolation {
                        path: path.to_path_buf(),
                    }
                    .into());
                }
            }
            Ok(canonical)
        } else {
            Err(SandboxError::PathOutsideSandbox {
                path: path.to_path_buf(),
                scopes: self.scopes.clone(),
            }
            .into())
        }
    }

    /// Create a sandboxed tokio process Command.
    pub fn create_command(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
    ) -> Result<tokio::process::Command> {
        if self.mode == SandboxMode::Test {
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

            // Cargo can be configured (via config or env) to write its target dir outside
            // the session sandbox. Force it back inside the working directory.
            if std::path::Path::new(program)
                .file_name()
                .is_some_and(|n| n == "cargo")
            {
                cmd.env("CARGO_TARGET_DIR", working_dir.join("target"));
            }
            Ok(cmd)
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, wrap each command with sandbox-exec
            let mut full_command = vec![program.to_string()];
            full_command.extend(args.iter().cloned());

            // For macOS seatbelt, we generate a profile that allows access to all scopes
            let (sandbox_program, sandbox_args) =
                self.build_macos_sandbox_command(&full_command, working_dir)?;

            let mut cmd = tokio::process::Command::new(sandbox_program);
            cmd.args(sandbox_args)
                .current_dir(working_dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            // Cargo can be configured (via config or env) to write its target dir outside
            // the session sandbox. Force it back inside the working directory.
            if std::path::Path::new(program)
                .file_name()
                .is_some_and(|n| n == "cargo")
            {
                cmd.env("CARGO_TARGET_DIR", working_dir.join("target"));
            }
            Ok(cmd)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // Fallback for unsupported OS - run directly if not in strict mode, or fail?
            // Since we have a check_sandbox_prerequisites, we assume if we are here we are allowed to run.
            // If strict mode is enforced elsewhere, maybe fine.
            // But for safety, let's behave like linux (no-op) but betterwarn?
            let mut cmd = tokio::process::Command::new(program);
            cmd.args(args)
                .current_dir(working_dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            Ok(cmd)
        }
    }

    /// Create a sandboxed shell command (e.g. bash -c "complex command")
    pub fn create_shell_command(
        &self,
        shell_program: &str,
        full_command: &str,
        working_dir: &Path,
    ) -> Result<tokio::process::Command> {
        let args = vec!["-c".to_string(), full_command.to_string()];
        self.create_command(shell_program, &args, working_dir)
    }

    #[cfg(target_os = "macos")]
    fn build_macos_sandbox_command(
        &self,
        command: &[String],
        working_dir: &Path,
    ) -> Result<(String, Vec<String>)> {
        // Generate a Seatbelt profile
        // We pass ALL scopes to the generator
        let profile = self.generate_seatbelt_profile(working_dir);

        // Build sandbox-exec arguments
        let mut args = vec!["-p".to_string(), profile];

        // Add the actual command
        args.extend(command.iter().cloned());

        Ok(("sandbox-exec".to_string(), args))
    }

    #[cfg(target_os = "macos")]
    fn generate_seatbelt_profile(&self, working_dir: &Path) -> String {
        let wd_str = working_dir.to_string_lossy();

        // Allowed scopes strings (all scopes are read/write allowed)
        let mut scope_rules = String::new();
        for scope in &self.scopes {
            scope_rules.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                scope.display()
            ));
        }

        // Get home directory for user-specific tool paths
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/Users/Shared".to_string());
        let home_path = std::path::Path::new(&home_dir);

        // Build optional user tool directory rules (only for paths that exist)
        let mut user_tool_rules = String::new();
        let cargo_path = home_path.join(".cargo");
        let rustup_path = home_path.join(".rustup");

        if cargo_path.exists() {
            user_tool_rules.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                cargo_path.display()
            ));
        }
        if rustup_path.exists() {
            user_tool_rules.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                rustup_path.display()
            ));
        }

        // Build temp directory rules
        let temp_rules = if self.no_temp_files {
            // No temp directory access - high security mode
            String::new()
        } else {
            // Normal mode: allow temp directories for tools that need them
            "(allow file-write* (subpath \"/private/tmp\"))\n\
             (allow file-write* (subpath \"/private/var/folders\"))\n"
                .to_string()
        };

        format!(
            r#"(version 1)
(deny default)
(allow process*)
(allow signal)
(allow sysctl-read)
(allow file-read*)
{user_tool_rules}{scope_rules}(allow file-write* (subpath "{working_dir}"))
{temp_rules}(allow file-write* (literal "/dev/null"))
(allow file-write* (literal "/dev/tty"))
(allow file-write* (literal "/dev/zero"))
(allow network*)
(allow mach-lookup)
(allow ipc-posix-shm*)
"#,
            working_dir = wd_str,
            user_tool_rules = user_tool_rules,
            scope_rules = scope_rules,
            temp_rules = temp_rules,
        )
    }
}

/// Helper: Normalize a path lexically (without filesystem access).
fn normalize_path_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.last().is_some_and(|c| *c != Component::RootDir) {
                    stack.pop();
                }
            }
            c => stack.push(c),
        }
    }

    stack.iter().collect()
}

// ------------------------------------------------------------------------------------------------
// Static/Global helpers for Prerequisite Checks (Pre-Initialization)
// These do not require a Sandbox instance.
// ------------------------------------------------------------------------------------------------

/// Check if the platform's sandboxing prerequisites are met.
pub fn check_sandbox_prerequisites() -> Result<(), SandboxError> {
    #[cfg(target_os = "linux")]
    {
        check_landlock_available()
    }

    #[cfg(target_os = "macos")]
    {
        check_macos_sandbox_available()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(SandboxError::UnsupportedOs(
            std::env::consts::OS.to_string(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn check_landlock_available() -> Result<(), SandboxError> {
    use std::fs;
    let landlock_abi_path = "/sys/kernel/security/lsm";
    match fs::read_to_string(landlock_abi_path) {
        Ok(content) => {
            if content.contains("landlock") {
                Ok(())
            } else {
                Err(SandboxError::LandlockNotAvailable)
            }
        }
        Err(_) => check_kernel_version_for_landlock(),
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
        "Landlock requires Linux kernel 5.13 or newer. Current: {}.",
        version_str.trim()
    )))
}

#[cfg(target_os = "macos")]
fn check_macos_sandbox_available() -> Result<(), SandboxError> {
    use std::process::Command;
    let result = Command::new("which").arg("sandbox-exec").output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(SandboxError::MacOSSandboxNotAvailable),
    }
}

#[cfg(target_os = "macos")]
pub fn test_sandbox_exec_available() -> Result<(), SandboxError> {
    use std::process::Command;
    let test_profile = "(version 1)(allow default)";
    let result = Command::new("sandbox-exec")
        .args(["-p", test_profile, "/usr/bin/true"])
        .output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Operation not permitted")
                || stderr.contains("sandbox_apply")
                || output.status.code() == Some(71)
            {
                Err(SandboxError::NestedSandboxDetected)
            } else {
                tracing::debug!("sandbox-exec test failed: {}", stderr);
                Err(SandboxError::NestedSandboxDetected)
            }
        }
        Err(e) => {
            tracing::debug!("sandbox-exec exec failed: {}", e);
            Err(SandboxError::MacOSSandboxNotAvailable)
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn test_sandbox_exec_available() -> Result<(), SandboxError> {
    Ok(())
}

pub fn exit_with_sandbox_error(error: &SandboxError) -> ! {
    eprintln!("\n❌ SECURITY ERROR: Cannot start MCP server\n");
    eprintln!("Reason: {}\n", error);
    std::process::exit(1);
}

// Global legacy helpers are REMOVED (initialize_sandbox_scope, get_sandbox_scope, etc.)
// They are replaced by Sandbox::new() and Sandbox methods.

/// Apply Landlock sandbox restrictions to the current process.
#[cfg(target_os = "linux")]
pub fn enforce_landlock_sandbox(scopes: &[PathBuf]) -> Result<()> {
    use anyhow::Context;
    use landlock::{
        ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };

    let abi = ABI::V3;
    let access_all = AccessFs::from_all(abi);
    let access_read = AccessFs::from_read(abi);

    let mut ruleset = Ruleset::default()
        .handle_access(access_all)
        .context("Failed to create Landlock ruleset")?
        .create()
        .context("Failed to create Landlock ruleset instance")?;

    for scope in scopes {
        ruleset = ruleset
            .add_rule(PathBeneath::new(
                PathFd::new(scope).context("Failed to open sandbox scope for Landlock")?,
                access_all,
            ))
            .context("Failed to add Landlock rule for sandbox scope")?;
    }

    // Allow read access to system directories
    let system_paths = ["/usr", "/bin", "/etc", "/lib", "/lib64", "/proc", "/dev"];
    for path in &system_paths {
        let path_obj = Path::new(path);
        if path_obj.exists() {
            if let Ok(fd) = PathFd::new(path_obj) {
                if let Err(e) = (&mut ruleset).add_rule(PathBeneath::new(fd, access_read)) {
                    tracing::debug!("Could not add Landlock rule for {}: {:?}", path, e);
                }
            }
        }
    }

    let status = ruleset
        .restrict_self()
        .context("Failed to apply Landlock restrictions")?;

    tracing::info!(
        "Landlock sandbox enforced for scopes: {:?} (status: {:?})",
        scopes,
        status
    );

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn enforce_landlock_sandbox(_scopes: &[PathBuf]) -> Result<()> {
    Ok(())
}
