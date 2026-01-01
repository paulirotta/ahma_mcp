use ahma_core::sandbox;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_high_security_mode_enforcement() {
    // Enable high security mode
    sandbox::enable_no_temp_files();
    assert!(sandbox::is_no_temp_files());

    // Initialize sandbox with a temp dir if not already initialized
    let temp = tempdir().unwrap();
    let scope = temp.path().to_path_buf();
    
    if sandbox::get_sandbox_scopes().is_none() {
        let _ = sandbox::initialize_sandbox_scope(&scope);
    }

    let current_scopes = sandbox::get_sandbox_scopes().unwrap();
    let first_scope = current_scopes.first().unwrap();

    // 1. Valid path in scope should be allowed UNLESS it's a temp dir and high security is on
    // Wait, if the scope ITSELF is a temp dir (like /var/folders or /tmp), 
    // then even "valid" paths in it should be blocked in high security mode.
    let valid_path = first_scope.join("test.txt");
    let result = sandbox::validate_path_in_sandbox(&valid_path);
    
    let is_temp_scope = first_scope.to_string_lossy().starts_with("/tmp") || 
                        first_scope.to_string_lossy().starts_with("/private/tmp") ||
                        first_scope.to_string_lossy().starts_with("/var/folders") ||
                        first_scope.to_string_lossy().starts_with("/private/var/folders");

    if is_temp_scope {
        // If the scope is a temp dir, it should be blocked in high security mode
        assert!(result.is_err(), "Path in temp scope should be blocked in high security mode");
    } else {
        assert!(result.is_ok(), "Valid path in non-temp scope should be allowed: {:?}", result.err());
    }

    // 2. Path in /tmp should be blocked
    let tmp_path = Path::new("/tmp/test.txt");
    let result = sandbox::validate_path_in_sandbox(tmp_path);
    assert!(result.is_err(), "Path in /tmp should be blocked in high security mode");

    // 3. Path in /dev should be blocked
    let dev_path = Path::new("/dev/null");
    let result = sandbox::validate_path_in_sandbox(dev_path);
    assert!(result.is_err(), "Path in /dev should be blocked in high security mode");
}

