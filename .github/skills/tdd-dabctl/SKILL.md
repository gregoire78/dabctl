---
name: tdd-dabctl
description: "Drive Rust changes in dabctl with Test-Driven Development (TDD): write failing tests first, implement minimal behavior, then refactor safely while preserving dablin ETI-only parity and stdout/stderr/fd3 contracts."
argument-hint: "Describe the behavior to add/fix, target module, and expected observable output."
user-invocable: true
---

# TDD dabctl Workflow

Use this skill for behavior-safe Rust implementation via Red-Green-Refactor.

## When to Use
- You add or fix behavior in Rust code.
- You need strong regression protection before refactoring.
- The task affects CLI parsing, decode logic, or metadata emission.

## Inputs to Collect
- Target behavior in observable terms (CLI output, PCM output, metadata events, error handling).
- Scope of change (module and public API impact).
- Constraints that must not change.
- Existing test coverage and missing cases.

## Repo-Specific Guardrails
- Keep ETI-only scope; no RF/SDR/live decode path.
- Preserve reference behavior parity by default.
- Keep output channel separation strict:
  - stdout: PCM only
  - stderr: logs only (or silent with --silent)
  - fd3: JSONL metadata only
- Keep decoding logic under src/dablin/.

## Decision Flow
1. Classify test level first:
- Unit test: pure logic and parser/transform behavior.
- Integration test: CLI and end-to-end observable behavior.
- Golden/fixture test: parity-sensitive decoding/metadata snapshots.

2. Select initial failing test strategy:
- Smallest case that proves missing behavior.
- One failure reason per test.
- Deterministic inputs only.

3. Implement minimum code to pass:
- Avoid opportunistic refactors during Green.
- Preserve public behavior unless explicitly requested.

4. Refactor safely:
- Rename/extract/simplify only with tests green.
- Keep semantics and output contracts unchanged.

5. Re-evaluate risk:
- If test is flaky or too broad, split it.
- If behavior is ambiguous, encode assumption in test name and notes.

## Procedure (Red-Green-Refactor)
1. Red:
- Write one failing test for the requested behavior.
- Run targeted test to confirm it fails for the expected reason.

2. Green:
- Implement the smallest change required to pass.
- Run targeted test, then nearby suite.

3. Refactor:
- Improve readability/structure while keeping tests green.
- Avoid changing output formats and channels.

4. Verify repository quality gates:
- rtk cargo fmt
- rtk cargo build --release
- rtk cargo clippy -- -D warnings
- rtk cargo test
- rtk cargo build --release --features fdk-aac (if touched or relevant)

5. Report results:
- Tests added/changed.
- Behavior guaranteed by tests.
- Any uncovered risk and next test to add.

## Completion Criteria
- At least one new failing-then-passing test demonstrates the requested behavior.
- Code change is minimal and traceable to the test.
- All relevant checks pass (or failure is explained with clear next action).
- No unintended contract regression on ETI-only behavior or output channels.

## Example Prompts
- "Use tdd-dabctl to add a failing test for SID parsing edge cases, then implement the minimal fix."
- "Use tdd-dabctl to reproduce and fix a metadata JSONL formatting bug without touching stdout PCM flow."
- "Use tdd-dabctl to protect AAC gap behavior with tests before refactoring decoder branching."
