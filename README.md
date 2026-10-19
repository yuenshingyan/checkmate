# Checkmate

Generate test-only Rust functions from existing source snippets—without adding
markers, annotations, or wrappers to the original code.

## What works today

- Extract complete statements or an expression using inclusive line ranges.
- Supply a function signature and optional return expression.
- Define extractions inline or in an external TOML file.
- Guard against line drift with explicitly reviewed snippet snapshots.
- Get source-aware extraction errors and automatic rebuilds when input files change.

Users write their own tests. Automatic discovery of surrounding types/imports is
**not publicly enabled yet**. Scope, declaration, and dependency analysis exist as
tested internal foundations; see the [task backlog](docs/tasks/README.md).

## Quick start

Run the included example and test suite from the repository root:

```sh
cargo test --workspace
cargo run -p checkmate-example
```

The example keeps its metadata in `examples/basic/checkmate.toml`:

```toml
[snippets.add_if]
fn_sig = "fn add_if(a: f32, b: f32) -> f32"
module = "src/main.rs"
start = 7
end = 11
output = "c"
snapshot = "snapshots/add_if.rs"
```

Its test module uses:

```rust
use checkmate::extract_fn;

extract_fn!(config = "checkmate.toml", snippet = "add_if");

#[test]
fn adds_nonnegative_inputs() {
    assert_eq!(add_if(2.0, 2.0), 4.0);
}
```

Paths are relative to the example package/configuration, not the workspace root.
See the [usage guide](docs/usage.md) for dependency setup, inline syntax, snapshots,
path resolution, and current limitations.

## More examples

See the [example index](examples/README.md) for a struct-returning extraction,
tuple outputs, and printed walkthroughs of the internal declaration/dependency analyzer.

```sh
cargo test -p checkmate-struct-example
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

The public struct-return example still imports its type explicitly. The walkthroughs
show internal analysis, not a new automatic-extraction API.

## Repository layout

```text
crates/checkmate/       Procedural macro, unit tests, and integration tests
examples/basic/        Standalone consumer with source, tests, config, and snapshot
examples/struct-return/ Struct/tuple examples and shared inputs for analysis walkthroughs
docs/usage.md          Public API and workflows
docs/architecture.md   Current implementation and module responsibilities
docs/tasks/            Small, dependency-ordered tasks for automatic context resolution
```

## Development

```sh
cargo fmt -p checkmate --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Format the macro crate with `cargo fmt -p checkmate`. The example and extraction
fixtures intentionally retain their line layout: formatting selected source can
invalidate ranges and snapshots. See [CONTRIBUTING.md](CONTRIBUTING.md).
