# 013 — Add end-to-end regression coverage

- **Status:** Completed
- **Dependencies:** 011, 012
- **Goal:** Validate the public workflow, not just individual syntax transforms.

## Scope

Extend `crates/checkmate/tests/` with compile-pass/fail consumers and rebuild checks.
Cover original type identity, copied local types, imports, shadowing, dependencies,
unsupported contexts, privacy, and snapshot drift. Keep fixtures outside normal modules
when testing external-file dependency tracking.

## Non-goals

Replacing focused unit tests or asserting that compilation alone guarantees semantics.

## Acceptance tests

- Consumers call extracted functions returning structs, enums, aliases, and tuples.
- Type identity and visibility failures are tested explicitly.
- Source/config/context-snapshot edits cause rebuilds without cleaning the target directory.
- Ordinary non-test builds exclude generated functions.
- `cargo test --workspace` and strict Clippy pass; original-source hashes are unchanged.

## Implementation

- `tests/public_values.rs`: three native groups exercise original enum variants, public
  reexports/import aliases, nested import/type shadowing, copied enum/struct/tuple returns,
  transparent aliases, shared diamond dependencies, recursive `Option<Box<Node>>` types,
  caller decoys, and explicit inputs without executing prior initializers. Runtime values,
  concrete assignments, shared-type borrows, TypeIds, and original execution are checked.
- `tests/public_failures.rs`: three groups check incompatible nominal assignments,
  original private routes/methods, missing inputs, generic locals, derives/cfg/macros,
  transitive private context with dependency ancestry, borrowed dot-call rejection,
  unsupported unused methods, and signature shadowing. Originals first pass `check --lib`.
  A duplicate-definition oracle checks no partial extracted/report API after a planner
  failure, with a positive control alongside `compile_error!`.
- `tests/public_rebuilds.rs`: two groups exercise custom library/integration target paths,
  config-relative inputs outside Rust modules, explicit report/review/approval, baseline-
  only and config-only changes, ordinary external-file edits, source moves/unrelated
  edits, and unselected implementation drift in reused target directories. Runtime-read
  expectations make stale external-fixture code observable. No `cargo clean` is used.
- `tests/support/mod.rs`: shared offline native consumer harness compares all existing
  non-target input bytes before/after every Cargo invocation, detects deletions, and
  rejects new inputs except Cargo's initial lockfile. Compilation never approves or
  creates review files; fixture updates are explicit outer-harness operations.
- Normal production library builds are distinguished from active ordinary macro expansion
  in a normal binary build. Review identified that automatic no-test gating needed an
  independent function-name check: `tests/context_snapshots.rs` now checks absence of both
  the report accessor and extracted function under its Unix compiler wrapper. Planner
  tests still inspect cfg gating for all emitted items.
- No production implementation, dependency, original example source/config/baseline, or
  extraction fixture changes were needed.

## Verification and boundaries

- Eight additional native groups pass; the workspace now has **287 passing tests** and
  two ignored tests (the existing inspection test and doctest).
- `cargo fmt -p checkmate --check`, strict all-target workspace Clippy, and the basic
  example pass. SHA-256 hashes of 26 existing example/fixture inputs match task 012.
- Review found no blocking issue in the new eight groups; its automatic-function gating
  gap was addressed with the independent native check above.
- These are behavioral/diagnostic regressions, not formal guarantees or new measured
  line/branch coverage. TypeIds are compared within a build only. Original graph terminals
  remain terminal; recursive allocation occurs in caller code rather than claiming
  unsupported associated-call discovery.

See the [public regression matrix and oracle limitations](../regression-tests.md).
Task 014 remains the integrated documentation/examples task.
