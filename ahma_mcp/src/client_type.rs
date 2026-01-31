//! # MCP Client Type Detection
//!
//! This module provides client type detection based on the `Implementation.name` field
//! sent by MCP clients during initialization. It allows the server to adjust behavior
//! based on known client quirks.
//!
//! ## Known Client Issues
//!
//! - **Cursor**: Logs errors for progress notifications with unknown tokens, even when
//!   the server correctly uses the client-provided `progressToken`. To avoid noisy
//!   error logs in Cursor, we skip sending progress notifications entirely for this client.
//!
//! - **VSCode/Copilot**: Handles progress notifications correctly.
//!
//! - **Claude Desktop**: Handles progress notifications correctly.

use rmcp::service::{Peer, RoleServer};

/// Represents known MCP client types with their behavioral quirks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpClientType {
    /// Cursor IDE - has issues with progress notification token handling.
    /// We skip progress notifications for this client.
    Cursor,
    /// VS Code / GitHub Copilot - handles progress notifications correctly.
    VSCode,
    /// Claude Desktop - handles progress notifications correctly.
    ClaudeDesktop,
    /// Zed editor
    Zed,
    /// Unknown client - optimistically assume progress is supported.
    #[default]
    Unknown,
}

impl McpClientType {
    /// Detect client type from the `Implementation.name` field sent during MCP initialization.
    ///
    /// The matching is case-insensitive and looks for known substrings.
    pub fn from_client_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        if name_lower.contains("cursor") {
            McpClientType::Cursor
        } else if name_lower.contains("claude") {
            McpClientType::ClaudeDesktop
        } else if name_lower.contains("vscode") || name_lower.contains("copilot") {
            McpClientType::VSCode
        } else if name_lower.contains("zed") {
            McpClientType::Zed
        } else {
            McpClientType::Unknown
        }
    }

    /// Detect client type from an MCP peer's stored client info.
    ///
    /// Returns `Unknown` if no client info is available.
    pub fn from_peer(peer: &Peer<RoleServer>) -> Self {
        peer.peer_info()
            .map(|info| Self::from_client_name(&info.client_info.name))
            .unwrap_or(McpClientType::Unknown)
    }

    /// Whether this client correctly handles MCP progress notifications.
    ///
    /// Returns `false` for Cursor (which logs errors for valid progress tokens),
    /// and `true` for all other clients (optimistic default).
    pub fn supports_progress(&self) -> bool {
        !matches!(self, McpClientType::Cursor)
    }

    /// Human-readable name for logging.
    pub fn display_name(&self) -> &'static str {
        match self {
            McpClientType::Cursor => "Cursor",
            McpClientType::VSCode => "VSCode/Copilot",
            McpClientType::ClaudeDesktop => "Claude Desktop",
            McpClientType::Zed => "Zed",
            McpClientType::Unknown => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_detection() {
        assert_eq!(
            McpClientType::from_client_name("cursor"),
            McpClientType::Cursor
        );
        assert_eq!(
            McpClientType::from_client_name("Cursor IDE"),
            McpClientType::Cursor
        );
        assert_eq!(
            McpClientType::from_client_name("CURSOR"),
            McpClientType::Cursor
        );
    }

    #[test]
    fn test_vscode_detection() {
        assert_eq!(
            McpClientType::from_client_name("vscode"),
            McpClientType::VSCode
        );
        assert_eq!(
            McpClientType::from_client_name("VSCode"),
            McpClientType::VSCode
        );
        assert_eq!(
            McpClientType::from_client_name("GitHub Copilot"),
            McpClientType::VSCode
        );
        assert_eq!(
            McpClientType::from_client_name("copilot-chat"),
            McpClientType::VSCode
        );
    }

    #[test]
    fn test_claude_detection() {
        assert_eq!(
            McpClientType::from_client_name("claude-desktop"),
            McpClientType::ClaudeDesktop
        );
        assert_eq!(
            McpClientType::from_client_name("Claude"),
            McpClientType::ClaudeDesktop
        );
    }

    #[test]
    fn test_zed_detection() {
        assert_eq!(McpClientType::from_client_name("zed"), McpClientType::Zed);
        assert_eq!(
            McpClientType::from_client_name("Zed Editor"),
            McpClientType::Zed
        );
    }

    #[test]
    fn test_unknown_detection() {
        assert_eq!(
            McpClientType::from_client_name("some-other-client"),
            McpClientType::Unknown
        );
        assert_eq!(McpClientType::from_client_name(""), McpClientType::Unknown);
    }

    #[test]
    fn test_supports_progress() {
        // Cursor does NOT support progress (logs errors)
        assert!(!McpClientType::Cursor.supports_progress());

        // All others support progress
        assert!(McpClientType::VSCode.supports_progress());
        assert!(McpClientType::ClaudeDesktop.supports_progress());
        assert!(McpClientType::Zed.supports_progress());
        assert!(McpClientType::Unknown.supports_progress());
    }

    #[test]
    fn test_default_is_unknown() {
        assert_eq!(McpClientType::default(), McpClientType::Unknown);
    }
}
