//! This test file used to validate the old subprocess restart + handshake replay behavior.
//!
//! That restart/replay mechanism has been intentionally removed in favor of
//! delaying tool execution until sandbox scopes are locked from client roots.
//!
//! The replacement coverage lives in:
//! - ahma_http_bridge/src/bridge.rs (session isolation gating test)
