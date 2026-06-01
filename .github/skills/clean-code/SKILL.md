---
name: clean-code
description: "Apply clean code practices in this repository with small, safe refactors, explicit naming, reduced complexity, and behavior-preserving changes. Use for readability improvements, technical debt cleanup, and maintainability reviews in Rust code."
argument-hint: "Describe the target file/module, current pain point, and expected behavior constraints."
user-invocable: true
---

# Clean Code Workflow

Use this skill to improve code quality without changing intended behavior.

## When to Use
- Readability is low (unclear naming, long functions, dense branching).
- You need safer refactors before adding features.
- You want a structured review focused on maintainability.

## Inputs to Collect
- Target files or module boundaries.
- Current pain points (duplication, complexity, unclear ownership).
- Non-negotiable behavior constraints.
- Test and verification expectations.

## Core Principles
- Prefer small, reversible changes.
- Keep observable behavior stable unless explicitly requested.
- Improve names before changing architecture.
- Remove duplication only when semantics are confirmed equivalent.
- Keep side effects explicit and localized.
- Avoid broad refactors mixed with feature work.

## Decision Flow
1. Classify the issue:
- Naming and intent clarity
- Function/class/module size
- Control-flow complexity
- Duplication and coupling
- Error handling consistency

2. Pick the lowest-risk action:
- Rename symbols for intent
- Extract small helper function
- Split long function into focused steps
- Isolate side-effecting logic from pure transformations
- Consolidate duplicated branches with shared helper

3. Re-check constraints:
- Public API unchanged (unless requested)
- Error semantics unchanged
- Logging/output channels unchanged
- Performance impact acceptable

4. Validate incrementally:
- Compile and run tests after each small batch
- Stop and reassess when behavior risk increases

## Procedure
1. Identify 1 to 3 high-impact readability issues.
2. Propose a minimal refactor sequence.
3. Apply changes in small commits/patches.
4. Keep comments concise and only where intent is not obvious.
5. Verify with formatter, lints, and tests.
6. Summarize behavior guarantees and remaining debt.

## Completion Criteria
- Intent is clearer (names and structure reflect behavior).
- Complexity is reduced in touched code.
- No unintended behavior regressions.
- Verification is documented (build/lint/tests or justified gaps).

## Repo-Specific Guardrails (dabctl)
- Keep ETI-only assumptions intact; do not introduce RF/SDR paths.
- Preserve stdout/stderr/fd3 separation rules.
- Keep dablin decoding logic inside `src/dablin/`.
- Treat reference behavior parity as default correctness criteria.

## Example Prompts
- "Use clean-code on src/dablin/runner.rs to reduce branching complexity while preserving behavior."
- "Use clean-code to improve naming and extraction in src/dablin/audio/mod.rs without API changes."
- "Use clean-code to review duplication in metadata emission paths and propose minimal safe refactors."
