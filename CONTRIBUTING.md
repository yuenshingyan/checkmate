# Development

## Setup and verification

Use a Rust toolchain supporting edition 2024. Run commands at the repository root:

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
cargo test -p checkmate-struct-example
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

The internal walkthrough unit tests read the source and configuration in
`examples/struct-return` and `examples/local-types`; run them from a full repository checkout. They share the
same source as the public integration tests, print their analysis, and assert the
illustrated results. They do not enable automatic context in the public macro.

## Coverage and generated properties

See [the measured baseline and limitations](docs/coverage.md). Task 008 adds bounded,
deterministic property-style tests without an additional testing dependency:

```sh
cargo test -p checkmate --lib resolve::graph::properties
cargo test -p checkmate --lib resolve::promote
```

Promotion tests compile temporary original libraries and generated consumers with
rustc. They verify runtime behavior and identity differences, not merely token strings.
The public automatic-context pipeline is still disabled.

## Where changes belong

- Macro implementation and tests: `crates/checkmate/`.
- Runnable consumer examples: `examples/` (keep each package's config/snapshots with it).
- Public workflows: `docs/usage.md`.
- Current module responsibilities: `docs/architecture.md`.
- Future automatic-context work: `docs/tasks/README.md` and its linked tasks.

Keep implementation behavior separate from planned features. Each backlog task has
dependencies and acceptance criteria; complete and test them before checking it off.
Do not implement all tasks merely because they have been recorded.

## Snapshot changes

Snapshots are reviewed source baselines, not generated build artifacts. If a selection
moves, correct its line range. If source content intentionally changes, review and
explicitly update the associated snapshot. Never add automatic writes to compilation.
Commit relevant source/config/snapshot changes together.
