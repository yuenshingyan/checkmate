# 007 — Build the dependency graph

- **Status:** Complete (internal/test-gated; no promotion or public generation)
- **Dependencies:** 006
- **Goal:** Discover transitive declaration requirements once and deterministically.

## Scope

Represent referenced declarations and their dependencies using stable IDs. Traverse
field types, alias targets, generic bounds, and other supported syntax. Deduplicate
nodes and track visiting/visited states. Distinguish legal recursive type relationships
from unsupported cycles; use stable emission ordering without assuming every graph is a DAG.

## Non-goals

Arbitrary helper-function evaluation or unsupported declaration kinds.

## Acceptance tests

- A struct containing another named type discovers both declarations.
- Shared dependencies appear once and output is deterministic.
- Recursive supported types terminate analysis without duplicate emission.
- Unsupported dependencies include the chain that led to them.
- An unresolved graph never produces a partially guessed expansion.

## Implementation and evidence

`resolve/graph.rs` builds an all-or-error graph from task 006's resolved references.
Source nodes use declaration IDs; external nodes use qualified symbolic paths.
Ordered maps/sets deduplicate dependencies and provide stable emission order, without
claiming the graph is a DAG. Per-reference paths, usage, source positions, and resolved
targets remain available to the later renaming pass.

Only function-local type dependencies are expanded. Reused originals and external
items are terminal: their private implementation dependencies stay in their original
crate and must not be copied or exposed. `graph/collect.rs` traverses fields, enum
payloads/discriminants, alias targets, trait/where bounds, and supported constant
expressions. Generic local declarations still fail under task 001's initial subset.

Discovery is iterative and expands each node once. Separate visiting/visited passes
on the completed graph reject alias-expansion cycles and unguarded layout cycles.
Where-clause constraints are not layout or alias-RHS edges. Trait-object arguments
still participate in alias expansion. Guarded recursive nominal relationships are
allowed; rustc remains responsible for full layout, trait, and typing checks.

25 focused tests cover shared/transitive dependencies, lexical shadowing, stable
ordering across roots/checkouts, recursive aliases and nominal types, complete-graph
cycle checks, a 501-alias chain, unsupported dependency chains, and all-or-error
failure. Real rustc checks validate representative legal and illegal recursion cases.
A seventh executable walkthrough deduplicates the public struct example's dependencies.

## Conservative boundaries

- Proven indirection includes references, raw/function pointers, the first type
  argument of known standard `Box`/`Vec`/`Rc`/`Arc`, and `PhantomData`'s zero-sized
  relationship. Allocator arguments are not incorrectly treated as indirect.
- Arbitrary wrapper layout is not inferred. Recursive relationships through opaque
  containers can be valid Rust and are nevertheless rejected by this initial subset.
- Constant expressions may use literals, simple operators/casts, and original named
  constants. Helper calls, local constant copying, and values of copied types are
  unsupported; nothing is evaluated to reconstruct a value.
- Captured lifetimes, non-documentation attributes, inferred/opaque/impl-trait types,
  and unresolved trait/associated operations fail. Self-contained higher-ranked
  function/trait binders and explicit `'static` references are tracked.
- Graph success alone is not permission to emit copied declarations. Promotion and
  collision handling belong to task 008; implementation/invocation checks belong to 009–011.
  IDs are deterministic for an unchanged source/index, not persisted cross-edit IDs.
