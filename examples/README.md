# Examples

Run these commands from the repository root.

| Example | What it demonstrates | Status |
| --- | --- | --- |
| [Basic](basic/checkmate.toml) | Extracting a conditional, TOML definitions, reviewed snapshots | Current public API |
| [Struct return](struct-return/README.md) | Returning the original struct type, tuple outputs, user-authored tests, ordinary method calls | Current public API; imports remain explicit |
| [Local types](local-types/README.md) | Returning a copied function-local struct, explicit new identity, collision-safe plans | Internal promotion only; not a public macro mode |
| [Analysis walkthroughs](struct-return/README.md#internal-analysis-walkthroughs) | Import origins, reusable declarations, dependency graphs, inputs, scopes, and deferred methods | Internal foundations only, not automatic extraction |

## Public examples

```sh
cargo test -p checkmate-example
cargo run -p checkmate-example
cargo test -p checkmate-struct-example
```

## Internal analysis walkthroughs

```sh
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

These eight tests print and assert analysis of the **same source/configuration**
used by the struct-return and local-types examples. An expected diagnostic is checked by a passing
test; it is not an intentionally broken workspace build.

Automatic context is not publicly enabled yet: do not add `context = "auto"` to
`extract_fn!`. Tasks 009–011 still need to check implementations, improve diagnostics, and integrate generation.
No example modifies its selected source, inserts snippet markers, or updates a snapshot
during compilation. Keep the configured ranges and reviewed snapshots synchronized
when deliberately editing example source.
