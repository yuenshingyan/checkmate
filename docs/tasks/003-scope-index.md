# 003 — Index lexical scopes and bindings

- **Status:** Planned
- **Dependencies:** 002
- **Goal:** Resolve names according to their actual lexical scope, not text matching.

## Scope

Add `crates/checkmate/src/scope.rs` with stable scope/declaration identifiers and
separate Rust name namespaces. Index named types, functions, constants, parameters,
and pattern bindings. Model local binding visibility and item visibility separately.
Cover nested blocks, closures, loop patterns, match arms, and let/if-let bindings.

## Non-goals

Inferred types, imports, dependency copying, and whole-program name resolution.

## Acceptance tests

- Inner bindings shadow outer bindings without leaking outside their scope.
- A let initializer resolves against the correct pre-binding environment.
- Destructuring and closure parameters are recorded correctly.
- Field labels are not recorded as variables; local items can be found before declaration.
- Unsupported binding forms fail explicitly rather than corrupting the index.
