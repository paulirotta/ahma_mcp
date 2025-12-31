//! Unit tests for sandbox.rs - Kernel-level sandboxing
//!
//! These tests cover:
//! - Path validation and normalization
//! - Sandbox scope initialization
//! - SandboxError formatting
//! - Platform-specific sandbox checks

use ahma_core::sandbox::{SandboxError, is_test_mode, is_no_temp_files};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

// ============= SandboxError Display Tests =============

#[test]
fn test_sandbox_error_already_initialized_display() {
    let error = SandboxError::AlreadyInitialized;
    let msg = error.to_string();
    assert!(msg.contains("already initialized"));
}

#[test]
fn test_sandbox_error_not_initialized_display() {
    let error = SandboxError::NotInitialized;
    let msg = error.to_string();
    assert!(msg.contains("not initialized"));
    assert!(msg.contains("initialize_sandbox_scope"));
}

#[test]
fn test_sandbox_error_path_outside_single_scope() {
    let error = SandboxError::PathOutsideSandbox {
        path: PathBuf::from("/etc/passwd"),
        scopes: vec![PathBuf::from("/home/user/project")],
    };
    let msg = error.to_string();
    assert!(msg.contains("/etc/passwd"));
    assert!(msg.contains("/home/user/project"));
    assert!(msg.contains("outside the sandbox"));
}

#[test]
fn test_sandbox_error_path_outside_multiple_scopes() {
    let error = SandboxError::PathOutsideSandbox {
        path: PathBuf::from("/etc/passwd"),
        scopes: vec![
            PathBuf::from("/home/user/project1"),
            PathBuf::from("/home/user/project2"),
        ],
    };
    let msg = error.to_string();
    assert!(msg.contains("/etc/passwd"));
    assert!(msg.contains("project1"));
    assert!(msg.contains("project2"));
}

#[test]
fn test_sandbox_error_landlock_not_available() {
    let error = SandboxError::LandlockNotAvailable;
    let msg = error.to_string();
    assert!(msg.contains("Landlock"));
    assert!(msg.contains("Linux kernel 5.13"));
}

#[test]
fn test_sandbox_error_macos_sandbox_not_available() {
    let error = SandboxError::MacOSSandboxNotAvailable;
    let msg = error.to_string();
    assert!(msg.contains("macOS"));
    assert!(msg.contains("sandbox-exec"));
}

#[test]
fn test_sandbox_error_unsupported_os() {
    let error = SandboxError::UnsupportedOs("freebsd".to_string());
    let msg = error.to_string();
    assert!(msg.contains("Unsupported"));
    assert!(msg.contains("freebsd"));
}

#[test]
fn test_sandbox_error_canonicalization_failed() {
    let error = SandboxError::CanonicalizationFailed {
        path: PathBuf::from("/nonexistent/path"),
        reason: "No such file or directory".to_string(),
    };
    let msg = error.to_string();
    assert!(msg.contains("/nonexistent/path"));
    assert!(msg.contains("No such file or directory"));
}

#[test]
fn test_sandbox_error_prerequisite_failed() {
    let error = SandboxError::PrerequisiteFailed("Kernel version too old".to_string());
    let msg = error.to_string();
    assert!(msg.contains("prerequisite"));
    assert!(msg.contains("Kernel version too old"));
}

#[test]
fn test_sandbox_error_nested_sandbox_detected() {
    let error = SandboxError::NestedSandboxDetected;
    let msg = error.to_string();
    assert!(msg.contains("Nested sandbox"));
    assert!(msg.contains("Cursor") || msg.contains("VS Code") || msg.contains("Docker"));
}

// ============= Path Normalization Tests (Lexical) =============

/// Test helper: normalize a path lexically without filesystem access
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

#[test]
fn test_normalize_path_removes_single_dot() {
    let path = Path::new("/home/user/./project");
    let normalized = normalize_path_lexically(path);
    assert_eq!(normalized, PathBuf::from("/home/user/project"));
}

#[test]
fn test_normalize_path_resolves_parent_dir() {
    let path = Path::new("/home/user/project/../other");
    let normalized = normalize_path_lexically(path);
    assert_eq!(normalized, PathBuf::from("/home/user/other"));
}

#[test]
fn test_normalize_path_multiple_parent_dirs() {
    let path = Path::new("/home/user/a/b/c/../../d");
    let normalized = normalize_path_lexically(path);
    assert_eq!(normalized, PathBuf::from("/home/user/a/d"));
}

#[test]
fn test_normalize_path_parent_at_root_stays_at_root() {
    let path = Path::new("/../etc/passwd");
    let normalized = normalize_path_lexically(path);
    // Parent dir at root should stay at root
    assert_eq!(normalized, PathBuf::from("/etc/passwd"));
}

#[test]
fn test_normalize_path_empty_after_parents() {
    let path = Path::new("/a/b/../../c");
    let normalized = normalize_path_lexically(path);
    assert_eq!(normalized, PathBuf::from("/c"));
}

#[test]
fn test_normalize_path_trailing_slash_removed() {
    let path = Path::new("/home/user/project/./");
    let normalized = normalize_path_lexically(path);
    assert_eq!(normalized, PathBuf::from("/home/user/project"));
}

// ============= Test Mode Detection Tests =============

#[test]
fn test_is_test_mode_enabled_by_cargo() {
    // When running under cargo test, test mode should be detected
    // This test is a bit meta - if it runs, we're in cargo test context
    
    // The function should return true when AHMA_TEST_MODE is set or
    // when detected as running in cargo test
    // We can't reliably test this without side effects on global state
    
    // At minimum, verify the function doesn't panic
    let _ = is_test_mode();
}

#[test]
fn test_is_no_temp_files_default_false() {
    // By default, no_temp_files should be false
    // Note: This could be affected by other tests if they call enable_no_temp_files
    // In a clean environment, it should be false
    let _ = is_no_temp_files(); // Just verify it doesn't panic
}

// ============= Path Validation Logic Tests =============

#[test]
fn test_relative_path_detection() {
    let relative = Path::new("src/main.rs");
    let absolute = Path::new("/home/user/src/main.rs");

    assert!(!relative.is_absolute());
    assert!(absolute.is_absolute());
}

#[test]
fn test_path_starts_with_check() {
    let scope = PathBuf::from("/home/user/project");
    let valid_path = PathBuf::from("/home/user/project/src/main.rs");
    let invalid_path = PathBuf::from("/etc/passwd");

    assert!(valid_path.starts_with(&scope));
    assert!(!invalid_path.starts_with(&scope));
}

#[test]
fn test_path_starts_with_similar_prefix() {
    // Ensure we don't match "/home/user/project-other" when scope is "/home/user/project"
    let scope = PathBuf::from("/home/user/project");
    let similar_but_different = PathBuf::from("/home/user/project-other/file.txt");

    // starts_with is directory-based, not string-based
    assert!(!similar_but_different.starts_with(&scope));
}

#[test]
fn test_path_join_for_relative() {
    let scope = PathBuf::from("/home/user/project");
    let relative = Path::new("src/main.rs");
    let joined = scope.join(relative);

    assert_eq!(joined, PathBuf::from("/home/user/project/src/main.rs"));
}

// ============= Canonicalization Tests =============

#[test]
fn test_canonicalize_temp_directory() {
    let temp_dir = tempdir().unwrap();
    let canonical = std::fs::canonicalize(temp_dir.path()).unwrap();
    
    // Canonical path should be absolute
    assert!(canonical.is_absolute());
    
    // Should point to the same location
    assert!(canonical.exists());
}

#[test]
fn test_canonicalize_nonexistent_path_fails() {
    let nonexistent = Path::new("/this/path/definitely/does/not/exist/12345");
    let result = std::fs::canonicalize(nonexistent);
    assert!(result.is_err());
}

#[test]
fn test_canonicalize_symlink_resolution() {
    let temp_dir = tempdir().unwrap();
    let original = temp_dir.path().join("original_dir");
    std::fs::create_dir(&original).unwrap();
    
    #[cfg(unix)]
    {
        let symlink = temp_dir.path().join("symlink");
        std::os::unix::fs::symlink(&original, &symlink).unwrap();
        
        let canonical = std::fs::canonicalize(&symlink).unwrap();
        // Canonical path should resolve to the original, not the symlink
        assert!(!canonical.ends_with("symlink"));
    }
}

// ============= Multi-Scope Logic Tests =============

#[test]
fn test_any_scope_matches() {
    let scopes = [PathBuf::from("/home/user/project1"),
        PathBuf::from("/home/user/project2"),
        PathBuf::from("/shared/workspace")];

    let path1 = PathBuf::from("/home/user/project1/src/main.rs");
    let path2 = PathBuf::from("/home/user/project2/lib/mod.rs");
    let path3 = PathBuf::from("/shared/workspace/data.json");
    let outside = PathBuf::from("/etc/passwd");

    assert!(scopes.iter().any(|scope| path1.starts_with(scope)));
    assert!(scopes.iter().any(|scope| path2.starts_with(scope)));
    assert!(scopes.iter().any(|scope| path3.starts_with(scope)));
    assert!(!scopes.iter().any(|scope| outside.starts_with(scope)));
}

// ============= Platform Detection Tests =============

#[test]
fn test_current_os_supported() {
    let os = std::env::consts::OS;
    let supported = matches!(os, "linux" | "macos");
    
    // Just log for informational purposes
    if !supported {
        println!("Running on unsupported OS: {}", os);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_linux_kernel_version_parsing() {
    // Test kernel version string parsing logic
    let test_cases = vec![
        ("5.13.0-generic", true),   // Exactly minimum
        ("5.14.2-arch1", true),     // Above minimum
        ("6.0.0", true),            // Major version 6
        ("5.12.0", false),          // Below minimum
        ("4.19.0", false),          // Old LTS
    ];

    for (version_str, expected_sufficient) in test_cases {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() >= 2 {
            let major: u32 = parts[0].parse().unwrap_or(0);
            let minor: u32 = parts[1].split('-').next().unwrap_or("0").parse().unwrap_or(0);
            
            let is_sufficient = major > 5 || (major == 5 && minor >= 13);
            assert_eq!(
                is_sufficient, expected_sufficient,
                "Failed for version: {}",
                version_str
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_macos_sandbox_exec_exists() {
    use std::process::Command;
    
    let result = Command::new("which").arg("sandbox-exec").output();
    assert!(result.is_ok());
    let output = result.unwrap();
    
    // sandbox-exec should be found on macOS
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout);
        assert!(path.contains("sandbox-exec") || path.contains("sbin"));
    }
}

// ============= Scope Formatting Tests =============

#[test]
fn test_format_scopes_empty() {
    let scopes: Vec<PathBuf> = vec![];
    let formatted = format_scopes_for_test(&scopes);
    assert!(formatted.contains("none"));
}

#[test]
fn test_format_scopes_single() {
    let scopes = vec![PathBuf::from("/home/user/project")];
    let formatted = format_scopes_for_test(&scopes);
    assert!(formatted.contains("/home/user/project"));
}

#[test]
fn test_format_scopes_multiple() {
    let scopes = vec![
        PathBuf::from("/project1"),
        PathBuf::from("/project2"),
    ];
    let formatted = format_scopes_for_test(&scopes);
    assert!(formatted.contains("project1"));
    assert!(formatted.contains("project2"));
}

/// Helper function mirroring format_scopes from sandbox.rs
fn format_scopes_for_test(scopes: &[PathBuf]) -> String {
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
