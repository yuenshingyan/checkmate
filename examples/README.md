# Examples

Run these commands from the repository root.

| Example | What it demonstrates | Status |
| --- | --- | --- |
| [Basic](basic/checkmate.toml) | Extracting a conditional, TOML definitions, reviewed snapshots | Ordinary mode; runnable binary |
| [Complete automatic workflow](automatic-context/README.md) | Original struct identity without imports, explicit inputs, inline tuple output, config-relative paths, both reviewed guards | Public automatic mode; library and integration tests |
| [Struct return](struct-return/README.md) | Returning the original struct type, tuple outputs, user-authored tests, ordinary method calls | Ordinary explicit imports and separate public automatic test |
| [Local types](local-types/README.md) | Returning a copied function-local struct, explicit new identity, collision-safe plans | Public automatic test plus internal walkthrough |
| [Context baseline](local-types/README.md#reviewed-context-baseline) | Versioned declaration/dependency review, snippet-guard coexistence, explicit report inspection | Public automatic context guard |
| [Inherent methods](inherent-methods/README.md) | Checked constructors, borrowed qualified calls, owned methods, and preserved behavior on copied types | Public automatic test plus internal walkthrough |
| [Analysis walkthroughs](struct-return/README.md#internal-analysis-walkthroughs) | Import origins, reusable declarations, dependency graphs, inputs, scopes, and deferred methods | Internal foundations only, not automatic extraction |

## Public examples

```sh
cargo test -p checkmate-example
cargo run -p checkmate-example
cargo test -p checkmate-auto-example
cargo test -p checkmate-struct-example
cargo test -p checkmate-local-example -p checkmate-inherent-example
```

The basic and ordinary struct-return definitions use `context = "explicit"`. Public
automatic examples omit `context`, demonstrating the default (explicit `"auto"` also works).
Only the basic package has a `cargo run` target. Automatic mode requires a verified
library root and literal qualified invocation at a package integration-test root;
these library examples run through `cargo test`.

To inspect the complete example's reviewed context without writing a baseline:

```sh
cargo test -p checkmate-auto-example --test automatic print_context_for_review -- --ignored --nocapture
```

See the [explicit review/update steps](automatic-context/README.md#4-review-changes-explicitly).
Copied locals have **new nominal identity**; they are not interchangeable with originals.
Original reuse and transparent aliases have different identity behavior, demonstrated
separately by the linked tests.

## Internal analysis walkthroughs

```sh
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

These nine tests print and assert analysis of the **same source/configuration**
used by the struct-return, local-types, and inherent-methods examples. An expected diagnostic is checked by a passing
test; it is not an intentionally broken workspace build. The missing-input walkthrough
prints a [structured diagnostic](../docs/diagnostics.md) with snippet identity and bounded
source context.

Separate `tests/automatic.rs` files exercise default automatic context at verified
integration-test roots. Local/inherent examples use `automatic.toml` files; selected
source and snapshot contents remain unchanged. Ordinary extraction metadata now
explicitly selects `context = "explicit"`. See the
[public placement restrictions](../docs/usage.md#automatic-context-opt-in).
Task 012 adds a separate local-type context baseline and an ignored, explicitly run
inspection test. [Context snapshots](../docs/context-snapshots.md) protect the supported
discovery graph, not whole-program semantics. The complete automatic example provides
the integrated public workflow; the nine internal walkthroughs remain contributor-only
tests, not alternative public macro entry points.
No example modifies its selected source, inserts snippet markers, or updates a snapshot
during compilation. Keep the configured ranges and reviewed snapshots synchronized
when deliberately editing example source.
