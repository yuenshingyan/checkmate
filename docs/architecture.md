# Current architecture

[Overview](../README.md) · [Usage](usage.md) · [Future tasks](tasks/README.md)

## Workspace

The root `Cargo.toml` is a virtual workspace. `crates/checkmate` is the procedural
macro package; `examples/basic` is an unpublished consumer package. One root
`Cargo.lock` records the workspace's resolved dependencies.

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
| `snapshot.rs` | Compare selected source against a reviewed baseline; report first mismatch |
| `generate.rs` | Signature parsing, inclusive line selection, snapshot validation, body parsing, token emission |
| `tests.rs` | Unit tests for syntax, configuration, ranges, diagnostics, and snapshots |

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

## Planned work boundary

The [automatic-context backlog](tasks/README.md) describes new source AST, scope,
reference-resolution, and dependency-graph passes. Those passes are not present yet.
The current source/argument/generation separation provides places to add them without
turning the public proc-macro entry point into the implementation itself.
