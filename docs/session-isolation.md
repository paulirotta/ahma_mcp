# Session Isolation for HTTP Mode

This document describes the architecture for implementing per-session sandbox isolation in HTTP mode. Session isolation allows multiple VS Code, Cursor, or other MCP clients on the same machine to connect to a single HTTP server instance, each with their own isolated sandbox scope.

## Overview

When `--session-isolation` is passed to the HTTP bridge, the server spawns a separate `ahma_mcp` subprocess per MCP session. Each subprocess has its own sandbox scope derived from the client's workspace roots, providing complete isolation between concurrent sessions.

## How It Works

### Protocol Flow

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  VS Code #1     │     │  VS Code #2     │     │  Cursor         │
│  Project: /foo  │     │  Project: /bar  │     │  Project: /baz  │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         │ POST initialize       │ POST initialize       │ POST initialize
         ▼                       ▼                       ▼
┌────────────────────────────────────────────────────────────────────┐
│                    ahma_mcp HTTP Bridge                            │
│                    (--session-isolation mode)                      │
│                                                                    │
│  1. Receive initialize request                                     │
│  2. Generate session ID (UUID)                                     │
│  3. Spawn ahma_mcp subprocess (sandbox TBD)                        │
│  4. Forward initialize to subprocess                               │
│  5. Subprocess requests roots/list                                 │
│  6. Bridge responds with empty roots (not yet known)               │
│  7. Return InitializeResult with Mcp-Session-Id header             │
│                                                                    │
│  SessionManager {                                                  │
│    sessions: {                                                     │
│      "session-abc": { subprocess, sandbox: None }                  │
│      "session-def": { subprocess, sandbox: None }                  │
│      "session-ghi": { subprocess, sandbox: None }                  │
│    }                                                               │
│  }                                                                 │
└────────────────────────────────────────────────────────────────────┘
         │                       │                       │
         │ Mcp-Session-Id:       │ Mcp-Session-Id:       │ Mcp-Session-Id:
         │ session-abc           │ session-def           │ session-ghi
         ▼                       ▼                       ▼
```

### Sandbox Scope Binding

The sandbox scope is determined lazily via the MCP `roots/list` protocol:

1. **Client sends `initialize`** with `capabilities.roots: { listChanged: true }`
2. **Server spawns subprocess** without sandbox restriction initially
3. **Subprocess sends `roots/list` request** to get workspace folders
4. **Bridge intercepts and caches** the first root as sandbox scope
5. **Subsequent file operations** are validated against this scope

**Important**: The sandbox scope is set **once** when the first `roots/list` response is received and cannot be changed for that session. This maintains the security invariant.

### Handling Roots Changes (Security)

The MCP protocol allows clients to send `notifications/roots/list_changed` when workspace folders change. However, for security reasons, **session isolation mode rejects roots changes after the sandbox is locked**:

1. **If `notifications/roots/list_changed` is received after sandbox lock**:

   - Log an error with session ID and attempted change
   - Immediately terminate the subprocess
   - Mark session as terminated with reason `"roots_change_rejected"`
   - Return HTTP 403 Forbidden for subsequent requests with that session ID

2. **Why this is necessary**:

   - Prevents sandbox escape via workspace folder changes
   - Maintains the "set once, immutable" security model
   - Aligns with kernel-level sandbox enforcement (cannot change Landlock/Seatbelt mid-process)

3. **Client impact**:
   - IDE must reconnect with a new session if workspace changes
   - This is the expected behavior for security-conscious deployments
   - Normal workflow (opening one project per session) is unaffected

### MCP Session Management (Per Spec 2025-03-26)

The MCP Streamable HTTP transport defines session management:

1. **Server assigns `Mcp-Session-Id`** on `InitializeResult` response header
2. **Client MUST include `Mcp-Session-Id`** on all subsequent requests
3. **Server routes requests** to the correct subprocess based on session ID
4. **Session termination**: Client sends HTTP DELETE, server stops subprocess

**VS Code/Cursor Compliance**: Both VS Code and Cursor implement the MCP specification and will automatically:

- Send the `Mcp-Session-Id` header on requests after initialize
- Handle session termination properly
- Each IDE instance creates a separate MCP connection with its own session

### Why This Works for Multiple IDE Instances

Each VS Code or Cursor window:

1. Opens a workspace folder (e.g., `/Users/paul/project-a`)
2. Starts an MCP client connection to the HTTP server
3. Sends its own `initialize` request
4. Receives a unique `Mcp-Session-Id`
5. Reports its workspace folder via `roots/list` response

Since each IDE window is a separate process with its own MCP client, they naturally get separate sessions with isolated sandboxes.

## Implementation

### CLI Flag

```bash
# Single-session mode (default, current behavior)
ahma_mcp --mode http --http-port 3000

# Multi-session isolation mode
ahma_mcp --mode http --http-port 3000 --session-isolation
```

### SessionManager Structure

```rust
use dashmap::DashMap;
use std::path::PathBuf;
use tokio::process::Child;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct Session {
    /// The subprocess running ahma_mcp in stdio mode
    subprocess: Child,
    /// Channel to send messages to the subprocess
    sender: mpsc::Sender<String>,
    /// Sandbox scope (set on first roots/list response)
    sandbox_scope: Option<PathBuf>,
    /// Whether sandbox has been locked (cannot change after first set)
    sandbox_locked: bool,
}

pub struct SessionManager {
    sessions: DashMap<String, Session>,
    /// Default sandbox scope if client provides no roots
    default_scope: PathBuf,
}

impl SessionManager {
    pub fn new(default_scope: PathBuf) -> Self {
        Self {
            sessions: DashMap::new(),
            default_scope,
        }
    }

    /// Create a new session, spawning a subprocess
    pub async fn create_session(&self) -> Result<String, Error> {
        let session_id = Uuid::new_v4().to_string();

        // Spawn subprocess WITHOUT sandbox restriction initially
        // Sandbox will be applied when roots/list is received
        let mut child = Command::new("ahma_mcp")
            .args(["--mode", "stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let (sender, receiver) = mpsc::channel(100);
        // ... setup message routing ...

        self.sessions.insert(session_id.clone(), Session {
            subprocess: child,
            sender,
            sandbox_scope: None,
            sandbox_locked: false,
        });

        Ok(session_id)
    }

    /// Lock sandbox scope for a session (called on first roots/list response)
    pub fn lock_sandbox(&self, session_id: &str, roots: &[Root]) -> Result<(), Error> {
        let mut session = self.sessions.get_mut(session_id)
            .ok_or(Error::SessionNotFound)?;

        if session.sandbox_locked {
            return Err(Error::SandboxAlreadyLocked);
        }

        // Use first root as sandbox scope, or default if no roots
        let scope = roots.first()
            .map(|r| PathBuf::from(r.uri.strip_prefix("file://").unwrap_or(&r.uri)))
            .unwrap_or_else(|| self.default_scope.clone());

        session.sandbox_scope = Some(scope);
        session.sandbox_locked = true;

        Ok(())
    }

    /// Route a message to the correct session
    pub async fn send(&self, session_id: &str, message: &str) -> Result<(), Error> {
        let session = self.sessions.get(session_id)
            .ok_or(Error::SessionNotFound)?;
        session.sender.send(message.to_string()).await?;
        Ok(())
    }

    /// Terminate a session
    pub async fn terminate(&self, session_id: &str) -> Result<(), Error> {
        if let Some((_, mut session)) = self.sessions.remove(session_id) {
            session.subprocess.kill().await?;
        }
        Ok(())
    }

    /// Handle roots/list_changed notification - terminates session if sandbox is locked
    pub async fn handle_roots_changed(&self, session_id: &str) -> Result<(), Error> {
        let session = self.sessions.get(session_id)
            .ok_or(Error::SessionNotFound)?;

        if session.sandbox_locked {
            // Security violation: attempt to change roots after sandbox lock
            tracing::error!(
                session_id = %session_id,
                sandbox_scope = ?session.sandbox_scope,
                "Roots change rejected after sandbox lock - terminating session"
            );
            drop(session); // Release lock before terminate
            self.terminate(session_id).await?;
            return Err(Error::RootsChangeRejected);
        }

        // Sandbox not yet locked - this shouldn't happen in normal flow
        // but allow it as roots/list hasn't been processed yet
        Ok(())
    }
}
```

### HTTP Handler Changes

```rust
async fn handle_mcp_post(
    State(state): State<BridgeState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> impl IntoResponse {
    // Check for session ID header
    let session_id = headers
        .get("Mcp-Session-Id")
        .and_then(|h| h.to_str().ok())
        .map(String::from);

    let method = request.get("method").and_then(|m| m.as_str());

    match (method, &session_id) {
        // Initialize request - create new session
        (Some("initialize"), None) => {
            let session_id = state.session_manager.create_session().await?;

            // Forward initialize to subprocess
            let response = state.session_manager
                .forward_and_wait(&session_id, &request).await?;

            // Return response with session ID header
            (
                StatusCode::OK,
                [(header::HeaderName::from_static("mcp-session-id"), session_id)],
                Json(response),
            )
        }

        // Roots change notification - reject if sandbox locked
        (Some("notifications/roots/list_changed"), Some(ref session_id)) => {
            match state.session_manager.handle_roots_changed(session_id).await {
                Ok(()) => (StatusCode::ACCEPTED, Json(json!({}))),
                Err(Error::RootsChangeRejected) => {
                    // Session terminated due to security violation
                    (
                        StatusCode::FORBIDDEN,
                        Json(json!({
                            "error": {
                                "code": -32600,
                                "message": "Session terminated: roots change not allowed after sandbox lock"
                            }
                        })),
                    )
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
            }
        }

        // All other requests - route to existing session
        (_, Some(ref session_id)) => {
            let response = state.session_manager
                .forward_and_wait(session_id, &request).await?;
            (StatusCode::OK, Json(response))
        }

        // Non-initialize without session ID
        (_, None) => {
            (StatusCode::BAD_REQUEST, Json(json!({"error": "Missing Mcp-Session-Id header"})))
        }
    }
}
```

## Current Behavior (Without --session-isolation)

Without the `--session-isolation` flag, the HTTP bridge maintains current behavior:

- Single subprocess handles all connections
- Sandbox scope set at server startup via `--sandbox-scope` or CWD
- All clients share the same sandbox

### Workaround for Multi-Project Use

Run separate HTTP server instances:

```bash
# Terminal 1: Project A
cd /path/to/project-a
ahma_mcp --mode http --http-port 3001

# Terminal 2: Project B
cd /path/to/project-b
ahma_mcp --mode http --http-port 3002
```

## Security Considerations

### Sandbox Scope Immutability

The sandbox scope is locked on the **first `roots/list` response** and cannot be changed for that session. This prevents:

1. Malicious requests attempting to escalate privileges mid-session
2. AI agents convincing the system to expand their sandbox scope
3. Race conditions where multiple roots responses could conflict

**Enforcement**: If a client sends `notifications/roots/list_changed` after the sandbox is locked, the session is **immediately terminated** and subsequent requests return HTTP 403 Forbidden. This is a hard security boundary.

### Kernel-Level Enforcement

On Linux (Landlock) and macOS (Seatbelt), sandbox restrictions are enforced at the kernel level within each subprocess. Even if the HTTP bridge is compromised, individual session subprocesses maintain their sandbox constraints.

### Trust Model

Session isolation operates under the same trust model as the rest of ahma_mcp:

- **Local development only**: HTTP mode binds to localhost by default
- **Trusted clients**: VS Code, Cursor, and CLI tools are trusted
- **No authentication**: Assumes all local connections are legitimate

For multi-user or network deployment, additional authentication would be required.

### Default Sandbox Fallback

If a client provides no roots (empty `roots/list` response), the session uses the HTTP server's working directory as the sandbox scope. This provides a sensible default while maintaining security.

## Implementation Priority

Session isolation is a **medium priority** feature because:

1. **STDIO mode handles the common case** - IDEs spawn one server per workspace
2. **Running multiple HTTP instances** is a viable workaround
3. **Developer experience improvement** - Single server for multiple projects is more convenient
4. **Implementation complexity** is manageable with the `Mcp-Session-Id` approach

## Related Requirements

- R7.1.3: HTTP mode sandbox scope configuration
- R7.1.7: Single-sandbox limitation documentation
- R7.1.8: Local development only security model
- R8D: Session isolation for HTTP mode
