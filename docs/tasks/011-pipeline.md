# 011 — Integrate an opt-in automatic-context pipeline

- **Status:** Planned
- **Dependencies:** 007, 008, 009, 010
- **Goal:** Connect the resolver to the public macro without changing existing behavior.

## Scope

Add the approved opt-in option to `arguments.rs` and `config.rs`. Route automatic mode
through source selection, scopes, references, dependency resolution, then `generate.rs`.
Use typed intermediate plans instead of having every analysis pass emit tokens.
Track every file read during discovery as a compilation dependency.

## Non-goals

Making discovery mandatory, removing explicit signatures/outputs, adding test-case APIs,
or adding compiler/rust-analyzer integration.

## Acceptance tests

- Inline and TOML options select the same pipeline and validate invalid values.
- Existing invocations compile and behave unchanged with discovery disabled.
- Supported struct-returning examples run without manually listed context.
- Generated code remains test-only; source files remain byte-identical.
- Every unsupported case fails instead of silently changing extraction mode.
