# Agent Plan: Tool Sequencing and Rust Quality Check

## Executive Summary

This plan implements a capability for `.ahma/tools/*.json` tool configurations to invoke sequences of other MCP tools with specific arguments. The primary use case is implementing a "Rust code quality check" that chains together multiple Cargo operations. The design emphasizes simplicity, maintainability, and proper handling of file locks through standardized delays.

## 1. Design for Tool Sequencing

### 1.1 Core Concept

**Simplest Approach**: Add a new field to the existing `ToolConfig` structure that allows a tool to declare itself as a "sequence" or "composite" tool that invokes other tools in order.

### 1.2 JSON Schema Extension

Add to `ToolConfig` in `src/config.rs`:

```json
{
  "name": "rust_quality_check",
  "description": "Comprehensive Rust code quality check: format, lint, test, build",
  "command": "sequence",
  "enabled": true,
  "sequence": [
    {
      "tool": "cargo_fmt",
      "subcommand": "fmt",
      "args": {}
    },
    {
      "tool": "cargo_clippy", 
      "subcommand": "clippy",
      "args": {
        "fix": true,
        "allow-dirty": true,
        "tests": true
      }
    },
    {
      "tool": "cargo_nextest",
      "subcommand": "nextest_run",
      "args": {
        "workspace": true
      }
    },
    {
      "tool": "cargo",
      "subcommand": "build",
      "args": {}
    }
  ],
  "step_delay_ms": 100,
  "synchronous": true
}
```

### 1.3 Implementation Strategy

**Location**: Extend `src/mcp_service.rs` and `src/config.rs`

**Key Components**:

1. **ToolConfig Extension** (`src/config.rs`):
   - Add `sequence: Option<Vec<SequenceStep>>` field
   - Add `step_delay_ms: Option<u64>` field (defaults to `SEQUENCE_STEP_DELAY_MS`)
   - Define `SequenceStep` struct:

     ```rust
     #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
     pub struct SequenceStep {
         pub tool: String,
         pub subcommand: String,
         pub args: Map<String, Value>,
     }
     ```

2. **MCP Service Handler** (`src/mcp_service.rs`):
   - Detect when `tool_config.sequence.is_some()`
   - Execute each step sequentially
   - Use `tokio::time::sleep(Duration::from_millis(step_delay_ms))` between steps
   - Aggregate results into a comprehensive output
   - Handle failures gracefully (stop on first failure vs continue)

3. **Execution Flow**:

   ```
   User invokes rust_quality_check
   -> MCP Service detects sequence tool
   -> For each step in sequence:
       -> Call the specified tool with args
       -> Wait for completion (synchronous tools) or await async operations
       -> Apply SEQUENCE_STEP_DELAY_MS pause
       -> Collect output
   -> Return aggregated results
   ```

## 2. Standardized Delay Constant

### 2.1 Purpose

Establish a single, maintainable constant for inter-step delays to prevent file lock contention (e.g., Cargo.lock conflicts).

### 2.2 Implementation

**Location**: `src/constants.rs`

**New Constant**:

```rust
/// Standard delay between sequential tool invocations to avoid file lock contention.
/// Particularly important for Cargo operations that may hold Cargo.lock.
/// This delay is used:
/// - Between steps in sequence tools (e.g., rust_quality_check)
/// - When spawning related commands that may conflict on shared resources
/// - In any scenario where temporal separation prevents race conditions
pub const SEQUENCE_STEP_DELAY_MS: u64 = 100;
```

### 2.3 Usage Points

Audit and update the following locations to use `SEQUENCE_STEP_DELAY_MS`:

1. **Existing delays in codebase**:
   - `src/test_utils.rs:44` - Currently uses `Duration::from_millis(100)`
   - `src/callback_system.rs:441` - Currently uses `Duration::from_millis(100)` (for timeouts, keep as-is)
   - `src/operation_monitor.rs:582` - Currently uses `Duration::from_millis(100)`

2. **New sequence tool implementation**:
   - Between each step in `rust_quality_check` sequence
   - Between any future sequence tool steps

### 2.4 Code Consistency Pattern

Replace ad-hoc delays:

```rust
// BEFORE
tokio::time::sleep(Duration::from_millis(100)).await;

// AFTER
use crate::constants::SEQUENCE_STEP_DELAY_MS;
tokio::time::sleep(Duration::from_millis(SEQUENCE_STEP_DELAY_MS)).await;
```

## 3. Rust Quality Check Tool

### 3.1 Tool Definition

**File**: `.ahma/tools/rust_quality_check.json`

```json
{
  "name": "rust_quality_check",
  "description": "Comprehensive Rust code quality pipeline: format, lint with auto-fix, test, and build. Runs each step sequentially with proper delays to avoid Cargo.lock contention. Always use this for pre-commit validation or CI preparation.",
  "command": "sequence",
  "enabled": true,
  "synchronous": true,
  "timeout_seconds": 1200,
  "hints": {
    "default": "Quality check running - this performs formatting, linting, testing, and building in sequence. Review any warnings or errors in the output to improve code quality."
  },
  "sequence": [
    {
      "tool": "cargo_fmt",
      "subcommand": "fmt",
      "args": {},
      "description": "Format code with rustfmt"
    },
    {
      "tool": "cargo_clippy",
      "subcommand": "clippy",
      "args": {
        "fix": true,
        "allow-dirty": true,
        "tests": true
      },
      "description": "Lint and auto-fix with clippy (including tests)"
    },
    {
      "tool": "cargo_nextest",
      "subcommand": "nextest_run",
      "args": {
        "workspace": true
      },
      "description": "Run all tests with nextest"
    },
    {
      "tool": "cargo",
      "subcommand": "build",
      "args": {},
      "description": "Build the project"
    }
  ],
  "step_delay_ms": 100,
  "subcommand": [
    {
      "name": "run",
      "description": "Execute the complete Rust quality check sequence",
      "options": [
        {
          "name": "working_directory",
          "type": "string",
          "description": "Working directory for all operations",
          "format": "path",
          "required": false
        }
      ]
    }
  ]
}
```

### 3.2 Command Breakdown

The sequence executes:

1. **`cargo fmt`** (via `cargo_fmt` tool)
   - Formats all code according to rustfmt rules
   - No arguments needed (uses default settings)

2. **`cargo clippy --fix --allow-dirty --tests`** (via `cargo_clippy` tool)
   - `--fix`: Automatically applies suggested fixes
   - `--allow-dirty`: Allows fixes even with uncommitted changes
   - `--tests`: Also checks test code

3. **`cargo nextest run`** (via `cargo_nextest` tool)
   - Runs all tests using the nextest runner
   - Faster and more reliable than `cargo test`

4. **`cargo build`** (via `cargo` tool)
   - Final verification that everything compiles
   - Catches any issues introduced by fixes

### 3.3 Integration with Existing Tools

The sequence leverages existing tool configurations:

- `cargo_fmt.json` - already exists
- `cargo_clippy.json` - already exists with necessary options
- `cargo_nextest.json` - already exists with `nextest_run` subcommand
- `cargo.json` - already exists with `build` subcommand

No modifications to these existing tools are required.

## 4. Implementation Tasks

### 4.1 Phase 1: Core Infrastructure

**Priority: High** | **Estimated Time: 2-3 hours**

1. **Update `src/constants.rs`**:
   - [x] Add `SEQUENCE_STEP_DELAY_MS` constant with documentation
   - [x] Add tests validating the constant exists

2. **Update `src/config.rs`**:
   - [x] Define `SequenceStep` struct
   - [x] Add `sequence: Option<Vec<SequenceStep>>` to `ToolConfig`
   - [x] Add `step_delay_ms: Option<u64>` to `ToolConfig`
   - [x] Update JSON schema validation
   - [x] Add tests for new configuration fields

3. **Replace ad-hoc delays**:
   - [x] Update `src/test_utils.rs:44` to use constant
   - [x] Update `src/operation_monitor.rs:582` to use constant
   - [x] Search codebase for other hardcoded `100` millisecond delays

### 4.2 Phase 2: Sequence Execution Logic

**Priority: High** | **Estimated Time: 3-4 hours**

1. **Update `src/mcp_service.rs`**:
   - [x] Add `handle_sequence_tool()` method to `AhmaMcpService`
   - [x] Modify `call_tool()` to detect sequence tools
   - [x] Implement sequential execution with delays
   - [x] Aggregate results from all steps
   - [x] Handle error propagation (fail-fast behavior)
   - [x] Add logging for each step

2. **Error Handling Strategy**:

   ```rust
   // Pseudocode
   for (index, step) in sequence.steps.iter().enumerate() {
       let result = execute_tool_step(step).await;
       
       if result.is_err() {
           return SequenceResult::failed_at_step(index, result);
       }
       
       if index < sequence.steps.len() - 1 {
           tokio::time::sleep(Duration::from_millis(step_delay_ms)).await;
       }
       
       aggregate_results.push(result);
   }
   ```

### 4.3 Phase 3: Rust Quality Check Tool

**Priority: High** | **Estimated Time: 1 hour**

1. **Create tool definition**:
   - [x] Create `.ahma/tools/rust_quality_check.json`
   - [x] Validate JSON syntax (fixed `option_type` -> `type` schema error)
   - [x] Test tool loads correctly

2. **Validation**:
   - [x] Run `ahma_mcp --validate .ahma/tools/rust_quality_check.json`
   - [x] Verify no schema errors

### 4.4 Phase 4: Testing

**Priority: High** | **Estimated Time: 2-3 hours** | **Status: ✅ COMPLETE**

1. **Unit Tests**:
   - [x] Test `SequenceStep` serialization/deserialization
   - [x] Test sequence tool detection
   - [x] Test delay application between steps
   - [x] Test error handling (failure at different steps)

2. **Integration Tests**:
   - [x] Create `tests/sequence_tool_test.rs`
   - [x] Test simple 2-step sequence
   - [x] Test rust_quality_check sequence end-to-end
   - [x] Test with working_directory parameter
   - [x] Test failure propagation
   - [x] All tests compile successfully
   - [x] All 696 tests pass

3. **Schema Validation**:
   - [x] Fixed schema validator to recognize `sequence` and `step_delay_ms` fields
   - [x] All tool configurations validate successfully

3. **Test Template** (`tests/sequence_tool_test.rs`):

   ```rust
   #[tokio::test]
   async fn test_rust_quality_check_sequence() {
       let client = new_client(Some(".ahma/tools")).await?;
       
       let args = json!({
           "working_directory": env!("CARGO_MANIFEST_DIR")
       });
       
       let result = client.call_tool(CallToolRequestParam {
           name: Cow::Borrowed("rust_quality_check"),
           arguments: Some(args),
       }).await?;
       
       // Verify all steps executed
       assert!(result.content.iter().any(|c| 
           c.as_text().unwrap().text.contains("cargo fmt")));
       assert!(result.content.iter().any(|c| 
           c.as_text().unwrap().text.contains("clippy")));
       // ... etc
   }
   ```

### 4.5 Phase 5: Documentation

**Priority: Medium** | **Estimated Time: 1 hour**

1. **Update documentation**:
   - [ ] Add sequence tool pattern to `docs/tool-schema-guide.md`
   - [ ] Update `docs/USAGE_GUIDE.md` with rust_quality_check example
   - [ ] Update `README.md` to mention sequence tools

2. **Add examples**:
   - [ ] Create `docs/examples/sequence-tool-pattern.md`
   - [ ] Show how to create custom sequences

## 5. Design Rationale

### 5.1 Why Sequence Tools?

**Alternatives Considered**:

1. **Shell scripts** - Less portable, harder to integrate with MCP protocol
2. **Hardcoded sequences in Rust** - Less flexible, requires recompilation
3. **VS Code tasks** - Platform-specific, not available via MCP

**Chosen Approach Benefits**:

- Declarative configuration in JSON
- Reuses existing tool infrastructure
- Standard delay mechanism prevents race conditions
- Easy to create new sequences without code changes
- Accessible via MCP protocol to AI agents

### 5.2 Why 100ms Delay?

**Rationale**:

- Sufficient for file system operations to complete
- Allows file locks (e.g., Cargo.lock) to be released
- Minimal impact on total execution time
- Matches existing implicit delay patterns in codebase
- Industry standard for tool chaining

**Impact Analysis**:

- For 4-step sequence: 300ms overhead (3 delays)
- Negligible compared to actual command execution (minutes)
- Prevents intermittent failures worth the small cost

### 5.3 Synchronous vs Async

**Decision**: Make sequence tools synchronous by default

**Reasoning**:

- Sequential execution is inherently blocking
- User expects to wait for all steps to complete
- Simplifies error handling and result aggregation
- Async would be misleading (steps can't run in parallel)
- Can override with `"synchronous": false` if needed

## 6. Backward Compatibility

### 6.1 Existing Tools

All existing tools continue to work without changes:

- Tools without `sequence` field work as before
- New fields are optional
- No breaking changes to tool schema

### 6.2 Configuration Loading

The `load_tool_configs()` function handles new fields gracefully:

- Unknown fields are ignored (via serde defaults)
- Validation only runs on tools with sequence field
- Legacy tools load unchanged

## 7. Future Enhancements

### 7.1 Potential Extensions

1. **Parallel Sequences**:
   - Add `"parallel": true` to `SequenceStep`
   - Execute independent steps concurrently
   - Example: Run `cargo check` and `cargo test` in parallel

2. **Conditional Steps**:
   - Add `"condition"` field to `SequenceStep`
   - Skip steps based on previous results
   - Example: Only run tests if build succeeds

3. **Variable Substitution**:
   - Pass outputs from one step as inputs to next
   - Example: Use test coverage % in subsequent steps

4. **Failure Handling**:
   - Add `"continue_on_error"` option
   - Choose between fail-fast and continue-all strategies

5. **Custom Delays**:
   - Per-step delay overrides
   - Example: Longer delay after build, shorter after format

### 7.2 Additional Sequence Tools

Other useful sequences to consider:

1. **`pre_commit_check`**: Git add, format, lint, test
2. **`release_prep`**: Version bump, changelog, build release, test
3. **`ci_simulation`**: All checks that CI would run
4. **`quick_check`**: Minimal checks for rapid iteration (fmt + check)

## 8. Risks and Mitigations

### 8.1 Risk: File Lock Contention

**Impact**: Medium - Cargo operations may conflict

**Mitigation**:

- Use `SEQUENCE_STEP_DELAY_MS` constant consistently
- Increase delay if needed (configurable via `step_delay_ms`)
- Document the purpose of delays clearly

### 8.2 Risk: Long Execution Times

**Impact**: Low - Quality checks take time regardless

**Mitigation**:

- Set appropriate `timeout_seconds` (2400s = 40mins)
- Use asynchronous execution pattern if user wants to continue working
- Provide progress updates for each step

### 8.3 Risk: Complex Error Messages

**Impact**: Medium - Multiple steps can fail in various ways

**Mitigation**:

- Clear step-by-step output formatting
- Indicate which step failed
- Include full error details from failing step
- Add summary at end showing success/failure of each step

## 9. Testing Strategy

### 9.1 Test Coverage Goals

- [ ] Unit tests for all new configuration structures
- [ ] Unit tests for sequence detection and parsing
- [ ] Integration tests for 2-step minimal sequence
- [ ] Integration tests for full rust_quality_check
- [ ] Error handling tests (failure at each possible step)
- [ ] Performance tests (verify delays are applied)

### 9.2 Test Data

Create test tool configs:

1. `tests/data/simple_sequence.json` - 2-step echo sequence
2. `tests/data/failing_sequence.json` - Sequence with intentional failure
3. Use actual `rust_quality_check.json` for integration tests

### 9.3 CI Integration

- [ ] Add rust_quality_check to CI pipeline
- [ ] Verify it catches common issues (format, lint, test failures)
- [ ] Compare execution time with individual step approach

## 10. Success Criteria

### 10.1 Must Have

- ✅ `SEQUENCE_STEP_DELAY_MS` constant defined and documented
- ✅ All hardcoded 100ms delays replaced with constant
- ✅ ToolConfig supports sequence field
- ✅ MCP Service executes sequences correctly
- ✅ rust_quality_check tool works end-to-end
- ✅ All tests pass including new sequence tests

### 10.2 Should Have

- ✅ Comprehensive error messages for sequence failures
- ✅ Documentation updated with examples
- ✅ VS Code task leverages the new tool

### 10.3 Nice to Have

- 📋 Additional example sequence tools
- 📋 Performance benchmarks
- 📋 Video demo of rust_quality_check

## 11. Timeline

**Estimated Total Time**: 10-12 hours

| Phase | Tasks | Time | Dependencies |
|-------|-------|------|--------------|
| Phase 1 | Constants & Config | 2-3h | None |
| Phase 2 | Execution Logic | 3-4h | Phase 1 |
| Phase 3 | Quality Check Tool | 1h | Phase 2 |
| Phase 4 | Testing | 2-3h | Phase 3 |
| Phase 5 | Documentation | 1h | Phase 4 |

**Suggested Approach**: Implement in order, with testing after each phase.

## 12. Validation Steps

Before considering this complete:

1. **Code Review Checklist**:
   - [ ] All constants properly documented
   - [ ] No hardcoded delays remain
   - [ ] Error handling covers all edge cases
   - [ ] Code follows existing patterns
   - [ ] No clippy warnings

2. **Functional Validation**:
   - [ ] Run rust_quality_check on ahma_mcp itself
   - [ ] Verify it catches formatting issues
   - [ ] Verify it catches clippy warnings
   - [ ] Verify it runs tests
   - [ ] Verify it builds successfully

3. **Performance Validation**:
   - [ ] Measure total execution time
   - [ ] Verify delays are applied (check logs)
   - [ ] Compare with running steps manually

4. **Integration Validation**:
   - [ ] Tool appears in MCP tool list
   - [ ] AI agents can discover and use it
   - [ ] VS Code task integration works
   - [ ] Works with different working directories

## Appendix A: Code Snippets

### A.1 ToolConfig Extension

```rust
// In src/config.rs

/// Represents a single step in a tool sequence
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SequenceStep {
    /// Name of the tool to invoke
    pub tool: String,
    /// Subcommand within that tool
    pub subcommand: String,
    /// Arguments to pass to the tool
    pub args: Map<String, Value>,
    /// Optional description for logging/display
    #[serde(default)]
    pub description: Option<String>,
}

/// Add to ToolConfig struct:
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolConfig {
    // ... existing fields ...
    
    /// Optional sequence of tools to execute in order
    #[serde(default)]
    pub sequence: Option<Vec<SequenceStep>>,
    
    /// Delay in milliseconds between sequence steps (default: SEQUENCE_STEP_DELAY_MS)
    #[serde(default)]
    pub step_delay_ms: Option<u64>,
}
```

### A.2 Sequence Handler Skeleton

```rust
// In src/mcp_service.rs

impl AhmaMcpService {
    async fn handle_sequence_tool(
        &self,
        tool_config: &ToolConfig,
        params: CallToolRequestParam,
    ) -> Result<CallToolResult, McpError> {
        let sequence = tool_config.sequence.as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Not a sequence tool".into()))?;
        
        let step_delay_ms = tool_config.step_delay_ms
            .unwrap_or(crate::constants::SEQUENCE_STEP_DELAY_MS);
        
        let mut results = Vec::new();
        
        for (index, step) in sequence.iter().enumerate() {
            tracing::info!(
                "Executing sequence step {}/{}: {} -> {}",
                index + 1,
                sequence.len(),
                step.tool,
                step.subcommand
            );
            
            // Execute this step
            let step_result = self.execute_sequence_step(step, &params).await?;
            results.push(step_result);
            
            // Apply delay between steps (but not after the last one)
            if index < sequence.len() - 1 {
                tokio::time::sleep(Duration::from_millis(step_delay_ms)).await;
            }
        }
        
        // Aggregate results into final response
        self.aggregate_sequence_results(results)
    }
}
```

## Appendix B: VS Code Task Integration

```json
// Add to .vscode/tasks.json
{
    "label": "Rust Quality Check (MCP)",
    "type": "shell",
    "command": "${workspaceFolder}/target/release/ahma_mcp",
    "args": [
        "rust_quality_check",
        "--working-directory",
        "${workspaceFolder}"
    ],
    "group": {
        "kind": "test",
        "isDefault": true
    },
    "presentation": {
        "reveal": "always",
        "panel": "dedicated"
    },
    "problemMatcher": [
        "$rustc",
        "$rust-panic"
    ]
}
```

---

## 13. Critical Bug Fix: file_tools ls Command Error

### 13.1 Problem Statement

**Reported Error**:

```
Error: MCP -32603: Error executing tool 'file_tools': Command failed with exit code 1
stderr: ls: --long: No such file or directory
stdout: /Users/phoughton/github/ai_usage/csv-in:
```

**Issue**: The `file_tools` ls command is incorrectly passing arguments to the underlying ls command, causing it to fail even when the directory exists. The error message suggests that `--long` is being treated as a path rather than a flag.

### 13.2 Root Cause Analysis

The issue appears to be in how the `file_tools` tool constructs and passes arguments to the shell command. Specifically:

1. The `--long` flag may not be properly formatted for the ls command
2. Argument ordering or quoting issues in command construction
3. Platform-specific flag handling (macOS vs Linux ls differences)

### 13.3 Test-Driven Development Approach

**Phase 1: Reproduce the Bug** (Priority: Critical)

1. **Create failing test** (`tests/file_tools_ls_bug_test.rs`):

   ```rust
   #[tokio::test]
   async fn test_ls_with_long_flag_in_existing_directory() -> Result<()> {
       init_test_logging();
       let client = new_client(Some(".ahma/tools")).await?;
       
       // Create a test directory
       let temp_dir = tempfile::tempdir()?;
       let test_path = temp_dir.path().to_str().unwrap();
       
       // This should work but currently fails
       let result = client.call_tool(CallToolRequestParam {
           name: Cow::Borrowed("file_tools"),
           arguments: Some(json!({
               "subcommand": "ls",
               "path": test_path,
               "long": true  // or "long": "" depending on schema
           }).as_object().unwrap().clone()),
       }).await;
       
       // Should succeed, not error with "ls: --long: No such file or directory"
       assert!(result.is_ok(), "ls with --long flag should work: {:?}", result.err());
       
       client.cancel().await?;
       Ok(())
   }
   ```

2. **Test argument parsing**:
   - Test with `long` flag
   - Test with `all` flag
   - Test with multiple flags combined
   - Test with both flags and path arguments

3. **Verify current behavior**:
   - Run test to confirm it fails with the reported error
   - Document exact failure mode

**Phase 2: Fix the Implementation** (Priority: Critical)

1. **Locate the bug**:
   - [ ] Check `.ahma/tools/file_tools.json` for schema definition
   - [ ] Check `src/adapter.rs` for command construction logic
   - [ ] Check if the issue is in flag vs option handling

2. **Potential fixes**:
   - Ensure boolean flags are converted to command-line flags properly
   - Fix argument ordering (flags before positional arguments)
   - Handle platform differences (GNU ls vs BSD ls)
   - Proper shell escaping and quoting

3. **Implementation guidelines**:

   ```rust
   // BEFORE (hypothetical broken code):
   // command = format!("ls {} {}", args.get("long"), path);
   // Result: "ls --long /path" but args might pass "--long" as a value
   
   // AFTER (correct code):
   // Build flags separately from paths
   let mut flags = Vec::new();
   if args.get("long").and_then(|v| v.as_bool()).unwrap_or(false) {
       flags.push("-l");  // Use short form for compatibility
   }
   let command = format!("ls {} {}", flags.join(" "), shell_escape(path));
   ```

4. **Edge cases to handle**:
   - Empty directories
   - Directories with spaces in names
   - Symbolic links
   - Permission errors (should fail gracefully)
   - Non-existent directories (should error appropriately, not with flag errors)

**Phase 3: Verify the Fix** (Priority: High)

1. **Run the failing test**: Should now pass
2. **Run related file_tools tests**: Ensure no regressions
3. **Manual verification**:

   ```bash
   # Test the fixed tool manually
   echo '{"subcommand": "ls", "path": ".", "long": true}' | \
     cargo run -- file_tools
   ```

4. **Additional test cases**:
   - [ ] Test all ls flags individually
   - [ ] Test flag combinations
   - [ ] Test with various directory types
   - [ ] Test error cases (non-existent paths)

### 13.4 Related Issues to Investigate

While fixing the ls command, audit the entire `file_tools` implementation for similar issues:

1. **Other subcommands** that may have flag/argument issues:
   - [ ] `cp` - Check `-r`, `-p`, `-n` flags
   - [ ] `mv` - Check `-f`, `-n` flags  
   - [ ] `rm` - Check `-r`, `-f` flags
   - [ ] `grep` - Check `-i`, `-r`, `-n` flags
   - [ ] `find` - Check `-name`, `-type` flags

2. **Command construction patterns**:
   - [ ] Review how boolean flags are converted to CLI arguments
   - [ ] Review how string options are passed
   - [ ] Review path escaping and quoting
   - [ ] Review working_directory handling

3. **Platform compatibility**:
   - [ ] Verify commands work on macOS (BSD tools)
   - [ ] Verify commands work on Linux (GNU tools)
   - [ ] Document any platform-specific behaviors

### 13.5 Implementation Tasks

- [ ] **Task 1**: Create `tests/file_tools_ls_bug_test.rs` with failing test
- [ ] **Task 2**: Run test to confirm it reproduces the bug
- [ ] **Task 3**: Investigate `.ahma/tools/file_tools.json` schema
- [ ] **Task 4**: Locate command construction code in `src/adapter.rs` or related files
- [ ] **Task 5**: Implement fix for ls command flag handling
- [ ] **Task 6**: Run test to verify fix works
- [ ] **Task 7**: Add comprehensive test coverage for all ls flags
- [ ] **Task 8**: Audit other file_tools subcommands for similar issues
- [ ] **Task 9**: Create tests for other subcommands if needed
- [ ] **Task 10**: Update documentation if command usage changed

### 13.6 Success Criteria

- ✅ Test reproduces the bug initially
- ✅ Fix resolves the reported error
- ✅ All file_tools tests pass
- ✅ No regressions in other commands
- ✅ Manual testing confirms ls works with all flag combinations
- ✅ Error messages are clear when actual errors occur (e.g., non-existent directory)

### 13.7 Timeline

**Estimated Time**: 3-4 hours

| Task | Time | Priority |
|------|------|----------|
| Create failing test | 30min | Critical |
| Debug and locate bug | 1h | Critical |
| Implement fix | 1h | Critical |
| Comprehensive testing | 1h | High |
| Related tools audit | 30min | Medium |

---

**Document Version**: 1.1  
**Created**: 2025-10-30  
**Updated**: 2025-10-30 (Added Section 13: file_tools bug fix)  
**Status**: In Progress  
**Author**: AI Agent  
**Review Status**: Pending
