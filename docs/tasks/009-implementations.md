# 009 — Handle a conservative implementation subset

- **Status:** Complete (internal/test-gated; public integration remains task 011)
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

## Implementation and evidence

The scope index now records impl/item and method/function capture boundaries and
receiver parameters, without introducing method names as lexical values. The catalog
retains implementation syntax; the resolver associates each header with its actual
nominal declaration ID, including shadowed names. Header discovery is not a privacy
approval. Reusing originals still checks routes, and rustc enforces member privacy.

`resolve/implementations.rs` checks **all** methods of each required copied type, not
only methods directly called by the snippet. Signatures, bodies, `Self`, named types,
constants, and helper references contribute identity-resolved dependencies to the graph.
Method dependencies are neither storage-layout nor alias-expansion edges, so mutually
referencing method signatures do not create spurious recursive-layout failures. Reused
originals remain terminals: their impls and private dependencies are never copied.

`resolve/implementations/methods.rs` approves a narrow dot-call subset: a proven owned
local nominal receiver and an inherent by-value `self` method. Source and generated
receiver identities must agree; supplying a differently typed parameter cannot retarget
an original call. Direct constructor shapes are distinguished: an uncalled tuple struct
constructor/tuple variant is a function item, not its result nominal type. Borrowing,
autoderef, trait dispatch, generic method lookup, and alias normalization are not guessed.
Explicit qualified calls can use supported borrowed methods without method lookup.

Promotion rewrites both declarations and impls by resolved identity. It preserves
method visibility and shorthand receiver syntax (`self`, `&self`, `&mut self`); syn's
implicit receiver types must not accidentally print as invalid `&self: &Renamed`.
All emitted items remain `#[cfg(test)]`, and nominal identity changes remain disclosed.

Evidence: 20 focused implementation tests (including real rustc consumers and runtime
assertions), three added scope tests, and a shared-source
[inherent-methods walkthrough](../../examples/inherent-methods/README.md). Tests cover
constructors, owned/borrowed methods, `Self`, enum patterns, raw names, multiple impls,
shadowed owners, generic type/const arguments, graph expansion, source preservation,
receiver dispatch, constructor function items, captures, privacy, rejection paths, and
disappearance outside `cfg(test)`. Compiler-backed tests also load the walkthrough's
actual source and compare original/generated results. This is regression evidence,
not formal equivalence or a new coverage-percentage measurement.

## Explicit initial boundaries

- Only non-generic inherent impls with ordinary non-generic methods are indexed.
  Trait/blanket impls, associated constants/types/macros, generic/const/async/unsafe/ABI
  methods, and conditional/semantic attributes fail. Literal documentation is retained;
  derives and doc macros are not expanded or copied.
- Copied method bodies may not capture surrounding values/generics. Nested items/imports,
  inferred/opaque/trait types, and unresolved helpers or associated operations fail.
- Unknown or borrowed dot-call receivers fail even if a same-named inherent method exists:
  autoref/deref can select a trait before reaching that method. Qualified calls preserve
  original syntax; no automatic UFCS conversion changes dispatch.
- Implementations through **type aliases** require additional normalization and fail;
  ordinary import aliases can resolve to a nominal owner.
- Impl declarations inside selected code or output require separate lifting and fail.
  Surrounding implementations are discovered without explicit ranges.
- Whole-file indexing remains conservative and may reject an unrelated unsupported impl.
- Caller placement/public integration remains task 011; context snapshots remain 012.
  Original source/configuration/snapshots are never written by compilation.
