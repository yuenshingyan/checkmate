# Automatic context: a complete public workflow

Run commands below from the **repository root**, with Rust 1.88+:

```sh
cargo test -p checkmate-auto-example
cargo build -p checkmate-auto-example --lib
```

This is a library plus integration tests, not a binary to run with `cargo run`.
The [basic example](../basic/checkmate.toml) remains the ordinary-extraction starting point.

## 1. Leave the original source alone

[`src/lib.rs`](src/lib.rs) is ordinary Rust, with no Checkmate imports, markers, attributes,
wrappers, or test-specific declarations:

```rust
pub struct Invoice {
    pub total: u64,
    pub items: u32,
}

pub fn invoice(unit_price: u32, quantity: u32) -> Invoice {
    let total = u64::from(unit_price) * u64::from(quantity);
    let invoice = Invoice {
        total,
        items: quantity,
    };
    invoice
}
```

We select only lines **8–11**, the construction statement. The earlier multiplication
is **not** part of the extraction. Checkmate does not copy or execute it to obtain `total`.
The package manifest allows Clippy's `let_and_return` style lint for this deliberately
retained binding; no lint attribute or rewrite is added to the original source.

## 2. Describe the extraction outside the source

[`metadata/snippets.toml`](metadata/snippets.toml) contains:

```toml
[snippets.invoice]
fn_sig = "fn extracted_invoice(total: u64, quantity: u32) -> Invoice"
module = "../src/lib.rs"
start = 8
end = 11
output = "invoice"
snapshot = "../snapshots/invoice.rs"
context_report = "invoice_context"
context_snapshot = "../snapshots/invoice.context.toml"
```

Omitted `context` selects automatic discovery; `context = "auto"` is equivalent.
Use `context = "explicit"` only when you intend ordinary extraction with manual context.
You choose the signature, typed inputs, inclusive range, output, guards, and tests.
Automatic discovery finds the **original** `Invoice`; no import or declaration range is
provided. The paths inside this file are relative to **`metadata/`**, not the workspace
root. `context_report` names a read-only accessor function, not a filename.

The [snippet baseline](snapshots/invoice.rs) preserves the selected source text. The
separate [version-1 context baseline](snapshots/invoice.context.toml) records normalized
signature/body/output, required input names, the original declaration and its reuse route,
and resolved mappings. Its `role = "original"` is important: this example does not copy
`Invoice` into a new nominal type.

## 3. Invoke at the integration-test root and write tests

[`tests/automatic.rs`](tests/automatic.rs) invokes:

```rust,ignore
checkmate::extract_fn!(config = "metadata/snippets.toml", snippet = "invoice");
```

The `config` path is package-relative. Automatic mode requires a literal fully qualified
macro invocation at the verified integration-test crate root—not inside a module,
`include!`, wrapper macro, or same-target unit test. Do not replace this call with a bare
imported macro name. See the [complete placement limits](../../docs/usage.md#automatic-context-opt-in).

One of the user-authored tests makes the identity check concrete:

```rust,ignore
let value: checkmate_auto_example::Invoice = extracted_invoice(12, 4);
assert_eq!((value.total, value.items), (12, 4));
```

No `Invoice` import supplies the generated signature. The qualified type in the assertion
only verifies that the result is usable as the original library type.

Other tests check that:

- `extracted_invoice(999, 0)` returns total `999`, while the unchanged original
  `invoice(7, 0)` returns total `0`. Explicit inputs are not overwritten by an initializer.
- Matching typed inputs produce the same fields as the original function for several cases.
- A separate **inline** automatic invocation returns a tuple using
  `output = (invoice.total, invoice.items)`. Inline paths are package-relative. This second
  extraction uses the snippet guard only; the configured extraction above also uses a
  context guard.

All generated items are test-only. The production library build does not compile the
integration-test macros. Native regressions separately check actual non-test emission.

## 4. Review changes explicitly

The context baseline was explicitly inspected and saved outside compilation. Inspect the
current model with the ignored user-authored printer:

```sh
cargo test -p checkmate-auto-example --test automatic print_context_for_review -- --ignored --nocapture
```

That test calls `invoice_context()` and prints the returned TOML between delimiters. It
never writes a file. With a valid guard it can show the current reviewed model; a missing
or mismatching guard prevents compilation and does not silently switch to report-only mode.

For an intentional context update:

1. Temporarily remove only `context_snapshot` from `metadata/snippets.toml`.
2. Run the inspection command. Review the extraction, required inputs, declaration roles,
   syntax, and dependency mappings. Copy only the TOML between the delimiters into
   `snapshots/invoice.context.toml` yourself.
3. Restore `context_snapshot` and run `cargo test -p checkmate-auto-example`.

Keep the snippet guard unless explicitly reviewing a snippet change too. If the source
moves, update both the configured and inline line ranges; snapshots do not relocate code.
Do not change original source merely to make an unsupported context pass analysis.

A context baseline is not a whole-program fingerprint: originals remain graph terminals,
private transitive dependencies and compiler/build semantics are not frozen, and Rust still
checks privacy/types. See the [format and review limits](../../docs/context-snapshots.md).

## Other identities and unsupported cases

- [Local-type example](../local-types/README.md): copied structs/enums have **new nominal
  identity**; inspect returned fields, but do not treat them as the original local type.
  Transparent aliases follow their targets, including original types where supported.
- [Inherent-method example](../inherent-methods/README.md): all methods on copied types,
  even unused methods, must satisfy the conservative method rules.
- [Supported matrix](../../docs/automatic-context.md): no general type/trait inference,
  initializer capture, macro expansion, arbitrary module discovery, or privacy bypass.
- [Diagnostics](../../docs/diagnostics.md): missing inputs (`CM004`), access routes (`CM006`),
  unsupported methods (`CM011`), and context drift (`CM013`) fail rather than falling back.
  Rust's own type/borrow/privacy errors remain separate.

These are public macro examples. The repository's internal analysis walkthroughs are
useful for contributors but are not additional user-facing APIs.
