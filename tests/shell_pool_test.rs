use ahma_mcp::shell_pool::{
    ShellCommand, ShellError, ShellPoolConfig, ShellPoolManager, ShellResponse,
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_shell_pool_config_default() {
    let config = ShellPoolConfig::default();
    assert!(config.enabled);
    assert_eq!(config.shells_per_directory, 2);
    assert_eq!(config.max_total_shells, 20);
    assert_eq!(config.shell_idle_timeout, Duration::from_secs(1800));
    assert_eq!(config.pool_cleanup_interval, Duration::from_secs(300));
    assert_eq!(config.shell_spawn_timeout, Duration::from_secs(5));
    assert_eq!(config.command_timeout, Duration::from_secs(300));
    assert_eq!(config.health_check_interval, Duration::from_secs(60));
}

#[tokio::test]
async fn test_shell_pool_config_custom() {
    let config = ShellPoolConfig {
        enabled: false,
        shells_per_directory: 3,
        max_total_shells: 15,
        shell_idle_timeout: Duration::from_secs(900),
        pool_cleanup_interval: Duration::from_secs(120),
        shell_spawn_timeout: Duration::from_secs(10),
        command_timeout: Duration::from_secs(600),
        health_check_interval: Duration::from_secs(30),
    };

    assert!(!config.enabled);
    assert_eq!(config.shells_per_directory, 3);
    assert_eq!(config.max_total_shells, 15);
    assert_eq!(config.shell_idle_timeout, Duration::from_secs(900));
    assert_eq!(config.pool_cleanup_interval, Duration::from_secs(120));
    assert_eq!(config.shell_spawn_timeout, Duration::from_secs(10));
    assert_eq!(config.command_timeout, Duration::from_secs(600));
    assert_eq!(config.health_check_interval, Duration::from_secs(30));
}

#[tokio::test]
async fn test_shell_pool_manager_creation() {
    let config = ShellPoolConfig::default();
    let manager = ShellPoolManager::new(config.clone());

    assert_eq!(manager.config().enabled, config.enabled);
    assert_eq!(
        manager.config().shells_per_directory,
        config.shells_per_directory
    );
    assert_eq!(manager.config().max_total_shells, config.max_total_shells);
}

#[tokio::test]
async fn test_shell_pool_manager_with_disabled_config() {
    let config = ShellPoolConfig {
        enabled: false,
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // When shell pool is disabled, getting a shell should return None
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let shell = manager.get_shell(working_dir).await;
    assert!(shell.is_none());
}

#[tokio::test]
async fn test_shell_command_creation() {
    let command = ShellCommand {
        id: "test_cmd_123".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        working_dir: "/tmp".to_string(),
        timeout_ms: 5000,
    };

    assert_eq!(command.id, "test_cmd_123");
    assert_eq!(command.command.len(), 2);
    assert_eq!(command.command[0], "echo");
    assert_eq!(command.command[1], "hello");
    assert_eq!(command.working_dir, "/tmp");
    assert_eq!(command.timeout_ms, 5000);
}

#[tokio::test]
async fn test_shell_command_serialization() {
    let command = ShellCommand {
        id: "test_cmd".to_string(),
        command: vec![
            "cargo".to_string(),
            "build".to_string(),
            "--release".to_string(),
        ],
        working_dir: "/project".to_string(),
        timeout_ms: 10000,
    };

    let json = serde_json::to_string(&command).unwrap();
    assert!(json.contains("test_cmd"));
    assert!(json.contains("cargo"));
    assert!(json.contains("build"));
    assert!(json.contains("--release"));
    assert!(json.contains("/project"));
    assert!(json.contains("10000"));

    // Test round-trip
    let deserialized: ShellCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, command.id);
    assert_eq!(deserialized.command, command.command);
    assert_eq!(deserialized.working_dir, command.working_dir);
    assert_eq!(deserialized.timeout_ms, command.timeout_ms);
}

#[tokio::test]
async fn test_shell_response_creation() {
    let response = ShellResponse {
        id: "response_123".to_string(),
        exit_code: 0,
        stdout: "Build successful".to_string(),
        stderr: "".to_string(),
        duration_ms: 2500,
    };

    assert_eq!(response.id, "response_123");
    assert_eq!(response.exit_code, 0);
    assert_eq!(response.stdout, "Build successful");
    assert!(response.stderr.is_empty());
    assert_eq!(response.duration_ms, 2500);
}

#[tokio::test]
async fn test_shell_response_serialization() {
    let response = ShellResponse {
        id: "test_response".to_string(),
        exit_code: 1,
        stdout: "warning: unused variable".to_string(),
        stderr: "error: compilation failed".to_string(),
        duration_ms: 1200,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("test_response"));
    assert!(json.contains("\"exit_code\":1"));
    assert!(json.contains("unused variable"));
    assert!(json.contains("compilation failed"));
    assert!(json.contains("1200"));

    // Test round-trip
    let deserialized: ShellResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, response.id);
    assert_eq!(deserialized.exit_code, response.exit_code);
    assert_eq!(deserialized.stdout, response.stdout);
    assert_eq!(deserialized.stderr, response.stderr);
    assert_eq!(deserialized.duration_ms, response.duration_ms);
}

#[tokio::test]
async fn test_shell_error_properties() {
    // Test SpawnError
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "command not found");
    let spawn_error = ShellError::SpawnError(io_error);
    assert!(!spawn_error.is_recoverable());
    assert!(!spawn_error.is_resource_exhaustion());
    assert!(spawn_error.is_io_error());
    assert_eq!(spawn_error.error_category(), "IO");
    assert_eq!(spawn_error.severity_level(), "ERROR");

    // Test Timeout
    let timeout_error = ShellError::Timeout;
    assert!(timeout_error.is_recoverable());
    assert!(timeout_error.is_resource_exhaustion());
    assert!(!timeout_error.is_io_error());
    assert_eq!(timeout_error.error_category(), "TIMEOUT");
    assert_eq!(timeout_error.severity_level(), "WARN");

    // Test ProcessDied
    let process_died = ShellError::ProcessDied;
    assert!(process_died.is_recoverable());
    assert!(!process_died.is_resource_exhaustion());
    assert!(!process_died.is_io_error());
    assert_eq!(process_died.error_category(), "PROCESS");
    assert_eq!(process_died.severity_level(), "ERROR");

    // Test SerializationError
    let invalid_json = "{invalid json";
    let serde_error = serde_json::from_str::<serde_json::Value>(invalid_json).unwrap_err();
    let serialization_error = ShellError::SerializationError(serde_error);
    assert!(!serialization_error.is_recoverable());
    assert!(!serialization_error.is_resource_exhaustion());
    assert!(!serialization_error.is_io_error());
    assert_eq!(serialization_error.error_category(), "SERIALIZATION");
    assert_eq!(serialization_error.severity_level(), "ERROR");

    // Test PoolFull
    let pool_full = ShellError::PoolFull;
    assert!(pool_full.is_recoverable());
    assert!(pool_full.is_resource_exhaustion());
    assert!(!pool_full.is_io_error());
    assert_eq!(pool_full.error_category(), "RESOURCE");
    assert_eq!(pool_full.severity_level(), "WARN");

    // Test WorkingDirectoryError
    let wd_error = ShellError::WorkingDirectoryError("invalid path".to_string());
    assert!(!wd_error.is_recoverable());
    assert!(!wd_error.is_resource_exhaustion());
    assert!(wd_error.is_io_error());
    assert_eq!(wd_error.error_category(), "IO");
    assert_eq!(wd_error.severity_level(), "ERROR");
}

#[tokio::test]
async fn test_shell_error_display() {
    // Test error message formatting
    let timeout_error = ShellError::Timeout;
    assert_eq!(format!("{}", timeout_error), "Shell communication timeout");

    let process_died = ShellError::ProcessDied;
    assert_eq!(
        format!("{}", process_died),
        "Shell process died unexpectedly"
    );

    let pool_full = ShellError::PoolFull;
    assert_eq!(format!("{}", pool_full), "Shell pool is at capacity");

    let wd_error = ShellError::WorkingDirectoryError("Path not found".to_string());
    assert_eq!(
        format!("{}", wd_error),
        "Working directory access error: Path not found"
    );
}

#[tokio::test]
async fn test_shell_pool_manager_get_shell_disabled() {
    let config = ShellPoolConfig {
        enabled: false,
        ..Default::default()
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Should return None when disabled
    let result = manager.get_shell(working_dir).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_shell_pool_manager_background_tasks_disabled() {
    let config = ShellPoolConfig {
        enabled: false,
        ..Default::default()
    };

    let manager = Arc::new(ShellPoolManager::new(config));

    // Should not panic when starting background tasks even if disabled
    manager.start_background_tasks();

    // Give a moment for any potential startup
    tokio::time::sleep(Duration::from_millis(10)).await;
}

#[tokio::test]
async fn test_shell_pool_manager_concurrent_access() {
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 1,
        max_total_shells: 2,
        ..ShellPoolConfig::default()
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Try to get multiple shells concurrently
    let manager_clone1 = manager.clone();
    let manager_clone2 = manager.clone();
    let wd1 = working_dir.to_string();
    let wd2 = working_dir.to_string();

    let task1 = tokio::spawn(async move { manager_clone1.get_shell(&wd1).await });

    let task2 = tokio::spawn(async move { manager_clone2.get_shell(&wd2).await });

    let (result1, result2) = tokio::join!(task1, task2);

    // At least one should succeed (or both could be None if shells fail to start)
    let shell1 = result1.unwrap();
    let shell2 = result2.unwrap();

    // Return shells if we got them
    if let Some(shell) = shell1 {
        manager.return_shell(shell).await;
    }
    if let Some(shell) = shell2 {
        manager.return_shell(shell).await;
    }
}

#[tokio::test]
async fn test_shell_pool_config_clone() {
    let config1 = ShellPoolConfig::default();
    let config2 = config1.clone();

    assert_eq!(config1.enabled, config2.enabled);
    assert_eq!(config1.shells_per_directory, config2.shells_per_directory);
    assert_eq!(config1.max_total_shells, config2.max_total_shells);
    assert_eq!(config1.shell_idle_timeout, config2.shell_idle_timeout);
}

#[tokio::test]
async fn test_shell_command_with_complex_args() {
    let command = ShellCommand {
        id: "complex_cmd".to_string(),
        command: vec![
            "cargo".to_string(),
            "test".to_string(),
            "--".to_string(),
            "--test-threads".to_string(),
            "4".to_string(),
            "--nocapture".to_string(),
        ],
        working_dir: "/workspace/project".to_string(),
        timeout_ms: 30000,
    };

    assert_eq!(command.command.len(), 6);
    assert!(command.command.contains(&"--test-threads".to_string()));
    assert!(command.command.contains(&"4".to_string()));
    assert!(command.command.contains(&"--nocapture".to_string()));
}

#[tokio::test]
async fn test_shell_response_with_mixed_output() {
    let response = ShellResponse {
        id: "mixed_output".to_string(),
        exit_code: 0,
        stdout: "Success: Build completed\nArtifacts: target/release/binary".to_string(),
        stderr: "warning: unused import\nwarning: deprecated function".to_string(),
        duration_ms: 4500,
    };

    assert!(response.stdout.contains("Success"));
    assert!(response.stdout.contains("Artifacts"));
    assert!(response.stderr.contains("warning"));
    assert!(response.stderr.contains("unused import"));
    assert!(response.stderr.contains("deprecated function"));
    assert_eq!(response.exit_code, 0);
}

#[tokio::test]
async fn test_shell_pool_manager_config_access() {
    let original_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 5,
        max_total_shells: 25,
        shell_idle_timeout: Duration::from_secs(3600),
        pool_cleanup_interval: Duration::from_secs(600),
        shell_spawn_timeout: Duration::from_secs(15),
        command_timeout: Duration::from_secs(900),
        health_check_interval: Duration::from_secs(120),
    };

    let manager = ShellPoolManager::new(original_config.clone());
    let retrieved_config = manager.config();

    assert_eq!(retrieved_config.enabled, original_config.enabled);
    assert_eq!(
        retrieved_config.shells_per_directory,
        original_config.shells_per_directory
    );
    assert_eq!(
        retrieved_config.max_total_shells,
        original_config.max_total_shells
    );
    assert_eq!(
        retrieved_config.shell_idle_timeout,
        original_config.shell_idle_timeout
    );
    assert_eq!(
        retrieved_config.pool_cleanup_interval,
        original_config.pool_cleanup_interval
    );
    assert_eq!(
        retrieved_config.shell_spawn_timeout,
        original_config.shell_spawn_timeout
    );
    assert_eq!(
        retrieved_config.command_timeout,
        original_config.command_timeout
    );
    assert_eq!(
        retrieved_config.health_check_interval,
        original_config.health_check_interval
    );
}
