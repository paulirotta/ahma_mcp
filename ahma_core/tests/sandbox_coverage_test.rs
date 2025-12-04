//! Sandbox Module Coverage Tests
//!
//! These tests focus on improving coverage for sandbox.rs, covering:
//! - SandboxError Display variants
//! - normalize_path_lexically edge cases
//! - validate_path_in_sandbox paths
//! - SandboxConfig constructors
//! - build_sandboxed_command error paths
//! - Platform-specific sandbox functions

use ahma_core::sandbox::{
    self, SandboxConfig, SandboxError, build_sandboxed_command, check_sandbox_prerequisites,
    create_sandboxed_command, create_sandboxed_shell_command, enable_test_mode, get_sandbox_scope,
    is_test_mode,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to ensure test mode is enabled for sandbox tests
fn setup_test_mode() {
    enable_test_mode();
}

// ============================================================================
// SandboxError Display Tests
// ============================================================================

#[test]
fn test_sandbox_error_already_initialized_display() {
    let err = SandboxError::AlreadyInitialized;
    let msg = format!("{}", err);
    assert!(msg.contains("already initialized"));
}

#[test]
fn test_sandbox_error_not_initialized_display() {
    let err = SandboxError::NotInitialized;
    let msg = format!("{}", err);
    assert!(msg.contains("not initialized"));
}

#[test]
fn test_sandbox_error_path_outside_sandbox_display() {
    let err = SandboxError::PathOutsideSandbox {
        path: PathBuf::from("/etc/passwd"),
        scope: PathBuf::from("/home/user/project"),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("/etc/passwd"));
    assert!(msg.contains("/home/user/project"));
    assert!(msg.contains("outside"));
}

#[test]
fn test_sandbox_error_landlock_not_available_display() {
    let err = SandboxError::LandlockNotAvailable;
    let msg = format!("{}", err);
    assert!(msg.contains("Landlock"));
    assert!(msg.contains("5.13"));
}

#[test]
fn test_sandbox_error_macos_sandbox_not_available_display() {
    let err = SandboxError::MacOSSandboxNotAvailable;
    let msg = format!("{}", err);
    assert!(msg.contains("sandbox-exec"));
}

#[test]
fn test_sandbox_error_unsupported_os_display() {
    let err = SandboxError::UnsupportedOs("windows".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("Unsupported"));
    assert!(msg.contains("windows"));
}

#[test]
fn test_sandbox_error_canonicalization_failed_display() {
    let err = SandboxError::CanonicalizationFailed {
        path: PathBuf::from("/nonexistent/path"),
        reason: "No such file or directory".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("/nonexistent/path"));
    assert!(msg.contains("canonicalize"));
}

#[test]
fn test_sandbox_error_prerequisite_failed_display() {
    let err = SandboxError::PrerequisiteFailed("Missing kernel module".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("prerequisite"));
    assert!(msg.contains("Missing kernel module"));
}

// ============================================================================
// SandboxError Debug Tests
// ============================================================================

#[test]
fn test_sandbox_error_debug_format() {
    let err = SandboxError::AlreadyInitialized;
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("AlreadyInitialized"));

    let err2 = SandboxError::PathOutsideSandbox {
        path: PathBuf::from("/etc"),
        scope: PathBuf::from("/home"),
    };
    let debug_str2 = format!("{:?}", err2);
    assert!(debug_str2.contains("PathOutsideSandbox"));
}

// ============================================================================
// SandboxConfig Tests
// ============================================================================

#[test]
fn test_sandbox_config_new() {
    let scope = PathBuf::from("/test/scope");
    let config = SandboxConfig::new(scope.clone());
    assert_eq!(config.scope, scope);
    assert!(config.strict);
}

#[test]
fn test_sandbox_config_from_cwd() {
    // from_cwd should succeed in any valid directory
    let result = SandboxConfig::from_cwd();
    assert!(result.is_ok());

    let config = result.unwrap();
    assert!(config.scope.is_absolute());
    assert!(config.strict);
}

#[test]
fn test_sandbox_config_debug() {
    let config = SandboxConfig::new(PathBuf::from("/test"));
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("SandboxConfig"));
    assert!(debug_str.contains("/test"));
}

#[test]
fn test_sandbox_config_clone() {
    let config1 = SandboxConfig::new(PathBuf::from("/test"));
    let config2 = config1.clone();
    assert_eq!(config1.scope, config2.scope);
    assert_eq!(config1.strict, config2.strict);
}

// ============================================================================
// Test Mode Tests
// ============================================================================

#[test]
fn test_enable_test_mode() {
    enable_test_mode();
    assert!(is_test_mode());
}

#[test]
fn test_is_test_mode_returns_true_after_enable() {
    enable_test_mode();
    // Should consistently return true
    assert!(is_test_mode());
    assert!(is_test_mode());
}

// ============================================================================
// build_sandboxed_command Tests
// ============================================================================

#[test]
fn test_build_sandboxed_command_empty_command() {
    let temp = TempDir::new().unwrap();
    let result = build_sandboxed_command(&[], temp.path(), temp.path());
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty command"));
}

#[test]
fn test_build_sandboxed_command_single_command() {
    let temp = TempDir::new().unwrap();
    let command = vec!["echo".to_string()];
    let result = build_sandboxed_command(&command, temp.path(), temp.path());
    assert!(result.is_ok());

    let (program, args) = result.unwrap();
    // On Linux, returns command as-is; on macOS, wraps with sandbox-exec
    #[cfg(target_os = "linux")]
    {
        assert_eq!(program, "echo");
        assert!(args.is_empty());
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(program, "sandbox-exec");
        assert!(args.contains(&"echo".to_string()));
    }
}

#[test]
fn test_build_sandboxed_command_with_args() {
    let temp = TempDir::new().unwrap();
    let command = vec!["ls".to_string(), "-la".to_string(), "/tmp".to_string()];
    let result = build_sandboxed_command(&command, temp.path(), temp.path());
    assert!(result.is_ok());

    let (program, args) = result.unwrap();
    #[cfg(target_os = "linux")]
    {
        assert_eq!(program, "ls");
        assert_eq!(args, vec!["-la", "/tmp"]);
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(program, "sandbox-exec");
        assert!(args.contains(&"ls".to_string()));
        assert!(args.contains(&"-la".to_string()));
    }
}

// ============================================================================
// create_sandboxed_command Tests (with test mode)
// ============================================================================

#[tokio::test]
async fn test_create_sandboxed_command_in_test_mode() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let result = create_sandboxed_command("echo", &["hello".to_string()], temp.path());
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_sandboxed_command_with_multiple_args() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let args = vec!["-n".to_string(), "test output".to_string()];
    let result = create_sandboxed_command("echo", &args, temp.path());
    assert!(result.is_ok());
}

// ============================================================================
// create_sandboxed_shell_command Tests (with test mode)
// ============================================================================

#[tokio::test]
async fn test_create_sandboxed_shell_command_in_test_mode() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let result = create_sandboxed_shell_command("/bin/sh", "echo hello", temp.path());
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_sandboxed_shell_command_with_complex_script() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let script = "for i in 1 2 3; do echo $i; done";
    let result = create_sandboxed_shell_command("/bin/sh", script, temp.path());
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_sandboxed_shell_command_bash() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let result = create_sandboxed_shell_command("/bin/bash", "pwd", temp.path());
    assert!(result.is_ok());
}

// ============================================================================
// check_sandbox_prerequisites Tests
// ============================================================================

#[test]
fn test_check_sandbox_prerequisites() {
    // This should return Ok on macOS and Linux, Err on other platforms
    let result = check_sandbox_prerequisites();

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // On Linux, may fail if Landlock not available; on macOS should succeed
        #[cfg(target_os = "macos")]
        assert!(result.is_ok());

        // On Linux, result depends on kernel version
        #[cfg(target_os = "linux")]
        {
            // Just check it returns a result (may be Ok or Err depending on kernel)
            let _ = result;
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        assert!(result.is_err());
        if let Err(e) = result {
            match e {
                SandboxError::UnsupportedOs(_) => {}
                _ => panic!("Expected UnsupportedOs error"),
            }
        }
    }
}

// ============================================================================
// macOS-specific tests
// ============================================================================

#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;

    #[test]
    fn test_build_macos_sandbox_command_structure() {
        let temp = TempDir::new().unwrap();
        let command = vec!["cat".to_string(), "file.txt".to_string()];

        let result = build_sandboxed_command(&command, temp.path(), temp.path());
        assert!(result.is_ok());

        let (program, args) = result.unwrap();
        assert_eq!(program, "sandbox-exec");
        assert!(args.len() >= 3); // -p, profile, cat, file.txt
        assert_eq!(args[0], "-p");
        // Profile string should be present
        assert!(args[1].contains("version 1"));
        assert!(args[1].contains("deny default"));
    }

    #[test]
    fn test_seatbelt_profile_contains_scope() {
        let temp = TempDir::new().unwrap();
        let command = vec!["test".to_string()];

        let result = build_sandboxed_command(&command, temp.path(), temp.path());
        let (_, args) = result.unwrap();

        let profile = &args[1];
        // Profile should contain the sandbox scope path
        let scope_str = temp.path().to_string_lossy();
        assert!(profile.contains(&*scope_str));
    }

    #[test]
    fn test_seatbelt_profile_allows_tmp() {
        let temp = TempDir::new().unwrap();
        let command = vec!["test".to_string()];

        let result = build_sandboxed_command(&command, temp.path(), temp.path());
        let (_, args) = result.unwrap();

        let profile = &args[1];
        // Profile should allow /private/tmp
        assert!(profile.contains("/private/tmp"));
    }

    #[test]
    fn test_seatbelt_profile_allows_var_folders() {
        let temp = TempDir::new().unwrap();
        let command = vec!["test".to_string()];

        let result = build_sandboxed_command(&command, temp.path(), temp.path());
        let (_, args) = result.unwrap();

        let profile = &args[1];
        // Profile should use /private/var/folders (real path, not symlink)
        assert!(profile.contains("/private/var/folders"));
    }
}

// ============================================================================
// Integration Tests with Actual Command Execution
// ============================================================================

#[tokio::test]
async fn test_sandboxed_command_executes_echo() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let mut cmd = create_sandboxed_command("echo", &["test_output".to_string()], temp.path())
        .expect("Failed to create command");

    let output = cmd.output().await.expect("Failed to execute command");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_output"));
}

#[tokio::test]
async fn test_sandboxed_shell_command_executes_pwd() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let mut cmd = create_sandboxed_shell_command("/bin/sh", "pwd", temp.path())
        .expect("Failed to create command");

    let output = cmd.output().await.expect("Failed to execute command");
    assert!(output.status.success());
}

#[tokio::test]
async fn test_sandboxed_command_returns_exit_code() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let mut cmd = create_sandboxed_shell_command("/bin/sh", "exit 42", temp.path())
        .expect("Failed to create command");

    let output = cmd.output().await.expect("Failed to execute command");
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(42));
}

#[tokio::test]
async fn test_sandboxed_command_captures_stderr() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();

    let mut cmd = create_sandboxed_shell_command("/bin/sh", "echo error_message >&2", temp.path())
        .expect("Failed to create command");

    let output = cmd.output().await.expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error_message"));
}

// ============================================================================
// get_sandbox_scope Tests
// ============================================================================

#[test]
fn test_get_sandbox_scope_after_test_mode() {
    setup_test_mode();
    // After enabling test mode, sandbox scope may or may not be set
    // depending on whether any sandboxed commands have been created
    let _scope = get_sandbox_scope();
    // Just verify it doesn't panic
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_sandbox_config_with_relative_path_in_scope() {
    // SandboxConfig::new accepts any path (doesn't validate)
    let config = SandboxConfig::new(PathBuf::from("relative/path"));
    assert_eq!(config.scope, PathBuf::from("relative/path"));
}

#[test]
fn test_sandbox_config_with_empty_scope() {
    let config = SandboxConfig::new(PathBuf::from(""));
    assert_eq!(config.scope, PathBuf::from(""));
}

#[test]
fn test_sandbox_error_is_send_sync() {
    // Verify SandboxError can be sent across threads
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SandboxError>();
}

// ============================================================================
// normalize_path_lexically Edge Cases (via integration)
// ============================================================================

// Note: normalize_path_lexically is private, so we test it indirectly
// through validate_path_in_sandbox or build_sandboxed_command

#[test]
fn test_build_command_with_path_containing_dotdot() {
    let temp = TempDir::new().unwrap();
    // Create a working directory with .. in the path
    let working_dir = temp.path().join("subdir").join("..").join("other");
    std::fs::create_dir_all(&working_dir).ok();

    let command = vec!["ls".to_string()];
    let result = build_sandboxed_command(&command, &working_dir, temp.path());
    // Should succeed - path normalization handles ..
    assert!(result.is_ok());
}

#[test]
fn test_build_command_with_path_containing_dot() {
    let temp = TempDir::new().unwrap();
    let working_dir = temp.path().join(".").join("subdir");
    std::fs::create_dir_all(&working_dir).ok();

    let command = vec!["ls".to_string()];
    let result = build_sandboxed_command(&command, &working_dir, temp.path());
    assert!(result.is_ok());
}

// ============================================================================
// Special Characters in Paths
// ============================================================================

#[test]
fn test_sandbox_command_with_spaces_in_path() {
    let temp = TempDir::new().unwrap();
    let dir_with_spaces = temp.path().join("path with spaces");
    std::fs::create_dir_all(&dir_with_spaces).unwrap();

    let command = vec!["ls".to_string()];
    let result = build_sandboxed_command(&command, &dir_with_spaces, temp.path());
    assert!(result.is_ok());
}

#[test]
fn test_sandbox_command_with_unicode_in_path() {
    let temp = TempDir::new().unwrap();
    let unicode_dir = temp.path().join("目录_дир_📁");
    std::fs::create_dir_all(&unicode_dir).unwrap();

    let command = vec!["ls".to_string()];
    let result = build_sandboxed_command(&command, &unicode_dir, temp.path());
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sandboxed_command_in_unicode_directory() {
    setup_test_mode();
    let temp = TempDir::new().unwrap();
    let unicode_dir = temp.path().join("тест_директория");
    std::fs::create_dir_all(&unicode_dir).unwrap();

    let mut cmd =
        create_sandboxed_command("pwd", &[], &unicode_dir).expect("Failed to create command");

    let output = cmd.output().await.expect("Failed to execute command");
    assert!(output.status.success());
}

// ============================================================================
// Long Path Tests
// ============================================================================

#[test]
fn test_sandbox_command_with_long_path() {
    let temp = TempDir::new().unwrap();
    // Create a deeply nested path
    let mut long_path = temp.path().to_path_buf();
    for i in 0..20 {
        long_path = long_path.join(format!("level_{}", i));
    }
    std::fs::create_dir_all(&long_path).unwrap();

    let command = vec!["ls".to_string()];
    let result = build_sandboxed_command(&command, &long_path, temp.path());
    assert!(result.is_ok());
}

// ============================================================================
// enforce_landlock_sandbox Tests (Linux only, no-op on other platforms)
// ============================================================================

#[test]
fn test_enforce_landlock_sandbox_noop_on_non_linux() {
    #[cfg(not(target_os = "linux"))]
    {
        let temp = TempDir::new().unwrap();
        let result = sandbox::enforce_landlock_sandbox(temp.path());
        // On non-Linux, this is a no-op and should succeed
        assert!(result.is_ok());
    }
}
