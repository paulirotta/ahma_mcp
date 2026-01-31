use ahma_mcp::shell_pool::{PrewarmedShell, ShellError, ShellPoolConfig};
use ahma_mcp::utils::logging::init_test_logging;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_shell_spawn_timeout() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    // Configure a very short spawn timeout (1 nanosecond)
    // This should be impossible to meet for a process spawn + initialization
    let config = ShellPoolConfig {
        enabled: true,
        shell_spawn_timeout: Duration::from_nanos(1),
        ..Default::default()
    };

    let result = PrewarmedShell::new(temp_dir.path(), &config).await;

    assert!(result.is_err());
    if let Err(error) = result {
        assert!(matches!(error, ShellError::Timeout));
        assert!(error.is_recoverable());
        assert!(error.is_resource_exhaustion());
    }
}
