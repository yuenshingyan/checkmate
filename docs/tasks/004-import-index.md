# 004 — Index module paths and explicit imports

- **Status:** Complete
- **Dependencies:** 003
- **Goal:** Record where explicitly imported names originate.

## Result

Implemented explicit import records and path provenance in `scope/imports.rs`, plus
verified library/binary-root discovery in `source/origin.rs`. Nineteen focused tests
cover aliases, namespaces, module paths, manifest layouts, identity mismatches, and
unsupported cases. This remains test-gated until task 011.

Imports are unvalidated namespace candidates, not proof of target visibility or
availability. Imported names in binding patterns and overlapping unproven namespaces
fail explicitly pending target-kind resolution. Source mapping supports conventional
or Cargo-declared library/binary roots with explicit 2018/2021/2024 editions; arbitrary
module files, custom `#[path]` modules, inherited editions, and glob imports are not guessed.

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
