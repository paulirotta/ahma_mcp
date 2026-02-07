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
        "Path '{path:?}' is outside the sandbox root{} (this usually means your MCP session is scoped to a different workspace root; reconnect from the intended workspace or use a multi-root workspace)",
        format_scopes(.scopes)
    )]
    PathOutsideSandbox { path: PathBuf, scopes: Vec<PathBuf> },

    #[error("Landlock is not available on this system (requires Linux kernel 5.13+)")]
    LandlockNotAvailable,

    #[error("macOS sandbox-exec is not available")]
    MacOSSandboxNotAvailable,

    #[error("Unsupported operating system: {0}")]
    UnsupportedOs(String),

    #[error("Failed to canonicalize path '{path:?}': {reason}")]
    CanonicalizationFailed { path: PathBuf, reason: String },

    #[error("Sandbox prerequisite check failed: {0}")]
    PrerequisiteFailed(String),

    #[error("Path '{path:?}' is blocked by high-security mode (no-temp-files)")]
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
pub struct Sandbox {
    scopes: std::sync::RwLock<Vec<PathBuf>>,
    mode: SandboxMode,
    no_temp_files: bool,
}

impl Clone for Sandbox {
    fn clone(&self) -> Self {
        Self {
            scopes: std::sync::RwLock::new(self.scopes.read().unwrap().clone()),
            mode: self.mode,
            no_temp_files: self.no_temp_files,
        }
    }
}

impl std::fmt::Debug for Sandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbox")
            .field("scopes", &self.scopes.read().unwrap())
            .field("mode", &self.mode)
            .field("no_temp_files", &self.no_temp_files)
            .finish()
    }
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
            scopes: std::sync::RwLock::new(canonicalized),
            mode,
            no_temp_files,
        })
    }

    /// Create a sandbox in Test mode (bypasses restrictions).
    pub fn new_test() -> Self {
        Self {
            scopes: std::sync::RwLock::new(vec![PathBuf::from("/")]),
            mode: SandboxMode::Test,
            no_temp_files: false,
        }
    }

    /// Update the sandbox scopes.
    pub fn update_scopes(&self, scopes: Vec<PathBuf>) -> Result<()> {
        let mut canonicalized = Vec::with_capacity(scopes.len());
        for scope in scopes {
            let canonical = std::fs::canonicalize(&scope).map_err(|e| {
                anyhow!(
                    "Failed to canonicalize sandbox scope '{}': {}",
                    scope.display(),
                    e
                )
            })?;
            canonicalized.push(canonical);
        }

        let mut current_scopes = self.scopes.write().unwrap();
        *current_scopes = canonicalized;
        Ok(())
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
    pub fn scopes(&self) -> ScopesGuard<'_> {
        ScopesGuard(self.scopes.read().unwrap())
    }

    /// Check if a path is within any of the sandbox scopes.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        let scopes = self.scopes();

        if self.should_bypass_validation(&scopes) {
            return self.resolve_test_path(path);
        }

        let canonical = self.resolve_path(path, &scopes)?;

        if self.is_path_allowed(&canonical, &scopes) {
            self.check_security_policies(path, &canonical)?;
            Ok(canonical)
        } else {
            Err(SandboxError::PathOutsideSandbox {
                path: path.to_path_buf(),
                scopes: scopes.to_vec(),
            }
            .into())
        }
    }

    fn should_bypass_validation(&self, scopes: &[PathBuf]) -> bool {
        self.mode == SandboxMode::Test
            && (scopes.is_empty() || scopes.iter().any(|s| s == Path::new("/")))
    }

    fn resolve_test_path(&self, path: &Path) -> Result<PathBuf> {
        std::fs::canonicalize(path).or_else(|_| Ok(path.to_path_buf()))
    }

    fn resolve_path(&self, path: &Path, scopes: &[PathBuf]) -> Result<PathBuf> {
        let first_scope = scopes
            .first()
            .ok_or_else(|| anyhow!("No sandbox scopes configured"))?;

        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            first_scope.join(path)
        };

        Ok(match std::fs::canonicalize(&full_path) {
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
        })
    }

    fn is_path_allowed(&self, canonical: &Path, scopes: &[PathBuf]) -> bool {
        scopes.iter().any(|scope| canonical.starts_with(scope))
    }

    fn check_security_policies(&self, original_path: &Path, canonical: &Path) -> Result<()> {
        if self.no_temp_files {
            let path_str = canonical.to_string_lossy();
            if path_str.starts_with("/tmp")
                || path_str.starts_with("/var/folders")
                || path_str.starts_with("/private/tmp")
                || path_str.starts_with("/private/var/folders")
                || path_str.starts_with("/dev")
            {
                return Err(SandboxError::HighSecurityViolation {
                    path: original_path.to_path_buf(),
                }
                .into());
            }
        }
        Ok(())
    }

    /// Create a sandboxed tokio process Command.
    pub fn create_command(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
    ) -> Result<tokio::process::Command> {
        if self.mode == SandboxMode::Test {
            return Ok(self.base_command(program, args, working_dir));
        }

        self.create_platform_sandboxed_command(program, args, working_dir)
    }

    fn base_command(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
    ) -> tokio::process::Command {
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
        cmd
    }

    fn create_platform_sandboxed_command(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
    ) -> Result<tokio::process::Command> {
        #[cfg(target_os = "linux")]
        {
            // On Linux, Landlock is applied at process level, so commands run directly
            Ok(self.base_command(program, args, working_dir))
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, wrap each command with sandbox-exec
            let mut full_command = vec![program.to_string()];
            full_command.extend(args.iter().cloned());

            let (sandbox_program, sandbox_args) =
                self.build_macos_sandbox_command(&full_command, working_dir)?;

            Ok(self.base_command(&sandbox_program, &sandbox_args, working_dir))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Ok(self.base_command(program, args, working_dir))
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
        let scope_rules = self.get_macos_scope_rules();
        let user_tool_rules = self.get_macos_user_tool_rules();
        let temp_rules = self.get_macos_temp_rules();

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

    #[cfg(target_os = "macos")]
    fn get_macos_scope_rules(&self) -> String {
        let mut rules = String::new();
        for scope in self.scopes.read().unwrap().iter() {
            rules.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                scope.display()
            ));
        }
        rules
    }

    #[cfg(target_os = "macos")]
    fn get_macos_user_tool_rules(&self) -> String {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/Users/Shared".to_string());
        let home_path = std::path::Path::new(&home_dir);
        let mut rules = String::new();

        let tool_paths = [".cargo", ".rustup"];
        for tool in &tool_paths {
            let path = home_path.join(tool);
            if path.exists() {
                rules.push_str(&format!(
                    "(allow file-read* (subpath \"{}\"))\n",
                    path.display()
                ));
            }
        }
        rules
    }

    #[cfg(target_os = "macos")]
    fn get_macos_temp_rules(&self) -> String {
        if self.no_temp_files {
            String::new()
        } else {
            "(allow file-write* (subpath \"/private/tmp\"))\n\
             (allow file-write* (subpath \"/private/var/folders\"))\n"
                .to_string()
        }
    }
}

/// Helper: Normalize a path lexically (without filesystem access).
pub fn normalize_path_lexically(path: &Path) -> PathBuf {
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
pub fn enforce_landlock_sandbox(scopes: &[PathBuf], no_temp_files: bool) -> Result<()> {
    use anyhow::Context;
    use landlock::{ABI, AccessFs, PathBeneath, PathFd, Ruleset, RulesetCreatedAttr};

    let abi = ABI::V3;
    let access_all = AccessFs::from_all(abi);
    let access_read = AccessFs::from_read(abi);

    let mut ruleset = Ruleset::default()
        .handle_access(access_all)
        .context("Failed to create Landlock ruleset")?
        .create()
        .context("Failed to create Landlock ruleset instance")?;

    // Add sandbox scopes
    for scope in scopes {
        ruleset = ruleset
            .add_rule(PathBeneath::new(
                PathFd::new(scope).context("Failed to open sandbox scope for Landlock")?,
                access_all,
            ))
            .context("Failed to add Landlock rule for sandbox scope")?;
    }

    add_landlock_system_rules(&mut ruleset, access_read)?;
    add_landlock_home_tool_rules(&mut ruleset, access_read)?;

    if !no_temp_files {
        add_landlock_temp_rules(&mut ruleset, access_all)?;
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

#[cfg(target_os = "linux")]
fn add_landlock_system_rules(
    ruleset: &mut landlock::RulesetCreated,
    access_read: enumflags2::BitFlags<landlock::AccessFs>,
) -> Result<()> {
    use landlock::{AccessFs, PathBeneath, PathFd, RulesetCreatedAttr};
    let system_paths = [
        "/usr", "/bin", "/sbin", "/etc", "/lib", "/lib64", "/proc", "/dev", "/sys",
    ];
    let access_read_execute = access_read | AccessFs::Execute;
    for path in &system_paths {
        let path_obj = std::path::Path::new(path);
        if path_obj.exists() {
            if let Ok(fd) = PathFd::new(path_obj) {
                let _ = ruleset.add_rule(PathBeneath::new(fd, access_read_execute));
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn add_landlock_home_tool_rules(
    ruleset: &mut landlock::RulesetCreated,
    access_read: enumflags2::BitFlags<landlock::AccessFs>,
) -> Result<()> {
    use landlock::{PathBeneath, PathFd, RulesetCreatedAttr};
    if let Ok(home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(&home);
        let tool_paths = [".cargo", ".rustup", ".nvm", ".npm", ".go", ".cache"];
        for tool in &tool_paths {
            let path = home_path.join(tool);
            if path.exists() {
                if let Ok(fd) = PathFd::new(&path) {
                    let _ = ruleset.add_rule(PathBeneath::new(fd, access_read));
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn add_landlock_temp_rules(
    ruleset: &mut landlock::RulesetCreated,
    access_all: enumflags2::BitFlags<landlock::AccessFs>,
) -> Result<()> {
    use landlock::{PathBeneath, PathFd, RulesetCreatedAttr};
    let tmp_path = std::path::Path::new("/tmp");
    if tmp_path.exists() {
        if let Ok(fd) = PathFd::new(tmp_path) {
            let _ = ruleset.add_rule(PathBeneath::new(fd, access_all));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn enforce_landlock_sandbox(_scopes: &[PathBuf], _no_temp_files: bool) -> Result<()> {
    Ok(())
}

/// A guard that holds a read lock on the sandbox scopes.
pub struct ScopesGuard<'a>(std::sync::RwLockReadGuard<'a, Vec<PathBuf>>);

impl std::ops::Deref for ScopesGuard<'_> {
    type Target = [PathBuf];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for ScopesGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
