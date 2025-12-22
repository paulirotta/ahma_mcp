//! Shared test utilities for HTTP bridge integration tests.
//!
//! This module provides utilities for integration tests that need a running HTTP server.
//!
//! ## Port Assignment
//!
//! - **Production default**: Port 3000
//! - **Integration tests**: Port 5721 (reserved for shared test server)
//!
//! ## Usage Patterns
//!
//! 1. **Tests using the shared singleton** (sse_tool_integration_test, http_roots_handshake_test):
//!    Call `get_test_server().await` to get a shared server on port 5721.
//!
//! 2. **Tests needing custom configurations** (http_bridge_integration_test):
//!    Use dynamic port allocation via `find_available_port()` and spawn their own server.

// Allow dead_code - these are test utilities, and rustc can't see usage across test crates
#![allow(dead_code)]

use reqwest::Client;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Reserved port for integration tests. DO NOT use port 3000 in tests.
pub const AHMA_INTEGRATION_TEST_SERVER_PORT: u16 = 5721;

/// A running test server instance
struct TestServerInstance {
    child: Child,
    _temp_dir: TempDir, // Keep alive for the duration of the server
}

impl Drop for TestServerInstance {
    fn drop(&mut self) {
        eprintln!(
            "[TestServer] Shutting down test server on port {}",
            AHMA_INTEGRATION_TEST_SERVER_PORT
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Guard that provides access to the test server
pub struct TestServerGuard {
    _inner: Arc<Mutex<Option<TestServerInstance>>>,
}

impl TestServerGuard {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", AHMA_INTEGRATION_TEST_SERVER_PORT)
    }

    pub fn port(&self) -> u16 {
        AHMA_INTEGRATION_TEST_SERVER_PORT
    }
}

/// Global singleton for the test server (within this process)
static TEST_SERVER: OnceLock<Arc<Mutex<Option<TestServerInstance>>>> = OnceLock::new();

/// Install panic hook to clean up server on panic
fn install_panic_hook() {
    static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
    HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Try to clean up the test server
            if let Some(server) = TEST_SERVER.get()
                && let Ok(mut guard) = server.lock()
                && let Some(mut instance) = guard.take()
            {
                eprintln!("[TestServer] Panic detected! Cleaning up test server...");
                let _ = instance.child.kill();
                let _ = instance.child.wait();
            }
            default_hook(info);
        }));
    });
}

/// Check if the test server is already running (started by another test process)
async fn is_server_running() -> bool {
    let client = Client::new();
    let health_url = format!(
        "http://127.0.0.1:{}/health",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    match client
        .get(&health_url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Build the ahma_mcp binary if needed and return the path
fn get_ahma_mcp_binary() -> PathBuf {
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();

    // Build ahma_mcp binary
    let output = Command::new("cargo")
        .current_dir(&workspace_dir)
        .args(["build", "--package", "ahma_core", "--bin", "ahma_mcp"])
        .output()
        .expect("Failed to run cargo build");

    assert!(
        output.status.success(),
        "Failed to build ahma_mcp: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    workspace_dir.join("target/debug/ahma_mcp")
}

/// Start the test server
async fn start_test_server() -> Result<TestServerInstance, String> {
    let binary = get_ahma_mcp_binary();

    // Get the workspace directory
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();

    // Use the workspace .ahma/tools directory
    let tools_dir = workspace_dir.join(".ahma/tools");

    // Create temp directory for sandbox scope
    let temp_dir = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let sandbox_scope = temp_dir.path().to_path_buf();

    // Build command args
    let args = vec![
        "--mode".to_string(),
        "http".to_string(),
        "--http-port".to_string(),
        AHMA_INTEGRATION_TEST_SERVER_PORT.to_string(),
        "--tools-dir".to_string(),
        tools_dir.to_string_lossy().to_string(),
        "--sandbox-scope".to_string(),
        sandbox_scope.to_string_lossy().to_string(),
    ];

    eprintln!(
        "[TestServer] Starting test server on port {}",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    let child = Command::new(&binary)
        .args(&args)
        .env("AHMA_TEST_MODE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn test server: {}", e))?;

    // Wait for server to be ready
    let client = Client::new();
    let health_url = format!(
        "http://127.0.0.1:{}/health",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    for i in 0..50 {
        sleep(Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            eprintln!("[TestServer] Server ready after {}ms", (i + 1) * 100);
            return Ok(TestServerInstance {
                child,
                _temp_dir: temp_dir,
            });
        }
    }

    // Failed to start - clean up
    let mut child = child;
    let _ = child.kill();
    let _ = child.wait();
    Err("Test server failed to start within 5 seconds".to_string())
}

/// Get or start the shared test server.
///
/// This function ensures a test server is running on port 5721:
/// 1. If a server is already running (from this or another process), use it
/// 2. If not, start one and keep it running for subsequent tests
///
/// Note: Due to nextest running tests in separate processes, each process
/// may start its own server. This is OK - the first one to bind wins.
pub async fn get_test_server() -> TestServerGuard {
    install_panic_hook();

    let server_arc = TEST_SERVER.get_or_init(|| Arc::new(Mutex::new(None)));

    // Check if we already have a server in this process
    {
        let guard = server_arc.lock().unwrap();
        if guard.is_some() {
            return TestServerGuard {
                _inner: server_arc.clone(),
            };
        }
    }

    // Check if a server is already running (from another process)
    if is_server_running().await {
        eprintln!(
            "[TestServer] Server already running on port {}, reusing",
            AHMA_INTEGRATION_TEST_SERVER_PORT
        );
        return TestServerGuard {
            _inner: server_arc.clone(),
        };
    }

    // Try to start a new server
    match start_test_server().await {
        Ok(instance) => {
            let mut guard = server_arc.lock().unwrap();
            *guard = Some(instance);
        }
        Err(e) => {
            // If we failed because the port is in use, another process grabbed it
            // Check if a server is now running
            sleep(Duration::from_millis(500)).await;
            if is_server_running().await {
                eprintln!(
                    "[TestServer] Another process started server on port {}, reusing",
                    AHMA_INTEGRATION_TEST_SERVER_PORT
                );
            } else {
                panic!("Failed to start test server: {}", e);
            }
        }
    }

    TestServerGuard {
        _inner: server_arc.clone(),
    }
}
