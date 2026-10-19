# 006 — Resolve reusable original declarations

- **Status:** Complete (internal/test-gated; public integration remains task 011)
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

## Implementation and evidence

`resolve/catalog.rs` borrows indexed original item syntax. `resolve/declarations.rs`
resolves namespace-aware paths to original declaration IDs, function-local type IDs,
or explicit external paths. It emits qualified imports, not copied declarations.
Type/const arguments remain in the input AST rather than being inserted into imports.
Struct construction and enum variants are distinguished from type references; deferred
methods and qualified trait operations cannot disappear from a successful plan.

The internal caller must establish either same-target placement or a same-package
integration-test target. The latter uses the verified library name and rejects binary
roots. Task 011 must verify actual invocation placement; this pass does not infer it.
Public canonical paths and simple public item re-exports are supported. Same-target
privacy and all field/type access remain rustc checks, never visibility rewrites.

18 focused tests include real rustc consumers proving original struct identity,
generic imports, public re-exports, and compile failures for item/field privacy.
Other cases cover aliases, nested modules, raw identifiers, custom library names,
namespace ambiguity, unknown paths, local types, stale sources, and source locations.
A sixth executable walkthrough resolves the struct example's `Account` references.

## Conservative boundaries

- Only a documented small standard-prelude set is mapped (see architecture).
  Cargo aliases shadowing `std`/`core` invalidate that proof.
- Nonstandard external type paths are symbolic references in the same target;
  rustc checks availability and typing. Cross-target dependency identity is not
  proven by matching manifest alias strings, so such reuse is rejected.
- External value paths, unresolved associated members, output-introduced imports,
  and ambiguous type/const arguments fail rather than guessing trait/context state.
- Public module-alias routes through otherwise private ancestors and variant-only
  re-exports without a nameable enum base are conservatively unsupported.
- Function-local types retain their source IDs; promotion belongs to task 008.
  Function-local helpers/constants outside the selection are not silently copied.
