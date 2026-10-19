# 008 — Promote self-contained local types

- **Status:** Complete (internal/test-gated; public integration remains task 011)
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

## Implementation and evidence

`resolve/promote.rs` builds an all-or-error plan from the completed dependency graph.
Generated names encode the extraction's function identifier and declaration/node ID;
known invocation names and identifiers in the analysis context are checked for
collisions. All emitted imports, types, and the function are `#[cfg(test)]` gated.

`promote/rewrite.rs` rewrites resolved AST paths at their recorded positions, not
identifier strings. Type/const arguments are rewritten before their enclosing path
is replaced. Field/pattern shorthand expands when necessary to preserve the original
field label. Bindings, labels, literals, and unrelated shadowed names are unchanged.
Selected type definitions are lifted only from the generated copy, avoiding duplicate
nominal definitions. The original source remains byte-for-byte unchanged.

Inherited/public visibility is preserved; restricted visibility and semantic attributes
are rejected rather than reinterpreted. Only literal documentation attributes are moved.
Original reusable items remain imports and rustc still enforces their field/type privacy.
The plan explicitly lists copied nominal declarations: **copied structs/enums have new
identity**. Transparent aliases follow their rewritten targets, which may remain original
types. This refinement avoids incorrectly claiming that a Rust alias creates a new type.

23 focused tests include real rustc consumers for returned fields, runtime `TypeId`
differences, original type identity through imports/aliases, two coexisting extractions,
recursive types, selected declarations, nested shadowing, signature/output scopes,
generic/const/variant arguments, shorthand patterns, raw/Unicode names, collision
failures, privacy, source preservation, and disappearance outside `cfg(test)`.
The [local-types walkthrough](../../examples/local-types/README.md) prints a real plan
from unannotated example source; this does not enable the public macro mode.

Also added: three bounded generated graph-property tests, with independent cycle
oracles and source/order/deduplication checks. The [coverage baseline](../coverage.md)
records measured task-007 line coverage and the unavailable branch-coverage metric.

## Conservative boundaries and follow-up

- Generic local types, enclosing generic captures, and implicit generic `Self` cannot
  be promoted by replacing a bare identifier. They fail before emission.
- Implementation-dependent operations still fail; task 009 must approve supported
  implementations before task 011 can integrate automatic generation.
- `use` declarations **inside** selected code/output require additional relocation
  analysis and are rejected. Surrounding import aliases are resolved normally.
- Local helper functions outside the selection are not copied to fake interoperability
  with the original unnameable type. Type-sensitive behavior such as `TypeId` is not
  claimed to remain equivalent after nominal promotion.
- Invocation placement and complete caller-scope discovery remain task 011 obligations.
  Known names can be supplied to the internal planner; generated names are not a stable
  public naming API or a promise of collision knowledge outside the supplied context.
- Declaration/context snapshots remain task 012; compilation never writes baselines.
