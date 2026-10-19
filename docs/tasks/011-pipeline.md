# 011 — Integrate an opt-in automatic-context pipeline

- **Status:** Complete
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

## Implementation and verification

- Typed `Context::{Explicit, Auto}` in inline/TOML options; only `"auto"` is accepted.
  Omission preserves ordinary extraction. Six option tests cover parity, invalid
  values/types, duplicate fields, unknown keys, and mixed forms.
- `automatic.rs` connects exact selection, source/caller identity, scopes, references,
  resolver, graph, and `PromotionPlan`. Only complete plans emit; every generated item
  and dependency marker is test-only. Compiler errors are anchored at the invocation
  after bounded structured rendering, with configured snippet/file identity.
- Public placement deliberately starts with **library-root → package integration-test
  root**, not the broader internal `SameTarget` API. `automatic/placement.rs` verifies
  compiler file/position/package/crate metadata, literal fully qualified tokens, Cargo
  target layout and name uniqueness, and conservative caller namespaces/attributes.
  No user assertion or filename-only guess supplies this proof. See the precise
  [public restrictions](../usage.md#automatic-context-opt-in).
- Source-file spans require proc-macro2's fallback parser, not its compiler text parser.
  A pinned version, macro-entry mutex covering all invocations, unwind-safe backend
  restoration, and plain-data/token-text boundary prevent mixed backend spans. A worker
  thread alone was experimentally insufficient because detection is global.
- Includes track every file whose contents are read: source, caller, package manifest,
  optional configuration, and optional snippet snapshot. No discovered out-of-line
  source modules or inherited workspace manifests are loaded.
- Ten focused pipeline/placement tests cover the all-or-error plan, complete dependency
  guards, type/impl/function/import test gating, new-nominal disclosure, explicit and
  auto-detected caller roots, ambiguity, namespaces, attributes/macros, and read-only
  failures. Review caught and fixed missing Cargo default `src/main.rs` inference for an
  explicit package-name sibling binary. A real build-script/include probe also exposed
  false test-placement acceptance when crate names matched; `build_script_` names are
  now reserved, with unit and real-Cargo rejection regressions.
- Four real-Cargo regression groups compile/run original reuse, local promotion and
  checked methods, compare `TypeId`, reject private fields/items and unsupported methods,
  mix ordinary/automatic invocations, reject wrappers/nested/same-target callers, and
  incrementally change context/configuration/snapshot/caller/manifest inputs. Input bytes
  are checked after successful and failed builds. Follow-up review exposed Rustc/fallback
  CRLF differences in multiline literal tokens; the caller's read-only parse now uses
  Rustc-equivalent normalization. A real Cargo regression combines a renamed raw-identifier
  macro path with a CRLF multiline raw-string signature.
- Separate public `tests/automatic.rs` examples in struct-return, local-types, and
  inherent-methods use the unchanged original source and snapshots. New `automatic.toml`
  files leave existing walkthrough definitions untouched. Original tests still run.
- Workspace verification: **256 passing tests**, one ignored doctest; formatting and
  strict all-target Clippy pass. These are regression tests, not formal equivalence
  proofs or new coverage measurements.

## Remaining boundary

Existing snapshots still protect **selected lines only**. Discovery changes trigger
analysis/recompilation but are not reviewed context baselines until task 012. Broader
integration regressions and integrated documentation remain tasks 013–014. Compiler
privacy/type/borrow checking remains authoritative. No source/snapshot writes, default
mode changes, branch merges, or full compiler inference are introduced.
