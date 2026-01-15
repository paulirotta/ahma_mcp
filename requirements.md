# Requirements

## Status
- [x] Improve Rust documentation for `ahma_validate`
    - [x] Document CLI usage
    - [x] Document internal functions
    - [x] Add examples
- [x] Improve Rust documentation for `ahma_core`
    - [x] Analyze public API surface
    - [x] Add missing documentation
    - [x] Add examples for key functions
    - [x] Verify `cargo doc` output
    - [x] Improve `Adapter::escape_shell_argument` documentation
    - [x] Improve `test_utils` skip macros documentation
- [ ] Improve `Adapter` struct documentation in `ahma_core`

## Findings
- `ahma_core` documentation updated. Added examples to `Adapter` and `Client`. improved `AhmaMcpService` docs.
- Improved documentation for `skip_if_disabled` macros in `test_utils.rs` with detailed explanation and JSON configuration examples.


## Findings
- `ahma_core` contains core logic for the MCP client/bridge.
- Key modules appear to be `client`, `config`, `sandbox`, `shell_pool`.
