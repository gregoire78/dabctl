---
name: rust-dabctl-workflow
description: "Implement and review Rust changes in dabctl with strict dablin ETI-only behavior parity, stdout/stderr/fd3 contracts, and deterministic verification (fmt/build/clippy/test). Use when editing src/dablin, CLI wiring, AAC gap handling, or metadata output paths."
argument-hint: "Describe the Rust task, target module, and expected behavior (parity/fix/refactor)."
user-invocable: true
---

# Rust dabctl Workflow

Use this skill to make safe, behavior-preserving Rust changes in dabctl.

## When to Use
- You need to implement or review Rust code in this repository.
- The task touches ETI decoding flow, CLI behavior, AAC gap handling, or metadata emission.
- You want a deterministic checklist before considering work complete.

## Inputs to Collect
- Requested change and expected observable behavior.
- Target path (for example: CLI wiring, `src/dablin/**`, audio decoder path, or metadata output path).
- Whether behavior must be strict parity or allows the explicit `--aac-gap silence` mode.

## Non-Negotiable Constraints
- Keep the implementation ETI-only. Do not add RF/SDR/live capture decoding paths.
- Preserve behavior from the gregoire78/dablin integration unless explicitly requested otherwise.
- Keep output channels strict:
  - stdout: raw PCM only
  - stderr: logs only (suppressed by `--silent`)
  - fd 3: JSONL metadata only
- Keep ETI decode logic under `src/dablin/`; core wiring outside this area should remain minimal.

## Decision Flow
1. Classify the change:
- CLI contract change
- Decode/data-path change
- Metadata/output channel change
- Performance/safety refactor with no behavioral change

2. Choose editing scope:
- CLI contract: focus on `src/cli.rs` and dispatch wiring in `src/main.rs` or `src/dablin/runner.rs`.
- Decode/data-path: prefer `src/dablin/**` only, preserving module boundaries.
- Metadata/output: verify separation across stdout/stderr/fd3 and JSONL shape.

3. Handle behavior mode:
- Parity mode (default): preserve legacy behavior exactly.
- Silence mode (`--aac-gap silence`): emit frame-exact zero PCM for missing/invalid AAC frames from decoder path only.

4. Validate risk before merge:
- Any change that alters user-observable output must be justified and documented.
- If behavior diverges from reference unexpectedly, treat as bug and revert approach.

## Procedure
1. Inspect current implementation in target files and related call sites.
2. Describe preserved behavior and assumptions before editing.
3. Apply the smallest patch set needed; avoid unrelated refactors.
4. Add brief comments only when logic is non-obvious.
5. Run verification commands in order:
- `rtk cargo fmt`
- `rtk cargo build --release`
- `rtk cargo build --release --features fdk-aac` (if applicable)
- `rtk cargo clippy -- -D warnings`
- `rtk cargo test`
6. For behavior-sensitive changes, run quick ETI smoke validation and confirm channel separation.
7. Summarize what changed, why it is safe, and what was verified.

## Completion Criteria
- Requested behavior is implemented with minimal, targeted edits.
- ETI-only and dablin parity constraints are respected.
- stdout/stderr/fd3 contracts remain intact.
- Verification commands pass (or failures are explained with next action).
- User-facing summary includes changed files, behavior impact, and validation evidence.

## Example Prompts
- "Use rust-dabctl-workflow to add a new validation in CLI parsing without changing runtime decode behavior."
- "Use rust-dabctl-workflow to debug AAC gap silence mode in `src/dablin/audio/` and preserve freeze default parity."
- "Use rust-dabctl-workflow to review whether metadata ever leaks to stdout or stderr."
