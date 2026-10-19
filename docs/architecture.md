# Current architecture

[Overview](../README.md) · [Usage](usage.md) · [Future tasks](tasks/README.md)

## Workspace

The root `Cargo.toml` is a virtual workspace. `crates/checkmate` is the procedural
macro package; `examples/basic` and `examples/struct-return` are unpublished consumer
packages. One root `Cargo.lock` records the workspace's resolved dependencies.

The example keeps all of its own inputs together: `src/main.rs`, `src/tests.rs`,
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
| `source/ast.rs` | Test-gated automatic-mode foundation: whole-file AST, exact selection, enclosing scope locations |
| `scope.rs`, `scope/build.rs`, `scope/patterns.rs` | Test-gated lexical namespaces, scope tree, declaration identities, binding visibility and pattern validation |
| `scope/imports.rs` | Test-gated explicit import trees, aliases, module paths, and unvalidated path provenance |
| `source/origin.rs` | Test-gated source-to-library/binary-root identity from the package manifest |
| `resolve.rs`, `resolve/collect.rs` | Test-gated reference/input collection across snippet, supplied signature, and output |
| `snapshot.rs` | Compare selected source against a reviewed baseline; report first mismatch |
| `generate.rs` | Signature parsing, inclusive line selection, snapshot validation, body parsing, token emission |
| `tests.rs` | Unit tests for syntax, configuration, ranges, diagnostics, and snapshots |
| `walkthroughs.rs` | Executable documentation using the struct-return example's real source/config |

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

Compilation reads inputs but never updates them. The supplied function signature
and output are explicit; surrounding local values, imports, and types are not
captured or inferred. Ordinary rustc type, ownership, and visibility checks still apply.

## Tests

- `src/tests.rs`: isolated parser/expansion tests, using temporary projects where needed.
- `tests/extraction.rs`: compiled functions using inline and TOML definitions.
- `tests/fixtures/`: exact source selections, configurations, and snapshots; not all
  `.rs` files are standalone modules or valid crates.
- `tests/rebuild.rs`: a real temporary consumer crate, repeatedly rebuilt after source,
  configuration, and snapshot changes. Its inner Cargo runs are offline and use an
  isolated target directory; the outer build supplies the needed dependency cache.
- `examples/basic/src/tests.rs`: user-authored tests for the unmodified example source.
- `examples/struct-return/tests/extraction.rs`: public-API tests for original struct
  identity, tuple outputs, and ordinary method calls with explicit imports/types.
- `src/walkthroughs.rs`: five repository-only unit tests that print and assert internal
  analysis of the same `examples/struct-return` source/config. See the
  [walkthrough guide](../examples/struct-return/README.md).

## Planned work boundary

The [design contract](automatic-context.md) defines the opt-in syntax and safety
boundary. The [automatic-context backlog](tasks/README.md) describes new source AST, scope,
reference-resolution, and dependency-graph passes. Whole-file source selection and
lexical scope/import indexing, source-root mapping, and reference collection are
implemented and unit-tested, but remain test-gated until task 011 integrates the
opt-in pipeline. Final declaration/accessibility resolution and dependency graphs
are not implemented yet.

The lexical index distinguishes hoisted items from position-dependent locals and
tracks type/value/macro namespaces separately. It models closure capture, item
capture barriers, module boundaries, condition chains, and pattern bindings. IDs
are deterministic per traversal, not persisted cross-edit snapshot identities.
Explicit imports are indexed as namespace candidates, not claims about accessibility
or enabled dependencies. Qualified source paths preserve `crate`/`self`/`super` meaning;
manifest-based mapping distinguishes library and binary roots. Overlapping unproven
import namespaces, imported binding-pattern names, implementations, conditional
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

The current source/argument/generation separation provides places to add them without
turning the public proc-macro entry point into the implementation itself.
