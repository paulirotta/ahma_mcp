# Developer Guide

This guide provides essential information for developers working on the `ahma_mcp` project. It covers the development workflow, troubleshooting common issues, and other best practices.

## Spec-Driven Development Workflow

We follow a lightweight, spec-driven development process. Please refer to `docs/spec-driven-development.md` for the full workflow. The key steps are:

1. **Create a feature branch.**
2. **Write a `spec.md`** defining the what and why of the feature.
3. **Write a `plan.md`** outlining the technical implementation.
4. **Get the spec and plan reviewed** in a pull request before implementation.
5. **Implement, test, and update documentation.**
6. **Merge.**

## Troubleshooting

(This section is a consolidation of previous troubleshooting documents.)

### Operation Timeout Issues

* **Default Timeout**: `await` operations have a default timeout of 240 seconds.
* **Stale Lock Files**: If operations hang, check for stale lock files (e.g., `Cargo.lock`, `.cargo-lock`) and remove them if no process is holding them.
* **Network Issues**: For network-related timeouts, check your connection and consider using offline modes for package managers (e.g., `cargo build --offline`).
* **Resource Exhaustion**: If the system is slow, monitor resource usage (`top`, `htop`) and consider reducing concurrency (e.g., `cargo build -j 1`).

### VS Code Integration

* **MCP Server Not Starting**:
  * Ensure the binary path in `.vscode/mcp.json` is correct and uses absolute paths.
  * Verify the `tools` directory exists.
  * Restart VS Code after any configuration changes.
* **Performance Issues**:
  * Always use the release binary (`cargo build --release`) for the MCP server.
  * Monitor resource usage to ensure the server is not consuming excessive resources.

## Graceful Shutdown

The server has a graceful shutdown mechanism, especially useful when using `cargo watch`.

* When a file change is detected, the server allows a 10-second grace period for ongoing operations to complete before restarting.
* This prevents abrupt termination of builds or tests during development.

For more detailed troubleshooting, refer to the original `TROUBLESHOOTING.md` which has been archived.
