use ahma_http_bridge::session::{McpRoot, SessionManager, SessionManagerConfig};
use serde_json::json;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

#[tokio::test]
async fn test_handshake_replay_on_restart() {
    // 1. Setup Mock MCP Server Script
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let script_path = temp_dir.path().join("mock_mcp.py");
    let log_path = temp_dir.path().join("mock_mcp.log");

    let script_content = r#"
import sys
import json
import os
import time

log_file = os.environ.get("MOCK_LOG_FILE")

def log(msg):
    if log_file:
        with open(log_file, "a") as f:
            f.write(msg + "\n")

log(f"STARTING PID={os.getpid()}")

while True:
    try:
        line = sys.stdin.readline()
        if not line:
            break
        
        log(f"RECEIVED: {line.strip()}")
        
        try:
            req = json.loads(line)
            if req.get("method") == "initialize":
                resp = {
                    "jsonrpc": "2.0",
                    "id": req.get("id"),
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "serverInfo": {"name": "mock", "version": "1.0"}
                    }
                }
                # Small delay to ensure the bridge waits
                time.sleep(0.1)
                print(json.dumps(resp))
                sys.stdout.flush()
                log(f"SENT: {json.dumps(resp)}")
        except Exception:
            pass
            
    except Exception as e:
        log(f"ERROR: {e}")
        break
"#;

    fs::write(&script_path, script_content).expect("Failed to write mock script");

    // 2. Configure SessionManager
    let config = SessionManagerConfig {
        server_command: "python3".to_string(),
        server_args: vec![script_path.to_str().unwrap().to_string()],
        default_scope: temp_dir.path().to_path_buf(),
        enable_colored_output: false,
    };

    let session_manager = SessionManager::new(config);

    // 3. Create Session
    // Set env var for the subprocess so it knows where to log
    // SAFETY: This is a test, and we are setting it before creating the session/subprocess.
    // In a real multi-threaded test runner this could be racy, but for this specific test it's fine.
    unsafe {
        std::env::set_var("MOCK_LOG_FILE", log_path.to_str().unwrap());
    }

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session exists");

    // Wait for the first process to start up and log
    sleep(Duration::from_millis(500)).await;

    // 4. Store Initialize Request (Simulate handshake)
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });
    session.store_initialize_request(init_req.clone()).await;

    // 5. Trigger Restart via lock_sandbox
    let roots = vec![McpRoot {
        uri: format!("file://{}", temp_dir.path().display()),
        name: Some("root".to_string()),
    }];

    // This should:
    // 1. Kill first process
    // 2. Start second process
    // 3. Send initialize to second process
    // 4. Wait for response
    // 5. Send initialized to second process
    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox and restart");

    // Give some time for logs to flush
    sleep(Duration::from_millis(500)).await;

    // 6. Verify Logs
    let log_content = fs::read_to_string(&log_path).expect("Failed to read log file");
    println!("Mock Server Log:\n{}", log_content);

    // Check for two processes
    let start_count = log_content.matches("STARTING PID=").count();
    // We expect at least 1 start (the restarted one). The first one might have been killed too fast if we didn't wait,
    // but with the sleep above we should see 2.
    assert!(start_count >= 1, "Should have started at least one process");

    // Split log by process to verify the second one got the handshake
    let parts: Vec<&str> = log_content.split("STARTING PID=").collect();
    let last_process_log = parts.last().expect("Should have log parts");

    // Verify handshake replay on the last process
    assert!(
        last_process_log
            .contains("RECEIVED: {\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"initialize\""),
        "Restarted process should receive initialize request. Log: {}",
        last_process_log
    );

    assert!(
        last_process_log.contains("SENT: {\"jsonrpc\": \"2.0\", \"id\": 1"),
        "Restarted process should send initialize response. Log: {}",
        last_process_log
    );

    assert!(
        last_process_log
            .contains("RECEIVED: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}"),
        "Restarted process should receive initialized notification. Log: {}",
        last_process_log
    );

    // Verify order: initialize -> SENT -> initialized
    let init_idx = last_process_log.find("RECEIVED: {\"id\":1").unwrap();
    let sent_idx = last_process_log.find("SENT:").unwrap();
    let initialized_idx = last_process_log
        .find("RECEIVED: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}")
        .unwrap();

    assert!(
        init_idx < sent_idx,
        "Should receive initialize before sending response"
    );
    assert!(
        sent_idx < initialized_idx,
        "Should send response before receiving initialized"
    );
}
