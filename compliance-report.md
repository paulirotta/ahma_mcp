# Compliance Report — Module: ahma_core

## Summary (short)

- Scope: Compare `ahma_core` code + tests against project `requirements.md` (relevant sections include R1, R2, R3, R4, R5, R6, R7, R8).
- Status: **Partial compliance** — most core requirements are implemented and tested, but a few notable mismatches were found and are recorded below.

---

## ✅ Requirements that appear satisfied (evidence)

- R6.2 (Core library): `ahma_core` is a library crate that exposes core types and APIs.
  - Evidence: `ahma_core/Cargo.toml` defines `[lib] name = "ahma_core"` and `src/lib.rs` re-exports `Adapter` and `AhmaMcpService`.

- R2 (Async-first behavior): Async operations return `operation_id`, operation monitoring and progress notifications are implemented and well tested.
  - Evidence: `ahma_core/src/operation_monitor.rs`, `ahma_core/src/mcp_callback.rs`; tests: `tests/mcp_callback_coverage_test.rs`, `tests/notification_tests/async_tracking.rs`, `tests/client_coverage_expansion_test.rs`.

- R1 (Tool config JSON & Hot-reloading): Tools are defined in JSON files and `start_config_watcher` implements file-watching with `notify` and sends `tools/list_changed` notifications on change.
  - Evidence: `ahma_core/src/mcp_service/mod.rs` function `start_config_watcher` and tests `tests/mcp_service_mod_unit_test.rs` (debounce logic) and coverage tests asserting `list_changed` capability.

- R5 (MTDF Schema validation): `schema_validation` module implements validation, error types, formatting and performance checks with tests for edge cases and large inputs.
  - Evidence: `ahma_core/src/schema_validation.rs` and tests in `tests/schema_validation/*` (comprehensive, performance, message quality).

- R7 (Sandbox enforcement core primitives): Sandbox module provides initialization, canonicalization, path-checking, and platform-specific checks for Linux and macOS; path validation and path-security tests exist.
  - Evidence: `ahma_core/src/sandbox.rs`, `ahma_core/src/path_security.rs`, tests in `tests/sandbox_coverage_test.rs`, `tests/macos_sandbox_integration_test.rs`, `tests/linux_sandbox_integration_test.rs`.

- R8 (HTTP bridge handshake & session isolation): `mcp_service` defers sandbox initialization via `--defer-sandbox`, and `configure_sandbox_from_roots` processes `roots/list`; HTTP bridge side implements the handshake and session sandbox locking with tests.
  - Evidence: `ahma_core/src/mcp_service/mod.rs` (methods `configure_sandbox_from_roots`, `on_roots_list_changed`) and extensive tests in `ahma_http_bridge/tests/*` (sandbox_roots_handshake_test, http_roots_handshake_integration_test).

---

## ⚠️ Notable mismatches (detailed)

1) R7.6: Required behavior on nested sandbox environments (macOS) — Implementation differs from requirement

- Requirement (R7.6.1-R7.6.3): "The system **must** detect when it is running inside another sandbox... Upon detection, the system **must** exit with a clear error message instructing the user to disable the internal sandbox using `--no-sandbox` or `AHMA_NO_SANDBOX=1`." (See `requirements.md` R7.6)

- Actual behavior (code): `sandbox::test_sandbox_exec_available()` detects nested sandbox conditions and returns `SandboxError::NestedSandboxDetected`.
  - But `ahma_core/src/shell/main_logic.rs` handles this error by logging warnings and calling `sandbox::enable_test_mode()` (which effectively disables sandboxing) and continues startup rather than exiting.
  - Files/locations (precise):
    - Detection: `ahma_core/src/sandbox.rs` function `test_sandbox_exec_available()` — **lines ~368-405** (returns `SandboxError::NestedSandboxDetected` on nested detection).
    - Handling: `ahma_core/src/shell/main_logic.rs` — **lines ~215-240** (main startup logic handles the error by logging warnings and calling `sandbox::enable_test_mode()`; see the `if let Err(e) = sandbox::test_sandbox_exec_available()` branch which sets `no_sandbox = true`).

      Example excerpt:
      ```text
      if let Err(e) = sandbox::test_sandbox_exec_available() {
          match e {
              SandboxError::NestedSandboxDetected => {
                  tracing::warn!("Nested sandbox detected - Ahma is running inside another sandbox");
                  tracing::warn!("Ahma's sandbox will be disabled; the outer sandbox provides security");
                  tracing::info!("To suppress this warning, use --no-sandbox or set AHMA_NO_SANDBOX=1");
                  sandbox::enable_test_mode();
                  no_sandbox = true;
              }
              _ => sandbox::exit_with_sandbox_error(&e);
          }
      }
      ```
- Tests: macOS integration tests (`ahma_core/tests/macos_sandbox_integration_test.rs`) skip when running inside a nested sandbox; they do not assert that the server exits with an error on nested detection.

- Conclusion: This is a requirement vs implementation mismatch. The requirement demands a *fail-secure exit* on nested sandbox; the current code prefers *graceful degradation* (disable sandbox and continue). This is a security-sensitive variance and should be flagged for correction.

---

2) R6.9: Workspace `default-members` mismatch

- Requirement (R6.9): "The root `Cargo.toml` **must** define `default-members = ["ahma_shell"]` so that `cargo run` executes the main MCP server binary by default."

- Actual state: Top-level `Cargo.toml` defines `default-members = ["ahma_core", "ahma_validate", "generate_tool_schema", "ahma_http_bridge", "ahma_http_mcp_client"]` (multiple members), and no separate `ahma_shell` crate exists; the server binary is implemented as `[[bin]] name = "ahma_mcp"` inside `ahma_core/Cargo.toml`.
  - Files/locations:
    - Root: `/Cargo.toml` — `[workspace] default-members` definition appears at the top of the file (e.g., **lines ~1-8**; currently set to multiple crates instead of `["ahma_shell"]`).
    - Binary: `/ahma_core/Cargo.toml` has `[[bin]] name = "ahma_mcp" path = "src/shell/bin.rs"` (see **lines ~1-12** of that file).

- Conclusion: This is an architecture/configuration mismatch: the workspace layout does not match the R6 expectation that a dedicated `ahma_shell` binary crate would be the workspace default member. This inconsistency affects developer UX (running `cargo run`), automation, and the formal project structure requirement.

---

## Minor notes / observations (informational)

- Many tests indicate awareness of known limitations (e.g., broad file-read* allowances, network access in Seatbelt profiles). These are documented as "KNOWN LIMITATION" tests (see `tests/sandbox_security_red_team_test.rs`). This is consistent with requirements that document allowed/required Seatbelt rules (R7.3.5) and explicit caveats.

- Schema fields: tests use `force_synchronous` in JSON test configs while `config.rs` uses `synchronous` with a Serde `alias = "force_synchronous"`; the alias ensures backward compatibility (validated in tests).

- `format: "path"` support and `file_arg`/`file_flag` semantics are present: `config` includes `file_arg`/`file_flag`, `adapter` uses them to create temp files when needed, and `path_security` provides validation. Tests exist to cover these flows (adapter and tool-suite tests).

---

## Next steps (per plan)

1. Finalize `ahma_core` section in the `compliance-report.md` (this file).
2. Continue module-by-module review: proceed to `ahma_http_bridge` and repeat (read module-specific `requirements.md` if present, review code and tests, document mismatches and confirmations).

---

(End of `ahma_core` module report)

---

# Compliance Report — Module: ahma_http_bridge

## Summary (short)

- Scope: Compare `ahma_http_bridge` code + tests against `ahma_http_bridge/requirements.md` (session isolation, deferred sandbox, restart/initialization behavior, handshake timeouts and safety invariants).
- Status: **Mostly compliant** — core handshake, sandbox locking, timeout semantics and tests are implemented and thorough; one API-level mismatch found concerning session termination.

---

## ✅ Requirements that appear satisfied (evidence)

- Session isolation and onboarding handshake (R8.4): Implemented via per-session `SessionManager` and handshake state machine.
  - Evidence: `ahma_http_bridge/src/session.rs` `HandshakeState`, `mark_mcp_initialized`, `mark_sse_connected`, and tests `tests/session_sandbox_test.rs`, `tests/handshake_state_machine_test.rs`.

- Deferred sandbox + request gating: `tools/call` requests are blocked with clear errors (HTTP 409 with code -32001) until sandbox is locked; handshake timeouts return 504 with code -32002.
  - Evidence: `ahma_http_bridge/src/bridge.rs` (blocking logic and error codes) and tests `tests/handshake_timeout_test.rs`, `tests/http_bridge_integration_test.rs` that assert 409 and 504 semantics.

- Restart & initialization correctness: Bridge waits for `notifications/initialized` and confirms handshake completion before marking the session ready; tests ensure subprocess restarts do not result in forwarding requests to uninitialized subprocess (no "expect initialized request" leak).
  - Evidence: `ahma_http_bridge/src/bridge.rs` and tests `tests/http_bridge_integration_test.rs` for restart & initialization verification.

---

## ⚠️ Notable mismatch (detailed)

1) R8.4.7: HTTP DELETE with session ID → Clean session termination (missing HTTP endpoint)

- Requirement (R8.4.7): "HTTP DELETE with `Mcp-Session-Id` terminates session and subprocess."

- Actual state: The `SessionManager` supports `terminate_session` (programmatic API) and there are unit/integration tests that call it directly (session cleanup/termination is tested extensively). However, the HTTP router does **not** expose an HTTP DELETE route for sessions: only `POST /mcp` and `GET /mcp` endpoints are registered in `ahma_http_bridge/src/bridge.rs`.
  - Files/locations:
    - Session termination API: `ahma_http_bridge/src/session.rs` (`terminate_session`) and tests `ahma_http_bridge/tests/session_sandbox_test.rs`, `session_coverage_test.rs` that validate termination behavior at API level (**terminate_session implementation at lines ~729-780**).
    - Router registration: `ahma_http_bridge/src/bridge.rs` sets up routes at **lines ~81-83** (and a secondary router at **~734-736**), e.g.: `.route("/mcp", post(handle_mcp_request).get(handle_sse_stream))`. There is **no `DELETE` route registered** to accept `DELETE /mcp` with a `Mcp-Session-Id` header.

    Evidence excerpt (router registration):
    ```text
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request).get(handle_sse_stream))
    ```

    The absence of a `DELETE` handler means the API-level requirement (HTTP DELETE to terminate a session) is not satisfied by an HTTP endpoint, despite having an internal `terminate_session` API.

- Implication: External clients (e.g., IDEs) cannot terminate sessions via the HTTP DELETE method as required by the spec; session termination is only possible via internal calls (or via sending `notifications/roots/list_changed` which may also terminate sessions for security reasons). This is an API-level non-compliance with the stated requirement.

- Recommended action (documented only): Add `DELETE /mcp` route that checks `Mcp-Session-Id` header and calls `session_manager.terminate_session(..., ClientRequested)` returning appropriate HTTP status (204 No Content on success or 404/403 for missing/terminated sessions).

---

## Minor observations

- The bridge's approach of returning clear 409/504 responses during handshake/timeout matches the requirement's allowance to "queue or handle gracefully" incoming requests during sandbox initialization. Tests assert both conflict and timeout behaviors.

- Tests comprehensively cover handshake ordering, concurrency, timeouts, and restart conditions. This indicates strong alignment with the requirements for robust session management.

---

## Next steps (per plan)

1. Record the `ahma_http_bridge` findings in `compliance-report.md` (this section).
2. Move to next module: `ahma_http_mcp_client` for the same analysis and add findings.

---

(End of `ahma_http_bridge` module report)

---

# Compliance Report — Module: ahma_http_mcp_client

## Summary (short)

- Scope: Review `ahma_http_mcp_client` for transport semantics, authentication support, token persistence, and message handling.
- Status: **Compliant** — implementation matches expectations and tests cover token storage and basic transport behavior.

---

## ✅ Requirements / expectations satisfied (evidence)

- Optional OAuth support implemented with token persistence and restore.
  - Evidence: `ahma_http_mcp_client/src/client.rs` contains `perform_oauth_flow`, `save_token`, `load_token`, and tests `load_token_returns_none_when_override_missing`, `save_token_round_trips_via_override_path`.

- Transport implementation matches rmcp transport API and handles JSON-RPC POSTs with bearer auth and response parsing.
  - Evidence: `impl Transport<RoleClient> for HttpMcpTransport` in `client.rs` (send/receive/close methods).

- Error handling and integration-ready example provided (Atlassian example). README documents usage and examples.

---

## Minor notes

- OAuth flow uses ephemeral local listener for redirect (http://localhost:8080), which is acceptable for CLI-style flows. Token file path is controlled by `AHMA_HTTP_CLIENT_TOKEN_PATH` and defaults to temp dir; tests guard env usage.

- No high-risk mismatches found in this module at the time of review.

---

## Next steps

1. Add this section to the report (done).
2. Proceed to review `ahma_validate` and `generate_tool_schema` (validation and MTDF schema generation) for R5 and R6 compliance.

---

(End of `ahma_http_mcp_client` module report)

---

# Compliance Report — Module: ahma_validate & generate_tool_schema

## Summary (short)

- Scope: Validate the schema validator (`ahma_validate`) and MTDF JSON Schema generation (`generate_tool_schema`) against R5 (MTDF validation) and R6.5 (schema generation).
- Status: **Compliant** — `ahma_validate` uses `ahma_core::schema_validation::MtdfValidator` and tests cover expected success/failure paths; `generate_tool_schema` produces the `mtdf-schema.json` and has tests for output and file writing.

---

## ✅ Evidence and mapping to requirements

- R5 (JSON Schema & Validation): `ahma_validate` uses `MtdfValidator` (from `ahma_core`) to validate tool JSON files and fails with non-zero exit code for invalid configs. Tests cover single-file, directory, comma-separated targets, invalid JSON, and missing guidance file cases.
  - Files: `ahma_validate/src/main.rs`, tests in same file.

- R6.5 (MTDF JSON Schema generation): `generate_tool_schema` generates `mtdf-schema.json` using `schemars::schema_for!(ToolConfig)`, writes to the `docs` directory, and has tests for generation and file writing.
  - Files: `generate_tool_schema/src/main.rs` and its tests.

- Non-functional / Testing Standards (coverage): `ahma_validate/requirements.md` documents strict coverage targets; tests in `main.rs` cover a wide range of input scenarios and follow `tempfile` isolation patterns.

---

## Minor notes

- `generate_tool_schema` produces a pretty-printed JSON Schema whose structure includes `$schema` or `$defs`, as expected by schema consumers.
- Both crates use `ahma_core` types and are well integrated with the core schema and config types.

---

## Next steps

1. Add this section to the report (done).
2. Consolidate findings and perform a final pass to ensure `compliance-report.md` includes precise file references and line ranges for each mismatch.

---

---

## Tests to add (suggested evidence gaps)

- **Nested sandbox detection fail-secure test**: Add an integration test for macOS that simulates nested sandbox detection and asserts the server **exits** with a clear error message (matching R7.6), rather than silently disabling sandbox.
  - Target: `ahma_core/tests/` (new test), reference code `ahma_core/src/sandbox.rs` `test_sandbox_exec_available()` (lines ~368-405) and `ahma_core/src/shell/main_logic.rs` branch (lines ~215-240).

- **HTTP DELETE session termination test**: Add an HTTP-level test asserting that `DELETE /mcp` with `Mcp-Session-Id` terminates the session (returns 204 No Content on success) and cleans up subprocesses (matching R8.4.7).
  - Target: `ahma_http_bridge/tests/` (new integration test); currently `SessionManager::terminate_session()` is tested at the API level (lines ~729-780) but not via HTTP.

---

(End of `ahma_validate` & `generate_tool_schema` module report)
