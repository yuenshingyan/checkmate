# Current architecture

[Overview](../README.md) · [Usage](usage.md) · [Milestone and deferred work](tasks/README.md)

## Workspace

The root `Cargo.toml` is a virtual workspace. `crates/checkmate` is the procedural
macro package. `examples/basic` demonstrates ordinary extraction; the new
[`examples/automatic-context`](../examples/automatic-context/README.md) demonstrates a
complete public automatic workflow, explicit inputs, original struct identity, tuple
output, and both reviewed guards. `examples/struct-return`, `examples/local-types`, and
`examples/inherent-methods` provide additional public examples and shared source for
internal walkthroughs. All examples are unpublished workspace packages. One root
`Cargo.lock` records the workspace's resolved dependencies.

The basic example keeps all of its own inputs together: `src/main.rs`, `src/tests.rs`,
`checkmate.toml`, and `snapshots/add_if.rs`. Moving the example does not change its
package-relative configuration or the bytes of its source snippet.

## Macro modules

All paths below are inside `crates/checkmate/src/`.

| Module | Responsibility |
| --- | --- |
| `lib.rs` | Public `extract_fn!` entry point, Cargo package root, compile-error conversion |
| `arguments.rs` | Inline/config invocation syntax, duplicate/unknown option validation |
| `config.rs` | TOML schema, named snippet lookup, package/config-relative path resolution |
| `source.rs` | Read-only UTF-8 source IO, dependency path literals, bounded source previews |
| `automatic.rs`, `automatic/placement.rs` | Opt-in orchestration, parser-backend boundary, verified integration-test placement and caller namespace checks |
| `source/ast.rs` | Whole-file AST, exact selection, enclosing scope locations |
| `scope.rs`, `scope/build.rs`, `scope/patterns.rs` | Lexical namespaces, scope tree, declaration identities, binding visibility and pattern validation |
| `scope/imports.rs` | Explicit import trees, aliases and module paths; standalone provenance inspection remains test-only |
| `source/origin.rs` | Source-to-library/binary-root identity from the package manifest |
| `resolve.rs`, `resolve/collect.rs` | Reference/input collection across snippet, supplied signature, and output |
| `resolve/catalog.rs`, `resolve/declarations.rs` | Original-item identities, namespace-aware reusable paths/imports, local-type IDs, conservative access checks |
| `resolve/graph.rs`, `resolve/graph/collect.rs` | Deterministic transitive dependencies, recursive type/alias validation, dependency-chain errors |
| `resolve/implementations.rs`, `resolve/implementations/methods.rs` | Inherent implementation dependencies and conservative owned-receiver dispatch checks |
| `diagnostics.rs` | Structured categories, source/generated sites, subjects/chains, bounded reports, and explicit invocation-anchored conversion |
| `resolve/promote.rs`, `resolve/promote/rewrite.rs` | Local-type promotion plans, unique names, identity-aware AST rewriting, and test-only emission |
| `snapshot.rs` | Compare selected source against a reviewed baseline; report first mismatch |
| `snapshot/context.rs`, `snapshot/identity.rs` | Versioned context model, named logical identities, normalized comparison, bounded differences, no baseline writes |
| `generate.rs` | Signature parsing, inclusive line selection, snapshot validation, body parsing, token emission |
| `tests.rs` | Unit tests for syntax, configuration, ranges, diagnostics, and snapshots |
| `walkthroughs.rs` | Executable documentation using the examples' real source/config |

## Expansion pipeline

```text
extract_fn! invocation
    → arguments::Invocation
    → config resolution (inline or named TOML definition)
    → signature and line-range validation
    → source selection and optional snapshot comparison
    → Rust block parsing and optional output expression
    → #[cfg(test)] function and include_str! dependency items
```

Snapshots are checked before parsing the selected block so that a moved, incomplete
snippet produces a drift diagnostic first. Source, config, and snapshot files are
tracked with emitted `include_str!` items, including files that are not Rust modules.
All generated items are gated by `cfg(test)`.

Compilation reads inputs but never updates them. The supplied function signature,
input types, and output remain explicit; surrounding runtime values are never captured.
Ordinary rustc type, ownership, and visibility checks still apply.

`context = "auto"` takes a separate route after configuration resolution:

```text
snapshot/range checks → whole-file AST and selection
    → library origin + compiler/manifest/caller-root placement proof → scopes
    → reference/input collection → resolver → shared graph → PromotionPlan
    → optional context model + reviewed baseline comparison
    → test-only plan + optional review accessor + file dependency markers
```

Only a complete approved plan emits tokens. The source, caller root, package manifest,
configuration, and optional snippet/context baselines are tracked; no out-of-line source modules or
workspace manifests are read. Caller proof uses physical compiler locations, literal
fully qualified invocation tokens, target layout/name disambiguation, and conservative
namespace/attribute checks. Public `SameTarget` placement remains disabled.

Caller capture uses an original input token's physical location, before reconstruction;
editor hosts may give `Span::call_site()` definition coordinates instead. This does not
replace any placement check or authorize forwarded tokens. The
[tested rust-analyzer setup](editor-support.md) enables a metadata-capable macro protocol;
legacy hosts still fail closed, and saved files remain the analysis inputs.

File analysis uses proc-macro2's pinned fallback parser because compiler-parsed strings
lack useful file-relative locations. A process-global backend switch is protected by a
macro-entry mutex covering **all** invocations and an unwind-safe restoration guard.
A worker thread alone does not isolate this global setting. Only plain data and token
text cross the backend boundary; emitted tokens and bounded errors receive real
invocation spans. The ordinary extraction path retains its original compiler tokens.

## Tests

- `src/tests.rs`: isolated parser/expansion tests, using temporary projects where needed.
- `src/options_tests.rs`, `src/automatic/tests.rs`: opt-in parity, checked placement,
  complete dependency markers, all-item test gating, and fail-closed plans.
- `tests/caller_spans.rs`: original argument locations, literal delimiters/config calls,
  and continued rejection of forwarded, nested, and included invocations.
- `scripts/test_rust_analyzer.py`: opt-in real LSP expansion, hover, diagnostics,
  unsaved-input rejection, and buffer restoration (separate from Cargo tests).
- `tests/automatic.rs`: real-Cargo original/local identity, methods, privacy, backend
  restoration, unsupported placement, incremental context/config/snapshot/manifest changes,
  and input immutability.
- `src/snapshot/context/tests.rs`, `src/snapshot/identity.rs`: stable identities,
  normalized content/dependency diffs, ambiguity/schema failures, and read-only behavior.
- `tests/context_snapshots.rs`: explicit review, native context-only drift/recompilation,
  baseline-only edits, coexistence with snippet guards, report-name safety/test gating,
  and byte-identical build inputs.
- `tests/public_values.rs`, `tests/public_failures.rs`, `tests/public_rebuilds.rs`:
  eight native public-workflow groups for enum/alias/tuple returns, scoped identity,
  dependency graphs, explicit inputs, privacy, unsupported contexts, and rebuilds.
  `tests/support/mod.rs` checks all input bytes and unexpected file creation around
  each build; see the [regression matrix](regression-tests.md).
- `examples/local-types/tests/context_snapshot.rs`: public reviewed baseline and an
  explicitly invoked, read-only report printer.
- `examples/*/tests/automatic.rs`: public automatic extraction using the struct-return,
  local-types, and inherent-methods sources, plus the complete automatic-context example.
  The latter checks original `Invoice` identity, explicit inputs rather than earlier
  initializer execution, original result comparisons, and inline tuple output.
- `tests/extraction.rs`: compiled functions using inline and TOML definitions.
- `tests/fixtures/`: exact source selections, configurations, and snapshots; not all
  `.rs` files are standalone modules or valid crates.
- `tests/rebuild.rs`: a real temporary consumer crate, repeatedly rebuilt after source,
  configuration, and snapshot changes. Its inner Cargo runs are offline and use an
  isolated target directory; the outer build supplies the needed dependency cache.
- `examples/basic/src/tests.rs`: user-authored tests for the unmodified example source.
- `examples/struct-return/tests/extraction.rs`: public-API tests for original struct
  identity, tuple outputs, and ordinary method calls with explicit imports/types.
- `src/resolve/declarations/tests.rs`: real rustc consumers verifying generated imports,
  original type identity, and item/field privacy failures without source modification.
- `src/resolve/graph/tests.rs`: shared/transitive dependencies, stable ordering,
  recursive relationships, unsupported chains, and compiler-backed recursion checks.
- `src/resolve/promote/tests.rs`: compiler-backed promotion, explicit nominal identity
  changes, AST rewriting, visibility/collision checks, and rejection paths.
- `src/resolve/promote/tests/implementations.rs`: compiler-backed inherent methods, receiver
  identity/dispatch, constructor function items, transitive behavior dependencies, privacy,
  unsupported contexts, and the actual inherent-methods example source.
- `src/resolve/graph/properties.rs`: bounded generated graph/source properties checked
  against independent oracles; see [coverage and properties](coverage.md).
- `src/diagnostics/tests.rs`, `src/diagnostics/integration.rs`: bounded Unicode/control-safe
  reports, actual resolver failure paths, 501-node chains, preserved categories/sites,
  and read-only failure behavior; see [diagnostics](diagnostics.md).
- `src/walkthroughs.rs`: nine repository-only unit tests that print and assert internal
  analysis of `examples/struct-return`, `examples/local-types`, and `examples/inherent-methods` source/config. See the
  [walkthrough guide](../examples/struct-return/README.md).

## Automatic-context boundary

The [design contract](automatic-context.md) defines the supported subset. The public
opt-in connects AST/scope/resolver/graph/promotion analysis for verified library-source
and package integration-test roots. Omission preserves ordinary extraction. Optional
[context snapshots](context-snapshots.md) use the same validated graph as promotion.
The [regression matrix](regression-tests.md) and
[complete example](../examples/automatic-context/README.md) cover the public workflow.
The 14-task milestone is complete; full compiler inference, arbitrary source discovery,
and other [deferred features](tasks/README.md#out-of-scope-for-this-milestone) are not implemented.

The lexical index distinguishes hoisted items from position-dependent locals and
tracks type/value/macro namespaces separately. It models closure capture, item
capture barriers, module boundaries, condition chains, and pattern bindings. IDs
are deterministic per traversal, not persisted cross-edit snapshot identities.
Explicit imports are indexed as namespace candidates, not claims about accessibility
or enabled dependencies. Qualified source paths preserve `crate`/`self`/`super` meaning;
manifest-based mapping distinguishes library and binary roots. Overlapping unproven
import namespaces, imported binding-pattern names, unsupported implementation forms, conditional
compilation, and opaque binding-producing syntax still fail explicitly. Bare unresolved
prelude constructors in binding patterns also fail; qualified constructor patterns are
unambiguous. Ordinary extraction is unaffected by these limitations.

Reference collection indexes a separate analysis function containing the supplied
signature, exact selected lines, and optional output. It maps body positions back
to the original file and retains distinct source/generated declaration identities.
Source-proven constant patterns are preserved rather than misread as new bindings
when their declarations are absent from the analysis function. Earlier statements
are never copied or executed. Signature/output locations are explicitly labeled;
they are not presented as original-file locations.

Only explicit supplied parameters satisfy required outer-value inputs. The collector
reports missing inputs and capture/shadowing changes, and records unknown roots,
ambiguous type/const arguments, methods, and qualified final targets as deferred
analysis. A collected root identity is not proof of final member resolution or access.

Declaration resolution borrows original item syntax and produces reusable qualified
imports or function-local type IDs. It does not copy items. Internal callers must
establish same-target or same-package integration-test placement; the latter requires
a verified library root/name. Public canonical routes and simple public item re-exports
are checked syntactically. rustc remains authoritative for same-target privacy,
private fields, type access, availability, and typing. Generic arguments stay in the
original AST; emitted base imports must not replace or erase them.

Known unshadowed prelude mappings cover primitive scalar/string-slice types, `String`,
`Vec`, `Box`, `Option`, `Result`, their `Some`/`None`/`Ok`/`Err` constructors, and
`Clone`, `Copy`, `Sized`, `Send`, `Sync`, `Default`. The supported source index rejects
prelude-changing crate attributes; Cargo `std`/`core` name collisions are also rejected.
No other prelude name is guessed. External types in the same target remain symbolic
Rust-checked paths. Nonstandard cross-target dependency aliases do not prove type
identity and are rejected, as are external values and unresolved trait/associated
operations. A successful reference plan cannot silently drop pending method analysis.

The dependency graph expands only function-local struct/enum/alias context. Reused
originals and external paths are terminals: their private implementation dependencies
are not requirements to copy. Source declaration IDs and external qualified paths
provide deterministic keys; maps/sets deduplicate nodes and retain individual source
reference sites for later renaming. Stable ID order is an emission order, not a claim
that recursive relationships form a DAG.

Iterative discovery terminates even with recursive types. Separate visiting/visited
passes over the complete graph check alias expansion and unguarded layout cycles.
Pointer/function-pointer and known standard container indirection is tracked; opaque
wrapper layouts are not guessed. Type/where constraints are distinguished from layout
edges and alias-RHS references. Bounds, fields, aliases, and simple original-constant
expressions are analyzed without evaluating helpers or initializers. Graph failures
include a root-to-dependency chain and discard the entire plan. Promotion and
implementation policy must also approve a graph before the public pipeline emits it.

Promotion plans generate deterministic extraction-specific names and rewrite resolved
paths in declarations, the signature, snippet, and output. Reference collection retains
both original diagnostic locations and exact analysis-shell positions for rewriting.
Arguments are rewritten before their enclosing path; shorthand fields/patterns retain
labels. Promoted definitions selected in the snippet are removed only from the generated
copy. Copies preserve inherited/public visibility; restricted visibility, nonliteral doc
attributes, semantic attributes, and selected/output imports fail conservatively.

Implementation headers are associated with nominal declaration IDs; methods introduce
receiver/parameter bindings but not free lexical method names. Every method on a copied
type contributes signature/body dependencies, including otherwise-unused methods. These
behavior edges are distinct from layout/alias edges. Reused originals retain their own
implementations and private dependencies without inspection/copying for emission.
Context snapshots record their direct declaration and known same-file inherent impl
syntax as review data, but do not expand their private/transitive dependencies.

Explicit inherent associated calls retain qualified syntax. Dot calls require a proven
owned local nominal receiver and an inherent by-value `self` method, with matching original
and generated receiver identities for source calls. Borrowed/autoderef/trait dispatch is
not inferred. Bare tuple constructors are function items, not owned result values. See
[the precise implementation subset](tasks/009-implementations.md). Receiver shorthand
stays shorthand after rewriting; selected/output impls fail rather than being duplicated.

Internal analysis APIs return boxed structured errors, preserving typed categories,
identifier/declaration subjects, precise source/generated sites, and dependency chains
through wrappers. A reporting request supplies snippet identity, file, and selected range.
Reports are capped at 8 KiB with bounded Unicode-safe previews and escaped controls.
Generated signature/output positions are never previewed using original-file text.
The explicit conversion to a compile error takes the invocation span; ordinary rustc
type/borrow/privacy errors are not remapped. The public adapter renders before discarding
analysis spans and constructs the `syn::Error` with the original invocation span.

Generated items are test-gated, but the analysis implementation is production code.
Copied structs/enums are explicitly recorded and documented as new nominal identities;
aliases follow their targets, and reusable originals are never copied. Known caller and
analysis identifiers are checked for collisions. Calls to the requested generated function
in user tests are references, not conflicting declarations. Unsupported caller contexts
fail rather than being treated as empty scopes.
