# 004 — Index module paths and explicit imports

- **Status:** Planned
- **Dependencies:** 003
- **Goal:** Record where explicitly imported names originate.

## Scope

Extend the scope index for `use` trees, aliases, inline modules, and qualified paths.
Establish a reliable source-file-to-module mapping for the supported crate layout;
do not assume that an arbitrary external file corresponds to a crate module.
Preserve original meanings of `crate`, `self`, and `super` before code is moved.

## Non-goals

Expanding glob imports, loading dependency source trees, or bypassing visibility.

## Acceptance tests

- Resolve nested explicit imports and `as` aliases without losing the original path.
- Test crate-relative, self-relative, and parent-relative paths in nested modules.
- Identify unsupported glob imports, ambiguous roots, and custom module paths.
- Merely seeing an import does not assert that its target is visible to the test scope.
