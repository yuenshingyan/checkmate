# 006 — Resolve reusable original declarations

- **Status:** Planned
- **Dependencies:** 004, 005
- **Goal:** Reuse existing types/items whenever their original identity can be preserved.

## Scope

Resolve indexed names to known original declaration paths or local declaration IDs.
Generate qualified references/imports for reusable items. Let rustc enforce visibility
and typing; do not widen access or copy inaccessible module-level items as a workaround.
Restrict supported resolution to task 001's explicit subset.

## Non-goals

Promoting local types, guessing glob/trait resolution, or making private items public.

## Acceptance tests

- An extracted function returns the same accessible original struct type.
- Explicit aliases and nested module paths preserve identity.
- Private/inaccessible targets fail rather than silently becoming copies.
- Unknown names and ambiguous mappings report useful source locations.
