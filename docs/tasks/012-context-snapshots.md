# 012 — Protect discovered context with snapshots

- **Status:** Completed
- **Dependencies:** 011
- **Goal:** Make relevant context changes reviewable, not just selected-body changes.

## Scope

Design a versioned context-baseline format containing stable source identity and the
resolved declarations/dependency mapping. Keep existing snippet snapshots backward
compatible. Track configuration, source, and baseline files for recompilation.
Separate location-only moves from content changes where the format permits it.

## Non-goals

Writing baselines during compilation, treating hashes as semantic proof, or committing
compiler build artifacts as the baseline.

## Acceptance tests

- Editing a discovered declaration triggers a useful mismatch even if the body is unchanged.
- Dependency additions/removals and alias target changes are visible.
- Unrelated source edits do not needlessly invalidate context baselines.
- Missing baselines require explicit creation/review; existing snapshots still work.
- Baseline file changes trigger recompilation in a real consumer crate.

## Implementation

- Optional inline/TOML `context_snapshot` paths and `context_report` accessor identifiers
  require `context = "auto"`; ordinary extraction and existing snippet snapshots remain
  unchanged. Both options can be used independently or together.
- `snapshot/context.rs` models the shared, validated dependency graph in strict version-1
  TOML: logical source identity, normalized signature/body/output, required inputs,
  reference mappings, original/copied/external declarations, known same-file inherent
  implementations, reuse routes, dependency references, and distinct graph relations.
- `snapshot/identity.rs` uses named ancestry and resolved impl-owner identities, never
  line positions, traversal IDs, generated helper names, or absolute checkout roots.
  Ambiguous required anchors fail; unrelated ambiguous locals do not. Canonical sources
  outside the package root are unsupported for context snapshots.
- Comparison normalizes Rust syntax/CRLF, ignores ordinary formatting/comments and
  location-only movement, and diagnoses declaration additions/removals/content changes,
  alias/mapping changes, extraction changes, and source/owner changes with `CM013`.
  Version/schema/embedded syntax failures never fall back or migrate automatically.
- The public adapter reads a requested baseline, tracks it with test-only `include_str!`,
  and emits only a complete approved plan. Report accessors return a string literal;
  no compilation/review operation creates, updates, prints, or approves a baseline file.
  User-authored tests can explicitly print that data for manual review/copying.
- Review caught a constant-pattern capture hazard in the initial report design. Reports
  now use accessor functions, not constants. Known caller/plan-name conflicts and standard
  prelude values are rejected; native tests cover unseen out-of-line caller binding
  patterns as well as extraction/implementation parameter conflicts. Follow-up review
  found no remaining concrete false acceptance; duplicate names produced by peer macros
  remain a Rust diagnostic boundary, covered by a native failure regression.
- A separate local-type configuration, integration test, ignored read-only printer, and
  manually reviewed context baseline demonstrate coexistence with the original snippet
  guard. All previous example source/config/snapshot/ordinary-test baselines are unchanged.

## Verification and limits

- Eleven context-model tests and three stable-identity tests cover deterministic rendering
  across independent checkouts, unrelated edits/moves, whole impl reordering, raw-literal
  CRLF behavior, content/dependency/alias changes, ambiguous anchors, schema failures,
  extraction/source identity, original terminal boundaries, and read-only behavior.
- Five additional option tests cover inline/TOML parity, auto-only options, identifiers,
  unknown/duplicate/mixed forms, and selected-definition validation.
- Three real-Cargo groups cover explicit print/review/copy, missing/malformed/versioned
  baselines, unselected declaration/impl/alias drift, snippet guard coexistence,
  config-relative paths, baseline-only recompilation, name safety/test gating, and byte
  preservation on successful and failing builds.
- The workspace has **279 passing tests**, one ignored inspection test (also run explicitly)
  and one ignored doctest. Formatting, strict all-target Clippy, and the basic example pass.
  These are regressions, not new measured coverage or formal guarantees.
- Original declarations remain graph terminals. Direct syntax/known inherent impls are
  reviewed, not transitive private dependencies, package/toolchain versions, environments,
  or all compiler semantics. A successful baseline comparison does not prove type/privacy
  correctness, ABI/TypeId stability, or semantic equivalence.

See [format and explicit review workflow](../context-snapshots.md). Task 013 remains the
broader integration-regression task; task 014 remains integrated feature documentation.
