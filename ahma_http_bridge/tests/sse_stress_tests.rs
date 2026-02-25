//! SSE Stress and Concurrent Tests
//!
//! High-volume and concurrent request tests for the HTTP SSE bridge.

mod common;

use common::McpTestClient;
use common::sse_test_helpers::{self, ensure_server_available};
use futures::future::join_all;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Stress test for concurrent tool calls.
///
/// This test is ignored by default because:
/// 1. It requires an external SSE server to be running
/// 2. It tests edge-case concurrent behavior that may be flaky
///
/// Run manually with: `cargo nextest run test_concurrent_tool_calls --run-ignored`
#[tokio::test]
#[ignore]
async fn test_concurrent_tool_calls() {
    let _server = ensure_server_available().await;

    let base_url = sse_test_helpers::get_sse_url();
    let mut mcp = McpTestClient::with_url(&base_url);
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    mcp.initialize_with_roots("stress-client", &[root])
        .await
        .expect("handshake failed");
    let mcp = Arc::new(mcp);
    let start = Instant::now();

    // Create a batch of concurrent requests
    let requests = vec![
        ("file-tools_pwd", json!({})),
        ("file-tools_ls", json!({"path": "."})),
        ("file-tools_ls", json!({"path": "ahma_mcp"})),
        ("file-tools_cat", json!({"files": ["Cargo.toml"]})),
        ("sandboxed_shell", json!({"command": "echo test1"})),
        ("sandboxed_shell", json!({"command": "echo test2"})),
        ("sandboxed_shell", json!({"command": "echo test3"})),
        ("sandboxed_shell", json!({"command": "pwd"})),
        ("sandboxed_shell", json!({"command": "ls -la"})),
        ("sandboxed_shell", json!({"command": "echo 'hello world'"})),
        ("sandboxed_shell", json!({"command": "date"})),
        ("sandboxed_shell", json!({"command": "whoami"})),
        ("sandboxed_shell", json!({"command": "uname -a"})),
        (
            "sandboxed_shell",
            json!({"command": "cat Cargo.toml | head -5"}),
        ),
    ];

    let num_requests = requests.len();

    // Execute all requests concurrently
    let futures: Vec<_> = requests
        .into_iter()
        .map(|(name, args)| {
            let mcp = Arc::clone(&mcp);
            async move { mcp.call_tool(name, args).await }
        })
        .collect();

    let results = join_all(futures).await;
    let total_duration = start.elapsed();

    // Analyze results
    let mut successes = 0;
    let mut failures = 0;
    let mut total_tool_time: u128 = 0;

    for result in &results {
        if result.success {
            successes += 1;
        } else {
            failures += 1;
            eprintln!("FAIL {} failed: {:?}", result.tool_name, result.error);
        }
        total_tool_time += result.duration_ms;
    }

    println!("\n📊 Concurrent Test Results:");
    println!("   Total requests: {}", num_requests);
    println!("   Successes: {}", successes);
    println!("   Failures: {}", failures);
    println!("   Total wall time: {}ms", total_duration.as_millis());
    println!("   Sum of individual times: {}ms", total_tool_time);
    println!(
        "   Concurrency benefit: {:.1}x speedup",
        total_tool_time as f64 / total_duration.as_millis() as f64
    );

    // All core file tools should succeed
    assert!(
        successes >= 8,
        "At least 8 out of {} requests should succeed",
        num_requests
    );
}

/// High-volume stress test for concurrent requests.
///
/// This test is ignored by default because:
/// 1. It requires an external SSE server to be running
/// 2. It sends 50 concurrent requests which may overwhelm the server
///
/// Run manually with: `cargo nextest run test_high_volume_concurrent_requests --run-ignored`
#[tokio::test]
#[ignore]
async fn test_high_volume_concurrent_requests() {
    let _server = ensure_server_available().await;

    let base_url = sse_test_helpers::get_sse_url();
    let mut mcp = McpTestClient::with_url(&base_url);
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    mcp.initialize_with_roots("stress-client", &[root])
        .await
        .expect("handshake failed");
    let mcp = Arc::new(mcp);
    let num_requests = 50;
    let start = Instant::now();

    // Create many concurrent echo requests
    let futures: Vec<_> = (0..num_requests)
        .map(|i| {
            let mcp = Arc::clone(&mcp);
            async move {
                mcp.call_tool(
                    "sandboxed_shell",
                    json!({"command": format!("echo 'Request {}'", i)}),
                )
                .await
            }
        })
        .collect();

    let results = join_all(futures).await;
    let total_duration = start.elapsed();

    let successes = results.iter().filter(|r| r.success).count();
    let failures = results.iter().filter(|r| !r.success).count();

    println!("\n📊 High-Volume Stress Test Results:");
    println!("   Total requests: {}", num_requests);
    println!("   Successes: {}", successes);
    println!("   Failures: {}", failures);
    println!("   Total time: {}ms", total_duration.as_millis());
    println!(
        "   Requests/second: {:.1}",
        num_requests as f64 / total_duration.as_secs_f64()
    );

    // At least 90% should succeed
    let success_rate = successes as f64 / num_requests as f64;
    assert!(
        success_rate >= 0.9,
        "Success rate {:.1}% below 90% threshold",
        success_rate * 100.0
    );
}
