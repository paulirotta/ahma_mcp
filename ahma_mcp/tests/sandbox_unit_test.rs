//! Unit tests for sandbox.rs - Kernel-level sandboxing
//!
//! These tests cover:
//! - Path validation and normalization
//! - Sandbox scope initialization
//! - SandboxError formatting
//! - Platform-specific sandbox checks

use ahma_mcp::sandbox::{Sandbox, SandboxError, SandboxMode};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

// ============= SandboxError Display Tests =============

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

// ============= Test Mode Detection Tests =============

#[test]
fn test_is_test_mode_on_sandbox_instance() {
    let sandbox = Sandbox::new_test();
    assert!(sandbox.is_test_mode());

    let sandbox_strict =
        Sandbox::new(vec![PathBuf::from("/tmp")], SandboxMode::Strict, false).unwrap();
    assert!(!sandbox_strict.is_test_mode());
}

#[test]
fn test_is_no_temp_files_on_sandbox_instance() {
    let sandbox = Sandbox::new(vec![PathBuf::from("/tmp")], SandboxMode::Strict, true).unwrap();
    assert!(sandbox.is_no_temp_files());

    let sandbox_default =
        Sandbox::new(vec![PathBuf::from("/tmp")], SandboxMode::Strict, false).unwrap();
    assert!(!sandbox_default.is_no_temp_files());
}

// ============= Path Validation Logic Tests =============

#[test]
fn test_path_validation_in_scope() {
    let temp = tempdir().unwrap();
    let scope = temp.path().to_path_buf();
    let sandbox = Sandbox::new(vec![scope.clone()], SandboxMode::Strict, false).unwrap();

    let valid_path = scope.join("test.txt");
    assert!(sandbox.validate_path(&valid_path).is_ok());
}

#[test]
fn test_path_validation_outside_scope() {
    let temp = tempdir().unwrap();
    let scope = temp.path().to_path_buf();
    let sandbox = Sandbox::new(vec![scope], SandboxMode::Strict, false).unwrap();

    let outside_path = PathBuf::from("/etc/passwd");
    assert!(sandbox.validate_path(&outside_path).is_err());
}
