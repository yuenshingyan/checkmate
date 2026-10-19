# Examples

Run these commands from the repository root.

| Example | What it demonstrates | Status |
| --- | --- | --- |
| [Basic](basic/checkmate.toml) | Extracting a conditional, TOML definitions, reviewed snapshots | Current public API |
| [Struct return](struct-return/README.md) | Returning the original struct type, tuple outputs, user-authored tests, ordinary method calls | Ordinary explicit imports and separate public automatic test |
| [Local types](local-types/README.md) | Returning a copied function-local struct, explicit new identity, collision-safe plans | Public automatic test plus internal walkthrough |
| [Inherent methods](inherent-methods/README.md) | Checked constructors, borrowed qualified calls, owned methods, and preserved behavior on copied types | Public automatic test plus internal walkthrough |
| [Analysis walkthroughs](struct-return/README.md#internal-analysis-walkthroughs) | Import origins, reusable declarations, dependency graphs, inputs, scopes, and deferred methods | Internal foundations only, not automatic extraction |

## Public examples

```sh
cargo test -p checkmate-example
cargo run -p checkmate-example
cargo test -p checkmate-struct-example
cargo test -p checkmate-local-example -p checkmate-inherent-example
```

## Internal analysis walkthroughs

```sh
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

These nine tests print and assert analysis of the **same source/configuration**
used by the struct-return, local-types, and inherent-methods examples. An expected diagnostic is checked by a passing
test; it is not an intentionally broken workspace build. The missing-input walkthrough
prints a [structured diagnostic](../docs/diagnostics.md) with snippet identity and bounded
source context.

Separate `tests/automatic.rs` files now exercise public `context = "auto"` at verified
integration-test roots. Local/inherent examples use new `automatic.toml` files; existing
walkthrough source/configuration/snapshot inputs remain unchanged. See the
[public placement restrictions](../docs/usage.md#automatic-context-opt-in).
Context snapshots and broader integrated documentation remain later tasks.
No example modifies its selected source, inserts snippet markers, or updates a snapshot
during compilation. Keep the configured ranges and reviewed snapshots synchronized
when deliberately editing example source.
