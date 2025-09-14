# `ahma_mcp` Constitution

This document outlines the core principles that govern the development of `ahma_mcp`. These principles are intended to ensure that the project remains maintainable, consistent, and high-quality. All new code and documentation should adhere to this constitution.

## Article I: Documentation-First Development

1. **No Code Before Docs**: All new features or significant changes must begin with a specification (`spec.md`) and an implementation plan (`plan.md`).
2. **Clarity and Conciseness**: Documentation should be clear, concise, and easy to understand. Avoid jargon where possible.
3. **Keep Docs Up-to-Date**: All documentation must be updated to reflect any changes in the code. Outdated documentation is worse than no documentation.

## Article II: Simplicity and Maintainability

1. **YAGNI (You Ain't Gonna Need It)**: Do not add functionality that is not required by a specification. Avoid premature optimization and over-engineering.
2. **Small, Focused Commits**: Commits should be small, atomic, and focused on a single logical change.
3. **Code Quality**: Code should be clean, well-formatted (`cargo fmt`), and free of warnings (`cargo clippy`).

## Article III: Test-Driven Development

1. **Test Everything That Can Break**: All new functionality must be accompanied by tests.
2. **Unit and Integration Tests**: A healthy mix of unit tests (for individual components) and integration tests (for interactions between components) is required.
3. **Tests as Documentation**: Tests should be written in a way that they also serve as documentation for the feature they are testing.

## Article IV: Asynchronous by Default

1. **Non-Blocking Operations**: The system should be designed to be non-blocking wherever possible. Long-running operations should be asynchronous.
2. **Clear Indication of Synchronicity**: If an operation must be synchronous, it should be clearly marked and documented as such.

## Amendment Process

Changes to this constitution can be proposed via a pull request and must be approved by the project maintainers.
