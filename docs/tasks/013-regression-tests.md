# 013 — Add end-to-end regression coverage

- **Status:** Planned
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
