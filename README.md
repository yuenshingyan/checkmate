# Checkmate

Generate test-only Rust functions from existing source snippets—without adding
markers, annotations, or wrappers to the original code. Requires **Rust 1.88+**.
The workspace's [editor/development setup](docs/editor-support.md) pins Rust 1.99.1
and configures a compatible rust-analyzer macro protocol.

You supply the line range, function signature, typed inputs, optional output, and your
own tests. Checkmate extracts a copy; it does not execute earlier statements or capture
the original function's runtime values.

## Choose an extraction mode

| Mode | What Checkmate does | Starting point |
| --- | --- | --- |
| Ordinary (`context = "explicit"`) | Copies the selected statements/expression into your function; you provide needed imports/context at the invocation site | [Basic example](examples/basic/checkmate.toml) |
| Automatic (default; omit `context` or use `context = "auto"`) | Discovers supported context for a verified **library-root → package integration-test-root** extraction; reuses accessible originals or promotes supported local types | [Complete automatic example](examples/automatic-context/README.md) |

Automatic mode preserves original declaration identity and privacy. Copied function-local
structs/enums have **new nominal identity**; aliases remain transparent. It is conservative
syntax analysis, not full compiler inference. Unsupported or ambiguous context fails—there
is no silent fallback. See the [supported subset](docs/automatic-context.md).

To retain existing ordinary extraction, add `context = "explicit"` to inline metadata
or the TOML snippet definition, never beside `config`/`snippet` in a config call.
Binary sources, unit-test invocations, and bare imported macro names require this
ordinary mode. Automatic mode requires a literal qualified call at the verified
integration-test root. Do not change selected production source, macro definitions,
or snapshot contents for this migration; no source annotations are needed.

## Run the examples

From the repository root:

```sh
cargo test --workspace
cargo run -p checkmate-example
cargo test -p checkmate-auto-example
```

The basic package is the runnable binary. The automatic example is a library with
user-authored integration tests; it does not have a `cargo run` target.

## Ordinary extraction

The basic example keeps ordinary-mode metadata in `examples/basic/checkmate.toml`:

```toml
[snippets.add_if]
context = "explicit"
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

## Automatic extraction

The [new example's source](examples/automatic-context/src/lib.rs) constructs an `Invoice`
using an earlier `total` value. Its metadata explicitly supplies `total: u64` and
`quantity: u32`; automatic discovery supplies the original `Invoice` declaration's
access route, **not** those runtime values.

At the root of [`tests/automatic.rs`](examples/automatic-context/tests/automatic.rs):

```rust,ignore
checkmate::extract_fn!(config = "metadata/snippets.toml", snippet = "invoice");

#[test]
fn returned_value_is_the_original_library_type() {
    let value: checkmate_auto_example::Invoice = extracted_invoice(12, 4);
    assert_eq!((value.total, value.items), (12, 4));
}
```

The [configuration](examples/automatic-context/metadata/snippets.toml) omits `context`
and uses the automatic default; explicit `context = "auto"` is equivalent. No `Invoice`
import or manual declaration range is supplied.
The [walkthrough](examples/automatic-context/README.md) explains every file and path.

## Reviewable guards, never compilation writes

- In both modes, omitted `snapshot` guards selected text using
  `snapshots/<generated-function-name>.rs` from parsed `fn_sig` (not the snippet key,
  module, or range). `r#type` becomes `type.rs`; Unicode names are retained.
  Defaults use the same bases as explicit paths: package-relative inline,
  configuration-directory-relative in TOML.
- `snapshot = "path"` overrides the default; `snapshot = false` explicitly opts out.
  `true` and empty strings are invalid. Same function names in one base share a path;
  use distinct explicit paths for different bodies.
- Optional, automatic-only `context_snapshot` has no default and additionally guards the supported discovery model,
  including normalized signature/body/output and declaration/dependency mappings.
- Both context options are valid when `context` is omitted (automatic mode) and
  rejected with `context = "explicit"`.
- `context_report` names a test-only accessor returning that model as TOML. A user-authored
  test may print it for review; you explicitly create/update the baseline yourself.

Previously unchecked omissions are now guarded: manually create/review the default
baseline, retain a custom explicit path, or choose `snapshot = false`. Context-only
guarding also requires `snapshot = false`.

Neither guard relocates code or proves whole-program semantic equivalence. A missing
default fails with its exact resolved path; missing/mismatching guards never fall back.
Compilation never creates, writes, updates, approves, or prints baseline files.
See the [snippet migration guide](docs/usage.md#migration-from-unchecked-omission) and the
[context review workflow and limits](docs/context-snapshots.md).

## Guides and examples

- [Usage](docs/usage.md): dependency setup, inline/TOML options, paths, placement, and errors.
- [Example index](examples/README.md): original structs, copied locals, tuples, checked
  inherent methods, and clearly separate internal analysis walkthroughs.
- [Diagnostics](docs/diagnostics.md): bounded Checkmate reports versus rustc errors.
- [rust-analyzer setup](docs/editor-support.md): real editor expansion, required metadata,
  tested versions, and saved-file limitations.
- [Architecture](docs/architecture.md): discovery, scope resolution, graph, promotion, and emission.
- [Regression matrix](docs/regression-tests.md) and [coverage limits](docs/coverage.md):
  behavioral tests and bounded properties are not formal guarantees or new coverage measurements.
- [Completed milestone and deferred work](docs/tasks/README.md): task records and stacked branches.

Paths in inline options and `config` are relative to the invoking package. Paths **inside**
a TOML definition are relative to that configuration file, not the workspace root.

## Repository layout

```text
crates/checkmate/            Procedural macro, unit tests, and native consumer tests
examples/basic/             Ordinary extraction and a runnable binary
examples/automatic-context/ Complete public automatic workflow and reviewed baselines
examples/struct-return/     Original structs/tuples and internal analysis walkthroughs
examples/local-types/       Copied local types and context baseline inspection
examples/inherent-methods/  Conservative checked methods on copied local types
docs/                       Usage, safety contract, review workflow, and implementation records
```

## Development

```sh
cargo fmt -p checkmate --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Format the macro crate with `cargo fmt -p checkmate`. Do not broadly format line-selected
example sources or fixtures: their ranges and reviewed snapshots are intentional.
See [CONTRIBUTING.md](CONTRIBUTING.md). The milestone was developed on dependency-stacked
branches; the task records preserve that implementation history. Completion does not
imply a release or support for deferred features.
