# 008 — Promote self-contained local types

- **Status:** Planned
- **Dependencies:** 007
- **Goal:** Support selected function-local structs/enums/type aliases without manual context ranges.

## Scope

Add generation plans for supported self-contained local declarations. Generate
collision-resistant test-only names and rewrite resolved references in declarations,
snippet, signature, and output. Keep generated types in a scope where supported tests
can inspect them, without changing original declarations or promising original identity.

## Non-goals

Blindly rewriting every matching identifier, opaque macro tokens, external generic
captures, or compatibility with functions expecting the original local type.

## Acceptance tests

- A local struct can be returned and its fields asserted by the caller's tests.
- Two extractions with identically named local structs do not collide.
- Nested supported local types and aliases are rewritten by declaration identity.
- Unrelated shadowed names remain unchanged.
- Unsafe-to-promote dependencies produce a diagnostic instead of changed semantics.
