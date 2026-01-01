//! Stress Test for AHMA HTTP Bridge
//!
//! This example spawns an AHMA HTTP server and runs multiple concurrent clients
//! to stress test the server's capabilities. It exercises both synchronous and
//! asynchronous command execution patterns.
//!
//! # Usage
//!
//! ```bash
//! # Run with defaults (port 7634, 60 second duration)
//! cargo run --example stress_test
//!
//! # Custom port and duration
//! cargo run --example stress_test -- --port 8080 --duration 120
//!
//! # Use existing binary instead of cargo run
//! cargo run --example stress_test -- --binary ./target/release/ahma_mcp
//! ```
//!
//! # Architecture
//!
//! - Spawns 1 HTTP server on the specified port
//! - Spawns 4 concurrent clients:
//!   - 1 synchronous client (waits for each command to complete)
//!   - 3 asynchronous clients (fire-and-forget pattern)
//! - Each client runs a randomized sequence of safe shell commands
//! - Server stderr is monitored; any error output stops the test immediately
//!
//! # Command Pool
//!
//! The test uses safe, read-only commands that don't modify the repository:
//! - `echo`: Tests basic command execution
//! - `ls -la`: Tests directory listing
//! - `whoami`: Tests user context
//! - `date`: Tests system commands
//! - `pwd`: Tests working directory
//! - `uname -a`: Tests system info
//! - `env | grep PATH`: Tests environment access
//! - `cat /etc/hosts | head -5`: Tests file reading (non-repo)

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

/// Command-line arguments for the stress test
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to run the server on
    #[arg(long, default_value_t = 7634)]
    port: u16,

    /// Duration of the test in seconds
    #[arg(long, default_value_t = 60)]
    duration: u64,

    /// Path to ahma_mcp binary (if not provided, uses cargo run)
    #[arg(long)]
    binary: Option<String>,

    /// Number of async clients (in addition to 1 sync client)
    #[arg(long, default_value_t = 3)]
    async_clients: usize,
}

/// Simple Linear Congruential Generator for deterministic randomization
/// Avoids dependency on `rand` crate
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        // LCG parameters from Numerical Recipes
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_range(&mut self, max: usize) -> usize {
        (self.next() as usize) % max
    }

    /// Shuffle a vector using Fisher-Yates algorithm
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.next_range(i + 1);
            items.swap(i, j);
        }
    }
}

/// A stress test client that sends requests to the server
struct StressClient {
    client: Client,
    base_url: String,
    is_sync: bool,
    session_id: Option<String>,
    request_id: AtomicU64,
    success_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
}

impl StressClient {
    fn new(
        base_url: String,
        is_sync: bool,
        success_count: Arc<AtomicU64>,
        error_count: Arc<AtomicU64>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url,
            is_sync,
            session_id: None,
            request_id: AtomicU64::new(0),
            success_count,
            error_count,
        }
    }

    /// Initialize the MCP session following the handshake protocol
    async fn initialize(&mut self) -> Result<()> {
        let url = format!("{}/mcp", self.base_url);
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        // Step 1: Send initialize request (no session header)
        let response = self
            .client
            .post(&url)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "roots": {
                            "listChanged": true
                        }
                    },
                    "clientInfo": {
                        "name": "stress-test",
                        "version": "0.1.0"
                    }
                },
                "id": id
            }))
            .send()
            .await
            .context("Failed to send initialize request")?;

        if !response.status().is_success() {
            return Err(anyhow!("Initialize failed: {}", response.status()));
        }

        // Step 2: Extract session ID from response header
        if let Some(session_id) = response.headers().get("mcp-session-id") {
            self.session_id = Some(session_id.to_str()?.to_string());
        } else {
            return Err(anyhow!("No mcp-session-id header in initialize response"));
        }

        let session_id = self.session_id.clone().unwrap();

        // Step 3: Open SSE stream BEFORE sending initialized notification
        // This is required to receive the roots/list request from the server
        let sse_client = self.client.clone();
        let sse_url = url.clone();
        let sse_session_id = session_id.clone();
        
        let sse_response = sse_client
            .get(&sse_url)
            .header("mcp-session-id", &sse_session_id)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .context("Failed to open SSE stream")?;

        if !sse_response.status().is_success() {
            return Err(anyhow!("SSE connection failed: {}", sse_response.status()));
        }

        // Step 4: Send initialized notification
        self.client
            .post(&url)
            .header("mcp-session-id", &session_id)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .send()
            .await
            .context("Failed to send initialized notification")?;

        // Step 5: Wait for roots/list request on SSE and respond
        let post_client = self.client.clone();
        let post_url = url.clone();
        let post_session_id = session_id.clone();
        
        let mut stream = sse_response.bytes_stream();
        let mut buffer = String::new();
        
        // Read SSE events looking for roots/list request
        let timeout = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.context("Failed to read SSE chunk")?;
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);
                
                // Parse SSE events from buffer
                for line in buffer.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(msg) = serde_json::from_str::<Value>(data) {
                            // Check if this is a roots/list request
                            if msg.get("method").and_then(|m| m.as_str()) == Some("roots/list") {
                                if let Some(req_id) = msg.get("id") {
                                    // Respond with roots
                                    let response = post_client
                                        .post(&post_url)
                                        .header("mcp-session-id", &post_session_id)
                                        .json(&json!({
                                            "jsonrpc": "2.0",
                                            "id": req_id,
                                            "result": {
                                                "roots": [{
                                                    "uri": "file://.",
                                                    "name": "workspace"
                                                }]
                                            }
                                        }))
                                        .send()
                                        .await
                                        .context("Failed to respond to roots/list")?;
                                    
                                    if !response.status().is_success() {
                                        return Err(anyhow!("roots/list response failed: {}", response.status()));
                                    }
                                    
                                    // Handshake complete!
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                
                // Clear processed lines from buffer, keep partial line
                if let Some(last_newline) = buffer.rfind('\n') {
                    buffer = buffer[last_newline + 1..].to_string();
                }
            }
            Err(anyhow!("SSE stream ended without receiving roots/list"))
        }).await;

        match timeout {
            Ok(Ok(())) => {
                // Give server a moment to process the roots response
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("Timeout waiting for roots/list request from server")),
        }
    }

    /// Run commands in a loop until stopped
    async fn run_commands(
        &mut self,
        mut commands: Vec<Vec<String>>,
        mut lcg: Lcg,
        stop_flag: Arc<AtomicBool>,
    ) -> Result<()> {
        // Shuffle commands for this client
        lcg.shuffle(&mut commands);

        let mut command_idx = 0;
        while !stop_flag.load(Ordering::Relaxed) {
            let cmd = &commands[command_idx % commands.len()];
            command_idx += 1;

            match self.send_request(cmd).await {
                Ok(_) => {
                    self.success_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!("Request error: {}", e);
                    self.error_count.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Brief delay between requests
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Ok(())
    }

    /// Send a single tool call request
    async fn send_request(&mut self, command: &[String]) -> Result<()> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let cmd_str = command.join(" ");
        let url = format!("{}/mcp", self.base_url);
        let session_id = self
            .session_id
            .as_ref()
            .ok_or_else(|| anyhow!("No session ID"))?;

        let response = self
            .client
            .post(&url)
            .header("mcp-session-id", session_id)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "sandboxed_shell",
                    "arguments": {
                        "command": cmd_str,
                        "working_directory": ".",
                        "synchronous": self.is_sync
                    }
                },
                "id": id
            }))
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            return Err(anyhow!("Request failed: {}", response.status()));
        }

        let body: Value = response
            .json()
            .await
            .context("Failed to parse response body")?;

        // Check for JSON-RPC error
        if let Some(error) = body.get("error") {
            return Err(anyhow!("JSON-RPC error: {}", error));
        }

        Ok(())
    }
}

/// Server manager that spawns and monitors the AHMA server
struct ServerManager {
    child: Child,
    error_flag: Arc<AtomicBool>,
    error_context: Arc<Mutex<Vec<String>>>,
}

impl ServerManager {
    /// Spawn the server and start monitoring its output
    async fn spawn(port: u16, binary: Option<String>) -> Result<Self> {
        let error_flag = Arc::new(AtomicBool::new(false));
        let error_context: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // Determine how to start the server
        let mut cmd = if let Some(binary_path) = binary {
            let c = Command::new(&binary_path);
            println!("Using binary: {}", binary_path);
            c
        } else {
            // Use cargo run
            let mut c = Command::new("cargo");
            c.args(["run", "-p", "ahma_http_bridge", "--"]);
            println!("Using: cargo run -p ahma_http_bridge");
            c
        };

        cmd.arg("--bind-addr")
            .arg(format!("127.0.0.1:{}", port))
            .arg("--default-sandbox-scope")
            .arg(".")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().context("Failed to spawn server")?;

        // Monitor stderr for errors
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stderr"))?;
        let error_flag_clone = error_flag.clone();
        let error_context_clone = error_context.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut recent_lines: Vec<String> = Vec::new();
            const CONTEXT_LINES: usize = 15;

            while let Ok(Some(line)) = reader.next_line().await {
                println!("[SERVER] {}", line);

                // Keep a rolling buffer of recent lines for context
                recent_lines.push(line.clone());
                if recent_lines.len() > CONTEXT_LINES {
                    recent_lines.remove(0);
                }

                // Stop immediately on any error output
                // Detect fatal errors that should abort the stress test
                let is_fatal_error = 
                    // CLI argument errors (e.g., "error: unexpected argument")
                    line.starts_with("error:") ||
                    // Runtime errors
                    (line.contains("ERROR") && !line.contains("0 errors")) ||
                    // Panic
                    line.contains("panicked at") ||
                    // Fatal process errors
                    line.contains("fatal error");

                if is_fatal_error {
                    // Store comprehensive error context
                    let mut ctx = error_context_clone.lock().await;
                    ctx.push(
                        "╔═══════════════════════════════════════════════════════════╗".to_string(),
                    );
                    ctx.push(
                        "║  ERROR CONTEXT (recent server output before error)       ║".to_string(),
                    );
                    ctx.push(
                        "╠═══════════════════════════════════════════════════════════╣".to_string(),
                    );

                    for (i, context_line) in recent_lines.iter().enumerate() {
                        if i == recent_lines.len() - 1 {
                            // Highlight the error line
                            ctx.push(format!(">>> {}", context_line));
                        } else {
                            ctx.push(format!("    {}", context_line));
                        }
                    }

                    ctx.push(
                        "╠═══════════════════════════════════════════════════════════╣".to_string(),
                    );
                    ctx.push(
                        "║  FOLLOWING OUTPUT                                         ║".to_string(),
                    );
                    ctx.push(
                        "╠═══════════════════════════════════════════════════════════╣".to_string(),
                    );

                    error_flag_clone.store(true, Ordering::SeqCst);

                    // Continue reading a few more lines to get full context
                    for _ in 0..10 {
                        if let Ok(Some(extra_line)) = reader.next_line().await {
                            println!("[SERVER] {}", extra_line);
                            ctx.push(format!("    {}", extra_line));
                        } else {
                            break;
                        }
                    }

                    ctx.push(
                        "╚═══════════════════════════════════════════════════════════╝".to_string(),
                    );
                    break;
                }
            }
        });

        Ok(Self {
            child,
            error_flag,
            error_context,
        })
    }

    /// Check if the server has encountered an error
    fn has_error(&self) -> bool {
        self.error_flag.load(Ordering::Relaxed)
    }

    /// Get the error context (surrounding log lines)
    async fn get_error_context(&self) -> Vec<String> {
        self.error_context.lock().await.clone()
    }

    /// Kill the server process
    async fn kill(mut self) -> Result<()> {
        self.child.kill().await.context("Failed to kill server")?;
        Ok(())
    }
}

/// Wait for the server to become healthy
async fn wait_for_health(base_url: &str, timeout_secs: u64) -> Result<()> {
    let client = Client::new();
    let health_url = format!("{}/health", base_url);
    let start = Instant::now();

    loop {
        if start.elapsed().as_secs() > timeout_secs {
            return Err(anyhow!(
                "Server failed to become healthy in {}s",
                timeout_secs
            ));
        }

        match client.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                println!("✓ Server is healthy");
                return Ok(());
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

/// Generate the list of safe commands to use in the stress test
fn get_command_pool() -> Vec<Vec<String>> {
    vec![
        vec!["echo".to_string(), "stress test ping".to_string()],
        vec!["ls".to_string(), "-la".to_string()],
        vec!["whoami".to_string()],
        vec!["date".to_string()],
        vec!["pwd".to_string()],
        vec!["echo".to_string(), "hello world".to_string()],
        vec!["uname".to_string(), "-a".to_string()],
        vec!["printenv".to_string(), "PATH".to_string()],
        vec!["echo".to_string(), "$HOME".to_string()],
        vec!["ls".to_string(), "-lh".to_string()],
        vec!["date".to_string(), "+%Y-%m-%d".to_string()],
        vec!["echo".to_string(), "test".to_string(), "123".to_string()],
    ]
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║       AHMA HTTP Bridge Stress Test                        ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("Configuration:");
    println!("  Port: {}", args.port);
    println!("  Duration: {}s", args.duration);
    println!("  Async clients: {}", args.async_clients);
    println!("  Sync clients: 1");
    println!();

    // Start server
    println!("Starting server...");
    let server = ServerManager::spawn(args.port, args.binary).await?;

    // Wait for server to be healthy (120s timeout for compilation)
    let base_url = format!("http://127.0.0.1:{}", args.port);
    if let Err(e) = wait_for_health(&base_url, 120).await {
        eprintln!("\n❌ Failed to start server: {}", e);
        eprintln!("Cleaning up server process...");
        let _ = server.kill().await;
        std::process::exit(1);
    }

    println!();
    println!("Starting {} client(s)...", args.async_clients + 1);

    // Shared counters
    let success_count = Arc::new(AtomicU64::new(0));
    let error_count = Arc::new(AtomicU64::new(0));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let commands = get_command_pool();
    let mut join_set = JoinSet::new();

    // Spawn 1 synchronous client
    {
        let base_url = base_url.clone();
        let commands = commands.clone();
        let success_count = success_count.clone();
        let error_count = error_count.clone();
        let stop_flag = stop_flag.clone();

        join_set.spawn(async move {
            let mut client = StressClient::new(base_url, true, success_count, error_count);
            let lcg = Lcg::new(12345);
            client.initialize().await?;
            println!("✓ Sync client initialized");
            client.run_commands(commands, lcg, stop_flag).await
        });
    }

    // Spawn N asynchronous clients
    for i in 0..args.async_clients {
        let base_url = base_url.clone();
        let commands = commands.clone();
        let success_count = success_count.clone();
        let error_count = error_count.clone();
        let stop_flag = stop_flag.clone();

        join_set.spawn(async move {
            let mut client = StressClient::new(base_url, false, success_count, error_count);
            let lcg = Lcg::new(67890 + i as u64);
            client.initialize().await?;
            println!("✓ Async client {} initialized", i + 1);
            client.run_commands(commands, lcg, stop_flag).await
        });
    }

    println!();
    println!("Running stress test...");
    println!();

    // Run for the specified duration
    let start = Instant::now();
    let duration = Duration::from_secs(args.duration);

    loop {
        // Check if server encountered an error - ABORT EVERYTHING IMMEDIATELY
        if server.has_error() {
            // Signal clients to stop IMMEDIATELY
            stop_flag.store(true, Ordering::SeqCst);

            eprintln!();
            eprintln!("╔═══════════════════════════════════════════════════════════╗");
            eprintln!("║             ❌ SERVER ERROR DETECTED                       ║");
            eprintln!("╚═══════════════════════════════════════════════════════════╝");
            eprintln!();

            // Print the full error context
            for line in server.get_error_context().await {
                eprintln!("{}", line);
            }

            eprintln!();
            eprintln!("ABORTING: Stress test terminated due to server error.");
            eprintln!();

            // Abort all client tasks immediately
            join_set.abort_all();

            // Kill server
            let _ = server.kill().await;

            // Exit with error code
            std::process::exit(1);
        }

        // Check if duration elapsed
        if start.elapsed() >= duration {
            println!("\n✓ Test duration completed");
            break;
        }

        // Check for excessive client errors (abort if all requests are failing)
        let success = success_count.load(Ordering::Relaxed);
        let errors = error_count.load(Ordering::Relaxed);
        
        // If we have many errors and no successes, something is fundamentally broken
        if errors >= 10 && success == 0 {
            stop_flag.store(true, Ordering::SeqCst);
            
            eprintln!();
            eprintln!("╔═══════════════════════════════════════════════════════════╗");
            eprintln!("║       ❌ TOO MANY CLIENT ERRORS - ABORTING                 ║");
            eprintln!("╚═══════════════════════════════════════════════════════════╝");
            eprintln!();
            eprintln!("  Errors: {} with 0 successes", errors);
            eprintln!("  This indicates a fundamental problem with the server or protocol.");
            eprintln!();
            eprintln!("  Common causes:");
            eprintln!("    - 409 Conflict: Sandbox not initialized (missing roots/list response)");
            eprintln!("    - 401/403: Authentication issues");
            eprintln!("    - 404: Wrong endpoint URL");
            eprintln!();
            
            join_set.abort_all();
            let _ = server.kill().await;
            std::process::exit(1);
        }

        // Print progress every 5 seconds
        if start.elapsed().as_secs() % 5 == 0 {
            let elapsed = start.elapsed().as_secs();
            let rate = if elapsed > 0 { success / elapsed } else { 0 };
            println!(
                "[{:3}s] Success: {} | Errors: {} | Rate: {}/s",
                elapsed, success, errors, rate
            );
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Signal clients to stop
    stop_flag.store(true, Ordering::Relaxed);

    println!();
    println!("Stopping clients...");

    // Wait for all clients to finish
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => eprintln!("Client error: {}", e),
            Err(e) => eprintln!("Task error: {}", e),
        }
    }

    // Check for errors before stopping server
    let had_server_error = server.has_error();

    // Stop server
    println!("Stopping server...");
    server.kill().await?;

    // Final report
    let success = success_count.load(Ordering::Relaxed);
    let errors = error_count.load(Ordering::Relaxed);
    let total = success + errors;
    let success_rate = if total > 0 {
        (success as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    println!();
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║                     Test Results                          ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!("  Total requests: {}", total);
    println!("  Successful: {} ({:.1}%)", success, success_rate);
    println!("  Errors: {}", errors);
    println!("  Duration: {:.1}s", start.elapsed().as_secs_f64());
    println!(
        "  Average rate: {:.1} req/s",
        total as f64 / start.elapsed().as_secs_f64()
    );
    println!();

    if errors > 0 || had_server_error {
        eprintln!("❌ Test completed with errors");
        std::process::exit(1);
    } else {
        println!("✓ Test completed successfully");
        Ok(())
    }
}
