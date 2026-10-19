# 009 — Handle a conservative implementation subset

- **Status:** Planned
- **Dependencies:** 008
- **Goal:** Avoid silently dropping required behavior when local types are copied.

## Scope

Discover implementations associated with copied local types and apply task 001's
policy. Start with self-contained inherent implementations whose references are all
resolvable. Rewrite their type references and add their dependencies to the graph.
Explicitly decide handling of derives/attributes; reject unsupported trait-dependent
behavior rather than pretending syntax-only analysis has inferred it.

## Non-goals

General method/trait resolution, arbitrary derive expansion, and blanket impl copying.

## Acceptance tests

- A supported constructor and inherent method work on a copied local type.
- `Self` and local type references retain their intended targets.
- External/local dependencies inside a method are checked, not silently omitted.
- Unsupported traits, macros, attributes, and captured context fail predictably.
