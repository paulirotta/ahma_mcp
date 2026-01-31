use ahma_mcp::transport_patch::PatchedTransport;
use rmcp::transport::Transport;
use std::io::Cursor;
use tokio::io::BufReader;

#[tokio::test]
async fn test_patched_transport_removes_tasks() {
    // 1. Prepare problematic JSON Input
    // This JSON mimics VS Code's initialize request which includes "tasks" object
    // Original rmcp 0.13.0 fails because it likely expects bool or nothing
    let input_json = r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {"tasks": {"cancel": {}, "list": {}}, "roots": { "listChanged": true }}, "clientInfo": {"name": "TestClient", "version": "1.0"}}}"#;

    // Create generic transport with in-memory buffers
    let input = input_json.as_bytes().to_vec();
    // Cursor implements AsyncRead. Wrap in BufReader for AsyncBufRead.
    let reader = BufReader::new(Cursor::new(input));

    let writer = Cursor::new(Vec::new()); // Writer

    let mut transport = PatchedTransport::new(reader, writer);

    // 2. Call receive
    println!("Waiting for message...");
    let msg_opt = transport.receive().await;

    // 3. Verify Parsing Success
    if let Some(msg) = msg_opt {
        println!("Successfully parsed message: {:?}", msg);
    } else {
        panic!("Transport returned None (error or EOF), expected parsing success");
    }
}
