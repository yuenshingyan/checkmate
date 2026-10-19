# Development

## Setup and verification

Use Rust 1.88+ (edition 2024 and stable compiler source-location APIs). Run commands at the repository root:

```sh
cargo fmt -p checkmate --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p checkmate-example
```

To format implementation code, use `cargo fmt -p checkmate`. Do not broadly format
line-selected example sources or snapshot/fixture files without reviewing their
ranges and baselines. Some fixture `.rs` files contain statements rather than modules.
The example's original source is deliberately not annotated for this tool.

The rebuild integration test launches Cargo in a temporary consumer package. It
uses offline mode and an isolated target directory, so the first run may take longer
while compiling dependencies already downloaded by the outer workspace build.

## Executable walkthroughs

```sh
cargo test -p checkmate-auto-example
cargo test -p checkmate-struct-example
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

The internal walkthrough unit tests read the source and configuration in
`examples/struct-return`, `examples/local-types`, and `examples/inherent-methods`; run them from a full repository checkout. They share the
same source as the public integration tests, print their analysis, and assert the
illustrated results. They test individual stages separately from the new public
`tests/automatic.rs` examples.

The [complete public workflow](examples/automatic-context/README.md) is a separate
library/integration-test example, not an internal analysis API. Its source is ordinary
Rust; extraction metadata, tests, and reviewed baselines live in separate files.

## Coverage and generated properties

See [the measured baseline and limitations](docs/coverage.md). Task 008 adds bounded,
deterministic property-style tests without an additional testing dependency:

```sh
cargo test -p checkmate --lib resolve::graph::properties
cargo test -p checkmate --lib resolve::promote
```

Promotion tests compile temporary original libraries and generated consumers with
rustc. They verify runtime behavior and identity differences, not merely token strings.
Task-009 implementation tests also compile the actual inherent-methods example source;
all copied methods and their dependencies are checked, including unused ones. Task 011
adds public-pipeline tests in `src/automatic/tests.rs` and `tests/automatic.rs`, including
real Cargo expansion and rebuilds. Task 012 adds context models/identities under
`src/snapshot/` and three real-Cargo groups in `tests/context_snapshots.rs`. Task 013
adds eight public value/failure/rebuild groups with shared input-inventory checks;
see the [regression matrix and oracle limitations](docs/regression-tests.md).

```sh
cargo test -p checkmate --test public_values --test public_failures --test public_rebuilds
```

## Internal diagnostics

```sh
cargo test -p checkmate --lib diagnostics
cargo test -p checkmate --lib walkthroughs::missing_input -- --nocapture
```

See [the diagnostic contract](docs/diagnostics.md). Preserve typed error metadata through
context wrappers; do not classify errors by matching messages. The public adapter
uses the bounded renderer with snippet identity/source context and an actual invocation
span, not the legacy internal `Display` string. Error handling must remain read-only.

## Parser backend boundary

Automatic file analysis needs real file-relative spans. `proc_macro2`'s compiler parser
assigns invocation spans to strings, and its backend detection is process-global, not
thread-local. The pinned dependency exposes `fallback::force/unforce`; Checkmate's macro
entry mutex serializes **every** expansion path while an unwind-safe guard selects the
file parser. Only plain data/serialized token text crosses the boundary. Do not move
compiler tokens into analysis, remove the entry lock, or assume a worker thread isolates
the backend. Updates to proc-macro2 must rerun real Cargo tests that mix ordinary and
automatic invocations, Unicode locations, and failed expansions.

## Where changes belong

- Macro implementation and tests: `crates/checkmate/`.
- Runnable consumer examples: `examples/` (keep each package's config/snapshots with it).
- Public workflows: `docs/usage.md`.
- Current module responsibilities: `docs/architecture.md`.
- Completed milestone records and deferred scope: `docs/tasks/README.md` and its linked tasks.

Keep implementation behavior separate from planned features. Each backlog task has
dependencies and acceptance criteria; complete and test them before checking it off.
Do not implement all tasks merely because they have been recorded.

## Snapshot changes

Snapshots are reviewed source baselines, not generated build artifacts. If a selection
moves, correct its line range. If source content intentionally changes, review and
explicitly update the associated snapshot. Never add automatic writes to compilation.
Commit relevant source/config/snapshot changes together.

[Context snapshots](docs/context-snapshots.md) are versioned logical models, not emitted
helper names or traversal IDs. Keep review data separate from filesystem writes. The
`context_report` accessor returns a literal, not a named constant that could capture
Rust binding patterns. Test identifier conflicts, unknown caller-module scopes, schema
failures, unrelated edits/moves, and baseline-only incremental rebuilds. Original nodes
remain terminals; do not claim to fingerprint unexpanded private dependencies.

```sh
cargo test -p checkmate --lib snapshot::
cargo test -p checkmate --test context_snapshots
cargo test -p checkmate-local-example --test context_snapshot print_context_for_review -- --ignored --nocapture
```
