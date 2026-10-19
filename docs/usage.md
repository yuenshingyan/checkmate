# Usage guide

[Project overview](../README.md) · [Architecture](architecture.md) · [Milestone and deferred work](tasks/README.md)

Turn an unmodified range of Rust source lines into a test-only function. No markers,
attributes, or wrappers need to be added to the original snippet.

Start with the [complete automatic workflow](../examples/automatic-context/README.md),
or see the [example index](../examples/README.md) and
[struct-return walkthrough](../examples/struct-return/README.md). The latter separately
distinguishes ordinary extraction, public automatic mode, and internal analysis walkthroughs.

The `crates/checkmate` workspace member provides the `extract_fn!` procedural macro.
The `checkmate-example` package in `examples/basic` demonstrates its use. Add
`checkmate` as a `dev-dependency` and invoke it inside a test module or integration
test. The bundled example uses this relative dependency path (adjust for your package):

```toml
[dev-dependencies]
checkmate = { path = "../../crates/checkmate" }
```

```rust
use checkmate::extract_fn;

extract_fn! {
    fn_sig = "fn add_if(a: f32, b: f32) -> f32",
    module = "src/main.rs",
    start = 7,
    end = 11,
    output = c,
}

#[test]
fn adds_nonnegative_inputs() {
    assert_eq!(add_if(2.0, 2.0), 4.0);
}
```

For a unit-test source file, gate the entire file with `#![cfg(test)]`, as in
[`examples/basic/src/tests.rs`](../examples/basic/src/tests.rs). Integration tests
under a package's `tests/` directory are already test-only.

## Choose a workflow

- **Ordinary extraction (omit `context`)**: supply any required context in the test
  scope yourself. Use this for standalone statement fixtures, binary sources, or
  context outside automatic discovery's supported subset.
- **Automatic context (`context = "auto"`)**: discover supported declarations from a
  verified library root, invoked literally at an integration-test crate root. This
  saves manual declaration plumbing, not test design or input typing.

In both modes you choose the range, explicit typed inputs, output, and assertions.
Discovery does not run earlier initializers, capture runtime values, infer a full
signature, or generate tests. All shell commands below run from the **repository
root**; macro/config path bases are specified separately.

## Inline options

- `fn_sig`: function signature as a string. An optional semicolon or empty/comment-only
  placeholder body is also accepted. Supply the snippet's inputs and their types here.
- `module`: source-file path, relative to the **invoking package's `Cargo.toml`**.
- `start`, `end`: **one-based, inclusive** line numbers. Select complete Rust
  statements or an expression, not part of a statement.
- `output` (optional): a Rust expression appended to the selected statements. Use
  `output = c` to return a local variable, or an expression such as `(c, c >= 0)`.
  Omit it if the snippet already ends with its return expression or returns `()`.
- `snapshot`: omission guards with `snapshots/<generated-function-name>.rs`, derived
  from the parsed `fn_sig`. `snapshot = "path"` overrides that default;
  `snapshot = false` explicitly disables the snippet guard. `true` and an empty string
  are invalid. Inline paths (including the default) are relative to the invoking
  package's `Cargo.toml`. Missing files and mismatches fail compilation.
- `context` (optional): only `"auto"` is accepted. Omission keeps ordinary extraction.
  See the placement and supported-context restrictions below.
- `context_snapshot` (optional, automatic mode only): path to a reviewed, versioned
  context baseline. Like `snapshot`, inline paths are package-relative.
- `context_report` (optional, automatic mode only): a Rust identifier in a string,
  naming a test-only accessor returning the full TOML model for explicit inspection.
  Prefer a distinct snake_case name; see [context snapshots](context-snapshots.md).

The example generates the equivalent of:

```rust
#[cfg(test)]
fn add_if(a: f32, b: f32) -> f32 {
    let c = if a >= 0.0 {
        a + b
    } else {
        a
    };
    c
}
```

The generated function exists in the scope where the macro is invoked. Bring any
needed types, functions, or imports into that scope in ordinary mode; the macro does not capture the
original surrounding locals or infer types. It tests an extracted copy, not the
execution of the original containing function. Context-sensitive macros such as
`file!()` and `line!()` refer to the expansion context.

## External definitions

Keep extraction metadata in a TOML file rather than in test code. For example,
`checkmate.toml` at the package root:

```toml
[snippets.add_if]
fn_sig = "fn add_if(a: f32, b: f32) -> f32"
module = "src/main.rs"
start = 7
end = 11
output = "c"
snapshot = "snapshots/add_if.rs"
```

Then invoke it in your test module:

```rust
use checkmate::extract_fn;

extract_fn!(config = "checkmate.toml", snippet = "add_if");
```

Each `[snippets.NAME]` defines one extraction; a file can contain multiple definitions.
`output` is a Rust expression **inside a TOML string**. `output`, `snapshot`,
`context`, `context_snapshot`, and `context_report` are optional; omitting `snapshot`
still enables the default snippet guard. Set `context = "auto"` in the definition, not alongside
`config`/`snippet` in the invocation. Unknown keys, missing required fields, and missing snippet names are errors.
Do not mix `config`/`snippet` with inline options.

The `config` path is relative to the invoking package's `Cargo.toml`. Paths **inside
that file** (`module`, `snapshot`, and `context_snapshot`) are relative to the configuration file's own
directory, so definitions can live in a subdirectory. The default snippet path uses
that same configuration directory, not the package root. See
[`examples/basic/checkmate.toml`](../examples/basic/checkmate.toml) and
[`examples/basic/src/tests.rs`](../examples/basic/src/tests.rs) for a working example.
The example's source/config/snapshot paths are relative to its package, not the
workspace root. Users still write their own tests.

## Automatic context (opt-in)

Requires Rust 1.88+ and a normal Cargo build. Editor expansion additionally needs the
[compatible rust-analyzer/server setup](editor-support.md); its legacy macro protocol
does not provide the physical caller location required by automatic mode. The runnable
[automatic-context example](../examples/automatic-context/tests/automatic.rs) uses a
literal qualified call at the root of `examples/automatic-context/tests/automatic.rs`:

```rust,ignore
checkmate::extract_fn!(config = "metadata/snippets.toml", snippet = "invoice");
```

This fragment is exercised in the workspace example, not as a standalone doctest.
The config path is relative to that package's manifest. Its
[`metadata/snippets.toml`](../examples/automatic-context/metadata/snippets.toml) contains:

```toml
[snippets.invoice]
context = "auto"
fn_sig = "fn extracted_invoice(total: u64, quantity: u32) -> Invoice"
module = "../src/lib.rs"
start = 8
end = 11
output = "invoice"
snapshot = "../snapshots/invoice.rs"
context_report = "invoice_context"
context_snapshot = "../snapshots/invoice.context.toml"
```

These three file paths are **config-directory-relative**, not repository-relative.
The selected constructor in [`src/lib.rs`](../examples/automatic-context/src/lib.rs)
is lines 8–11; the earlier `total = u64::from(unit_price) * u64::from(quantity)`
is not selected or executed. `Invoice { total: u64, items: u32 }` is an accessible
original library type: a test assigns the result to `checkmate_auto_example::Invoice`.
`extracted_invoice(999, 0)` retains total `999`, whereas original `invoice(7, 0)`
computes `0`. Other tests compare original execution with matching explicit inputs.
The same integration test has an inline `invoice_fields(total: u64, quantity: u32)
-> (u64, u32)` extraction with `output = (invoice.total, invoice.items)`.

```sh
cargo test -p checkmate-auto-example
```

This package has no binary; use tests, not `cargo run`. For **copied** function-local
nominal types instead, see [local types](../examples/local-types/README.md).
The [struct-return](../examples/struct-return/README.md) and
[inherent-methods](../examples/inherent-methods/README.md) examples cover more cases.

The public boundary is intentionally narrower than the internal resolver:

- The source must be the invoking package's **verified library root**, conventional or
  explicitly declared in Cargo, with an explicit edition of 2018, 2021, or 2024.
  Workspace-inherited editions are not currently proven. Arbitrary module/fixture
  files and binary roots fail.
- The caller must be the unique Cargo integration-test root, matched using compiler
  file/position/package/crate metadata and the manifest/target layout. Conventional
  tests and explicit `[[test]]` paths are supported. Same-target unit-test modules,
  nested modules, `include!`/macro-generated invocations, missing metadata, and ambiguous
  target identities fail. Names beginning `build_script_` are reserved/rejected because
  Cargo custom-build targets do not carry binary-target metadata. Checkmate does not treat a user-supplied placement assertion
  or a matching filename alone as proof.
- Use a fully qualified Checkmate dependency alias, declared directly in the package's
  dependencies/dev-dependencies. Bare imported macro names, workspace-inherited or
  target-specific-only macro aliases are not currently proven.
- Caller-root globs, `extern crate`, opaque root macros, namespace collisions, and
  unrecognized caller attributes are rejected rather than assuming generated names
  are safe. Nested module contents are not read for parent bindings. Rust remains
  authoritative for final availability, member privacy, and typing.
- You still supply the signature, typed inputs, selection, output, and tests. Signature
  type names are interpreted in the selected source context. Earlier initializers are
  **never copied or executed**. No type inference or general trait resolution is added.
- Accessible original declarations keep their identity/privacy. Supported function-local
  structs/enums are copied with **new nominal identity**; transparent aliases follow their
  rewritten targets. The generated function documents this change. Do not depend on
  generated helper names or use them to claim interoperability with original locals.
- All required methods of copied types are checked, including unused methods. See the
  [supported subset](automatic-context.md) and [method limits](tasks/009-implementations.md).
  Unsupported context always errors; there is no fallback to ordinary extraction.

All emitted functions, types, impls, imports, and dependency markers are test-only.
Automatic reports use [bounded structured diagnostics](diagnostics.md), anchored at
an actual invocation span. Compiler type/borrow/privacy errors are not remapped.
Source, caller, Cargo manifest, configuration, and enabled snapshot contents read by
analysis are tracked through dependency markers. No input file is modified.

Existing `snapshot` guards still protect selected source text only. Add
`context_snapshot` to review the discovered declarations/dependency mappings and the
normalized signature/body/output. `context_report` supplies a read-only accessor for
explicit inspection and manual baseline creation. See the
[format, workflow, and terminal-graph limits](context-snapshots.md); a matching baseline
is not proof of whole-program semantic equivalence.

### Supported and deferred context

| Context | Supported boundary / deferred behavior |
| --- | --- |
| Selection and inputs | Complete statements/tail expression in one block; surrounding values require explicit typed parameters. No initializer execution or full inference. |
| Accessible same-file originals | Reuse proven paths for structs/enums/aliases/functions/constants; preserve identity and privacy, never copy private originals to bypass access. |
| Function-local declarations | Copy self-contained non-generic structs/enums with **new nominal identity**; aliases remain transparent to rewritten targets. Generic captures and dependent const analysis are deferred. |
| Imports and paths | Resolve known explicit imports/aliases and inline modules. Required globs, ambiguous routes, and out-of-line dependency discovery are unsupported. |
| Standard/external paths | Only known unshadowed prelude mappings are proven. Supporting `Box`/`Vec` types does not imply support for `Box::new`/`Vec::new` discovery. Matching nonstandard Cargo aliases alone does not prove cross-target dependency identity. |
| Inherent methods | Check all methods of required copied types, even unused ones. Only the conservative non-generic subset is supported; dot calls require proven owned nominal receivers and by-value `self`. Qualified calls can support borrowed methods without inferred dispatch. |
| Traits and compiler integration | Trait/blanket impl copying, general method lookup, autoref/autoderef inference, and full compiler integration remain deferred. Bounded rust-analyzer expansion is available with the [tested editor setup](editor-support.md), not full semantic integration. |
| Macros and attributes | Macros in moved snippets/copied declarations, derives on copied types, conditional compilation affecting required context, and unproven semantic attributes are rejected; literal documentation is supported. Reused original behavior remains in its original crate. |

See the [safety contract and detailed matrix](automatic-context.md),
[method limits](tasks/009-implementations.md), and [workflow regression evidence](regression-tests.md).
Whole-file indexing can conservatively reject unsupported syntax outside the selected
range. Completion of the public workflow does not imply full Rust inference support.

### Review context explicitly

The optional `context_snapshot` is versioned TOML (`format = "checkmate-context"`,
`version = 1`), separate from the source-text `snapshot`. It has no default path and
is automatic-only. To use a context guard without a snippet guard, explicitly set
`snapshot = false`; omitting `snapshot` enables the default. `context_report` is also optional: `invoice_context()` only returns
a TOML string literal; it does not execute the extraction, print, or write anything.
The example's ignored user-authored test prints it explicitly:

```sh
cargo test -p checkmate-auto-example --test automatic print_context_for_review -- --ignored --nocapture
```

For a new baseline, first enable the report without a context guard. For an intentional
update to a failing guard, explicitly remove only `context_snapshot` from the extraction
metadata temporarily; keep the source-text guard. Run the print test, review the model
and differences, and manually copy **only the TOML** (not markers or test-harness output)
to `examples/automatic-context/snapshots/invoice.context.toml`. Restore the guard, rerun
tests, and commit the reviewed metadata/baseline together. Do not edit original source
to satisfy discovery or auto-accept a mismatch. Compilation never writes or approves a
baseline, and a failing guard does not fall back to report-only mode.

The model covers the approved discovery graph, not whole-program semantics: reused
originals are terminals, private/transitive dependencies are not recursively frozen,
and external/toolchain/runtime behavior is not fingerprinted. See
[context snapshots](context-snapshots.md) for schema, stable identity, and review limits.

## Detecting line drift with snapshots

A snapshot is a persistent, reviewed copy of the selected snippet, stored outside
the original source. Unlike compiler-generated functions or build artifacts, it
survives clean builds and can be committed to version control. **Both ordinary and
automatic extraction are snippet-guarded by default.**

Omitting `snapshot` selects `snapshots/<generated-function-name>.rs` from the valid,
parsed `fn_sig`, independent of the TOML snippet key, source `module`, and line range.
`fn r#type()` uses `snapshots/type.rs`; Unicode identifiers are retained. The signature
must parse before loading this default guard; this does not specify the ordering of
all source/range validation. Inline defaults are package-relative and TOML defaults
are configuration-directory-relative, just like explicit paths. Same function names
within one base share a default path: choose explicit distinct paths for different bodies.

### Migration from unchecked omission

Previously, omitting `snapshot` left the snippet unchecked. Now either manually create
and review the default baseline, retain an existing explicit custom path, or set
`snapshot = false` to intentionally keep unchecked extraction. `snapshot = "path"`
always wins; `snapshot = true` and `snapshot = ""` are errors. Do not edit original
source to satisfy a guard or discovery limitation.

1. Select the range and copy those exact lines, retaining indentation, into a
   snapshot file. For the example, `snapshots/add_if.rs` contains:

   ```text
       let c = if a >= 0.0 {
           a + b
       } else {
           a
       };
   ```

2. Use the default path or set an explicit `snapshot` path and commit the reviewed file.
3. Run tests. Checkmate compares the current selection to the saved snippet **before
   parsing it**. A difference fails compilation with the source range, snapshot path,
   and first differing line, including expected and actual text.
4. If lines moved, fix `start`/`end`; the snapshot need not change. If you intentionally
   changed the snippet, review the change and explicitly replace the snapshot.

A missing default file fails with its exact resolved path. Compilation never creates,
writes, updates, approves, or prints baseline files—not even when missing. Missing or
mismatching baselines never fall back to unchecked extraction.
Snapshots normalize LF/CRLF line endings and allow a final newline; other whitespace
and comments are significant. The guard compares the snippet, not the wrapper's
signature or `output`. It cannot detect a move to a different but identical snippet,
and it does not automatically relocate code or fingerprint the whole program. With
`snapshot = false` and no `context_snapshot`, extraction is unchecked for drift. A context guard alone compares normalized syntax, not ordinary
comments/formatting. No markers or annotations are inserted into the original source.

## Diagnostics and rebuilding

Extraction errors include the original file path and requested line range. Syntax
errors also show a bounded, numbered preview of the selection and guidance. Errors
from configured extractions identify the TOML file and snippet name. Snapshot errors
show the first mismatch, making stale ranges easier to identify.

Rust still checks the generated function's types normally. Type/borrow errors may
point at the macro invocation rather than the original source; these diagnostics
are produced by rustc, not remapped by Checkmate.

Emitted `include_str!` dependencies track the source, configuration, and snapshot,
so changes to any of them trigger recompilation, even for source files that are not
otherwise part of a Rust module.

### Practical automatic-mode troubleshooting

| Diagnostic | What to do without editing original source |
| --- | --- |
| `CM003` unproven placement/source | Use a verified library root and a literal qualified macro invocation at the unique package integration-test root. Check manifest edition, target paths/names, dependency alias, and caller namespace restrictions. Do not add annotations or wrappers to the source to assert placement. |
| `CM004` missing input | Add the known value as an explicitly typed `fn_sig` parameter and pass it from your test. Do not copy/run its earlier initializer or guess its type. |
| `CM006` inaccessible original | Check the reported source route and dependency chain. Use only an already accessible, proven original route; never widen visibility or substitute a copied private original. If no legal route exists, test through the public API or use ordinary extraction with legally accessible test context. |
| `CM011` unproven method/trait resolution | Check receiver ownership and the conservative inherent-method limits. Do not assume Rust's general trait/autoderef inference is implemented. Choose a supported selection or ordinary extraction with explicit test context; discovery does not rewrite calls to force dispatch. |
| `CM013` context guard/report problem | Check path base, version/schema, stable identity, and accessor-name collisions. For intentional drift, use the explicit report/review/restore flow above, never automatic acceptance. Ambiguous identities may require omitting the optional context guard, not renaming original declarations. |

See [diagnostic categories and bounded reports](diagnostics.md). Rust remains
responsible for final member privacy, typing, and borrowing; changing mode cannot
bypass those checks. There is no silent fallback or source-editing remedy.

## Using existing testing tools

Checkmate only generates functions; test cases remain yours to define. Returning
multiple values needs no new feature: use a tuple return type and `output = (a, b)`
(or `output = "(a, b)"` in TOML).

Property-based tools such as `proptest` can call the generated function just like any
other function. They generate inputs and check properties you specify; Checkmate
does not add a dependency on them or invent test cases.

## Run

```sh
cargo test --workspace
cargo run -p checkmate-example
```
