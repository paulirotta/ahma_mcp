# Session Isolation for HTTP Mode (Future Work)

This document describes the architectural considerations for implementing per-session sandbox isolation in HTTP mode. This is **not currently implemented** but is documented here for future development.

## Current Limitation

In the current implementation, HTTP bridge mode spawns a **single** `ahma_mcp` subprocess that handles all client connections. All clients share the same sandbox scope, which is set once at server startup.

### Why This Is a Limitation

When multiple developers or projects want to use the same HTTP server:

- All clients are constrained to the same sandbox directory
- No project isolation between concurrent users
- Cannot dynamically switch projects without restarting the server

### Current Workaround

Run separate HTTP server instances for each project:

```bash
# Terminal 1: Project A
cd /path/to/project-a
ahma_mcp --mode http --http-port 3001

# Terminal 2: Project B
cd /path/to/project-b
ahma_mcp --mode http --http-port 3002
```

## Future Architecture: Per-Session Sandbox Isolation

### Option 1: Session-Based Subprocess Pool

Spawn a separate `ahma_mcp` subprocess per session, each with its own sandbox scope.

**Pros:**

- Complete isolation between sessions
- Crash in one session doesn't affect others
- Sandbox scope set at subprocess spawn time (immutable per session)

**Cons:**

- Higher memory usage (one process per session)
- Subprocess spawn overhead on new sessions
- More complex session management

**Implementation Sketch:**

```rust
struct SessionManager {
    sessions: HashMap<SessionId, ChildProcess>,
}

impl SessionManager {
    async fn get_or_create_session(&mut self, session_id: &str, sandbox_scope: &Path) -> &ChildProcess {
        if !self.sessions.contains_key(session_id) {
            let child = Command::new("ahma_mcp")
                .args(["--mode", "stdio", "--sandbox-scope", sandbox_scope.to_str().unwrap()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?;
            self.sessions.insert(session_id.to_string(), child);
        }
        &self.sessions[session_id]
    }
}
```

### Option 2: MCP Initialize with Roots

Use the MCP protocol's `initialize` method to pass workspace roots from the client.

**MCP Spec Reference:**
The MCP `initialize` request can include a `roots` array specifying workspace directories:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "example", "version": "1.0" },
    "roots": [{ "uri": "file:///path/to/project", "name": "My Project" }]
  }
}
```

**Implementation:**

- First `initialize` request sets the sandbox scope for that session
- Subsequent requests use the established scope
- Requires session tracking via HTTP cookies or headers

**Pros:**

- Standard MCP protocol mechanism
- Client controls workspace root
- Simpler than subprocess pool

**Cons:**

- Must track sessions (stateful)
- Security: must trust client's declared roots (only safe for local dev)

### Option 3: Header-Based Sandbox Selection

Accept a custom HTTP header (e.g., `X-Sandbox-Scope`) on requests.

**Implementation:**

```rust
async fn handle_request(headers: HeaderMap, body: Json<Value>) {
    let sandbox_scope = headers
        .get("X-Sandbox-Scope")
        .map(|h| PathBuf::from(h.to_str().unwrap()))
        .unwrap_or_else(|| default_sandbox_scope());

    // Route to appropriate subprocess or validate path
}
```

**Pros:**

- Simple to implement
- Stateless
- Easy for clients to use

**Cons:**

- **Security risk**: Client can change sandbox on every request
- Violates "set once, immutable" security model
- NOT recommended for anything beyond local dev

## Security Considerations

### Why Per-Request Sandbox Changes Are Dangerous

The current security model states:

> "The sandbox scope is set once at server/session initialization and cannot be changed during the session."

This is intentional. If an AI can change its sandbox scope during a session, it could:

1. Start constrained to `/home/user/project`
2. Convince the system to switch to `/`
3. Access any file on the system

### Recommended Approach

If implementing session isolation, the recommended approach is **Option 1 (Session-Based Subprocess Pool)** because:

- Each subprocess has its sandbox scope set at spawn time via `--sandbox-scope`
- The sandbox is enforced by Landlock/Seatbelt at the kernel level
- Once the subprocess starts, its sandbox cannot be changed
- Session identification happens at the HTTP layer, not inside the sandbox

### Trust Model for HTTP Mode

HTTP mode is designed for **local development only**:

- The server runs on localhost
- Clients are trusted (VS Code, Cursor, CLI tools)
- Network exposure is not supported

If multi-user or network deployment is needed in the future, additional authentication and authorization mechanisms would be required.

## Implementation Priority

This feature is **low priority** because:

1. STDIO mode handles the common case (IDE spawns server per workspace)
2. Running multiple HTTP instances is a viable workaround
3. HTTP mode is primarily for debugging and advanced use cases
4. Implementation complexity is high for marginal benefit

## Related Requirements

- R7.1.3: HTTP mode sandbox scope configuration
- R7.1.7: Single-sandbox limitation documentation
- R7.1.8: Local development only security model
