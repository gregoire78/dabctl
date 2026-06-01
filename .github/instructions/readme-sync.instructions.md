---
description: "Use when changing CLI flags, command behavior, output contracts, metadata schema, or user-facing examples. Keep README.md aligned with the implemented behavior."
applyTo: "src/**,Cargo.toml,build.rs"
---

# README Sync Rules

- Treat README updates as required for user-facing changes.
- Update the relevant README sections when behavior changes:
  - CLI reference tables (flags, defaults, accepted values)
  - Metadata output examples and field semantics
  - Usage examples and command lines
  - Architecture notes if processing stages or supported FIG/features change
- Keep option names, defaults, and examples exactly consistent with clap definitions.
- If a change does not affect documentation, state this explicitly in the final change summary.
- Do not change README sections unrelated to the implementation.
