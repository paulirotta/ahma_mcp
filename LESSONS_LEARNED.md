# Lessons Learned: Development Workflow and Timeout Enhancements

This document captures the key lessons learned during implementation of graceful shutdown and timeout functionality. These insights should guide future development and refactoring decisions.

## 🎯 Original Requirements

1. **"Does the ahma_mcp server shut down gracefully when .vscode/mcp.json watch triggers a restart?"**
2. **"I think 'wait' should have an optional timeout, and a default timeout of 240sec"**

## 🔑 Critical Implementation Decisions

### 1. Graceful Shutdown (10-second window)

- **Signal Handling**: Must handle both SIGTERM (cargo watch) and SIGINT (Ctrl+C)
- **Timing**: 10 seconds provides optimal balance between operation completion and development speed
- **Progress Feedback**: Users need visual feedback during shutdown process with emojis and status
- **Force Exit**: Additional 5-second window prevents hanging processes

### 2. Wait Tool Timeout Bounds

- **Default**: 240 seconds (changed from 300s per user request)
- **Minimum**: 1 second (prevents accidentally short timeouts)
- **Maximum**: 1800 seconds (30 minutes - prevents resource waste)
- **Validation**: Always clamp values to prevent user errors

### 3. Progressive Warning System

- **Thresholds**: 50%, 75%, 90% provide optimal user feedback without spam
- **Spacing**: Warnings accelerate as timeout approaches (25%, 15% gaps)
- **Purpose**: Keep users informed without being annoying

### 4. Error Remediation Patterns

- **Lock Files**: `.cargo-lock`, `package-lock.json`, `yarn.lock`, `Cargo.lock`, etc.
- **Network Operations**: `download`, `fetch`, `pull`, `push`, `clone`, `update`
- **Build Operations**: `build`, `compile`, `install`, `test`, `check`
- **Generic Detection**: Pattern matching helps users resolve issues

## 🧪 Test Architecture Lessons

### 1. Tool Configuration Count Brittleness

**Problem**: Tests expecting exact JSON file counts break when development files are added.
**Solution**: Use minimum expectations with reasonable maximums for flexibility.

### 2. Hardwired vs JSON Tools

**Insight**: MCP tools (status, wait) don't need JSON configs - they're hardwired in the service.
**Exception**: Users may add JSON files for IDE/documentation support.

### 3. Test Guidance Comments

**Purpose**: Preserve architectural decisions and prevent regression during refactoring.
**Format**: Clear, concise comments explaining WHY decisions were made, not just WHAT.

## 📋 Development Workflow Integration

### 1. cargo watch Compatibility

- File changes trigger SIGTERM, not SIGKILL
- 10-second grace period allows most operations to complete
- Progress feedback keeps developers informed during restarts

### 2. User Experience Priorities

- **Efficiency**: Default 240s timeout balances patience vs speed
- **Feedback**: Progressive warnings and emoji status indicators
- **Recovery**: Actionable remediation suggestions when things go wrong
- **Flexibility**: Tool filtering allows targeted waits

## 🔮 Future Refactoring Guidance

### 1. Timeout Values - DO NOT CHANGE

- Default 240s was specifically requested by user
- Bounds (10s-1800s) prevent both user errors and resource waste
- Warning percentages (50%, 75%, 90%) are tuned for optimal UX

### 2. Signal Handling - CRITICAL

- SIGTERM handling is essential for cargo watch integration
- SIGINT handling is essential for user interrupts (Ctrl+C)
- 10-second shutdown window is carefully balanced

### 3. Test Expectations - BE FLEXIBLE

- Allow for development artifacts (ls_new.json, ls_fixed.json)
- Use minimum requirements rather than exact counts
- Focus on core functionality rather than configuration details

### 4. Error Messages - KEEP ACTIONABLE

- Always provide specific commands users can run
- Detect common patterns rather than specific tools
- Include context about why operations might be slow

## 🏗️ Architecture Patterns to Preserve

### 1. Async-First Design

- Status tool provides non-blocking monitoring
- Wait tool is explicitly blocking but warns users
- Operations run concurrently by default

### 2. Progressive Enhancement

- Basic functionality works without timeouts
- Advanced features (warnings, remediation) enhance experience
- Graceful degradation when features aren't available

### 3. User Guidance Philosophy

- Assume users want to solve problems, not just see errors
- Provide specific, actionable remediation steps
- Use friendly tone and emoji for better experience
- Balance information density with readability

## 🎪 Testing Strategy

### 1. Invariant Tests (`development_workflow_invariants_test.rs`)

- Encode critical values that must not change
- Test architectural decisions, not just functionality
- Provide clear failure messages explaining WHY something matters

### 2. Integration Tests with Guidance

- Comment WHY specific test patterns were chosen
- Explain common failure modes and their causes
- Make tests self-documenting for future developers

### 3. Flexible Assertions

- Use ranges instead of exact values where appropriate
- Allow for development variations while testing core requirements
- Focus on user-visible behavior over internal implementation

---

**Remember**: These lessons were hard-won through real user feedback and testing. Preserve them to avoid repeating the same discovery process during future development.
