//! Consolidated MCP service tests
//!
//! This module consolidates all MCP-related tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "mcp_callback_test.rs"]
mod mcp_callback_test;

#[path = "mcp_cancellation_bug_test.rs"]
mod mcp_cancellation_bug_test;

#[path = "mcp_server_integration_test.rs"]
mod mcp_server_integration_test;

#[path = "mcp_server_reproduction_test.rs"]
mod mcp_server_reproduction_test;

#[path = "mcp_service_async_edge_cases_test.rs"]
mod mcp_service_async_edge_cases_test;

#[path = "mcp_service_basic_coverage_test.rs"]
mod mcp_service_basic_coverage_test;

#[path = "mcp_service_comprehensive_test.rs"]
mod mcp_service_comprehensive_test;

#[path = "mcp_service_coverage_expansion_test.rs"]
mod mcp_service_coverage_expansion_test;

#[path = "mcp_service_coverage_improvement_test.rs"]
mod mcp_service_coverage_improvement_test;

#[path = "mcp_service_coverage_test.rs"]
mod mcp_service_coverage_test;

#[path = "mcp_service_test.rs"]
mod mcp_service_test;

#[path = "vscode_mcp_config_test.rs"]
mod vscode_mcp_config_test;
