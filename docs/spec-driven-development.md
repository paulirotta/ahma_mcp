# Spec-Driven Development for `ahma_mcp`

This document outlines a proposal for adopting Spec-Driven Development (SDD) practices within the `ahma_mcp` project, inspired by the principles of the `spec-kit` repository.

## Summary of Spec-Driven Development (`spec-kit`)

Spec-Driven Development inverts the traditional software development model. Instead of code being the primary artifact, the **specification** becomes the executable source of truth from which code is generated and maintained. This approach aims to close the gap between project requirements and implementation, making the development process more aligned with the intended goals.

Key principles from `spec-kit` include:

* **Intent-Driven Development**: Focus on the "what" and "why" in natural language specifications.
* **Executable Specifications**: Creating specifications that are precise enough to generate code.
* **Constitutional Principles**: A set of rules (e.g., library-first, test-first) that enforce architectural consistency and quality.
* **Automated Workflow**: Using commands like `/specify`, `/plan`, and `/tasks` to streamline the process from idea to implementation plan to actionable tasks.

## Chosen Approach: Custom Workflow Inspired by `spec-kit`

We will adopt a lightweight, custom workflow inspired by the principles of Spec-Driven Development (SDD), tailored specifically for the `ahma_mcp` Rust project. This approach prioritizes flexibility and gradual adoption while still providing strong guardrails for quality and consistency.

This choice was made because it allows us to get the benefits of SDD—clearer requirements, better alignment between docs and code, and a more disciplined development process—without the overhead of a new, external tooling ecosystem that may not be a perfect fit for our existing Rust project.

## Human Workflow for Spec-Driven Development

This section provides a concrete, step-by-step guide for developers to follow when implementing new features or making significant changes to `ahma_mcp`.

### Step 1: Create a Feature Branch

All new work should start on a feature branch. The branch name should be descriptive of the feature, for example: `feature/new-tool-validation-rules`.

### Step 2: Write the Specification (`spec.md`)

Before writing any code, create a `spec.md` file in a new directory under `docs/features/`. For example: `docs/features/new-tool-validation-rules/spec.md`.

The `spec.md` should be based on a project-specific `SPEC_TEMPLATE.md` (to be created) and must define:

* **What** the feature is.
* **Why** it is needed (the user story or problem it solves).
* **Acceptance Criteria**: A clear, testable list of what must be true for the feature to be considered complete.

**Do not** include implementation details in the spec.

### Step 3: Write the Implementation Plan (`plan.md`)

Once the spec is clear, create a `plan.md` in the same directory. This document, based on a `PLAN_TEMPLATE.md` (to be created), translates the "what" of the spec into the "how" of the implementation.

The `plan.md` should outline:

* The technical approach.
* Which files in `src/` will be created or modified.
* The high-level logic and data structures.
* Any new dependencies.

### Step 4: Review and Implement

* Open a pull request with the `spec.md` and `plan.md`. This allows for an early review of the proposed changes before implementation begins.
* Once the plan is approved, implement the feature, following the plan.
* As you implement, update the `plan.md` if any details change.

### Step 5: Update Documentation and Merge

* Ensure that any relevant documentation in `docs/` is updated to reflect the changes.
* Once the implementation is complete and all checks pass, merge the pull request.

This lightweight process ensures that we think through our changes, document them clearly, and get feedback early, leading to higher-quality code and a more maintainable project.
