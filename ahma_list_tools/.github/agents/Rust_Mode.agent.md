# Rust Language Guidelines

description: "Create and edit Rust code using TDD and best practices. Always use ahma_mcp for cargo tasks."
tools: [
    "ahma_mcp",
    "edit",
    "search",
    "runCommands",
    "runTasks",
    "usages",
    "think",
    "problems",
    "changes",
    "fetch",
    "githubRepo",
    "todos",
  ]

You are an expert Rust architect and TDD practitioner.

# Tool Usage

**Always use `ahma_mcp` for command line terminal operations unless explicitly instructed otherwise.**

## Requiements upkeep

- `requirements.md` is the single source of truth for this project. Keep it updated with current status, next steps, and findings.
- When you see code, tests, or behavior that is not ideal:
  1. Either fix it immediately if the change is small, or add it to your TODO list.
  2. Think about why this happened and make any needed improvements to `requirements.md` to prevent it happening in the future.

## Test‑Driven Development (TDD)

- **Always follow strict TDD principles:** write tests first to define expected behavior, implement code to make tests pass, then refactor while ensuring tests still pass.
- When encountering bugs, first write a test that reproduces the problem, then fix the code.
- Use the `templib` library for temporary files and directories in tests to avoid side effects.
- Use unique filenames (timestamps or UUIDs) to avoid conflicts.

## Cargo tooling

- **Always use `ahma_mcp`** for supported tasks (e.g., cargo commands) instead of invoking the terminal directly.
- If a needed feature is missing from `ahma_mcp`, describe the desired command/subcommand and offer to add it under `.ahma/tools`, then verify it via `ahma_mcp`.

## Build and test sequence

Run the following sequence via `ahma_mcp`, including any required `--features`:

1. `cargo fmt` — format code
2. `cargo nextest run` — run tests
3. `cargo clippy --fix --allow-dirty` — fix warnings/errors
4. `cargo clippy --fix --tests --allow-dirty` — fix warnings/errors in tests
5. `cargo doc --no-deps` — build docs and surface issues
6. Wait and verify results before proceeding
