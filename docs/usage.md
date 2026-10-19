# Usage guide

[Project overview](../README.md) · [Architecture](architecture.md) · [Planned tasks](tasks/README.md)

Turn an unmodified range of Rust source lines into a test-only function. No markers,
attributes, or wrappers need to be added to the original snippet.

For runnable demonstrations, see the [example index](../examples/README.md) and
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

## Inline options

- `fn_sig`: function signature as a string. An optional semicolon or empty/comment-only
  placeholder body is also accepted. Supply the snippet's inputs and their types here.
- `module`: source-file path, relative to the **invoking package's `Cargo.toml`**.
- `start`, `end`: **one-based, inclusive** line numbers. Select complete Rust
  statements or an expression, not part of a statement.
- `output` (optional): a Rust expression appended to the selected statements. Use
  `output = c` to return a local variable, or an expression such as `(c, c >= 0)`.
  Omit it if the snippet already ends with its return expression or returns `()`.
- `snapshot` (optional): path to a reviewed copy of the selected source lines,
  relative to the invoking package's `Cargo.toml`. A mismatch fails compilation.
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
`context`, `context_snapshot`, and `context_report` are optional. Set `context = "auto"` in the definition, not alongside
`config`/`snippet` in the invocation. Unknown keys, missing required fields, and missing snippet names are errors.
Do not mix `config`/`snippet` with inline options.

The `config` path is relative to the invoking package's `Cargo.toml`. Paths **inside
that file** (`module`, `snapshot`, and `context_snapshot`) are relative to the configuration file's own
directory, so definitions can live in a subdirectory. See
[`examples/basic/checkmate.toml`](../examples/basic/checkmate.toml) and
[`examples/basic/src/tests.rs`](../examples/basic/src/tests.rs) for a working example.
The example's source/config/snapshot paths are relative to its package, not the
workspace root. Users still write their own tests.

## Automatic context (opt-in)

Requires Rust 1.88+ and a normal Cargo build. In a **crate-root integration-test file**
(e.g. `tests/automatic.rs`), invoke the macro with its literal, fully qualified path:

```rust,ignore
checkmate::extract_fn! {
    fn_sig = "fn automatic_user(name: String, active: bool) -> Account",
    module = "src/lib.rs",
    start = 17,
    end = 20,
    output = user,
    snapshot = "snapshots/user.rs",
    context = "auto",
}
```

This is the runnable [struct example](../examples/struct-return/tests/automatic.rs).
It returns the original library `models::User` without importing `Account` manually.
For TOML, add `context = "auto"` to a snippet definition and use
`checkmate::extract_fn!(config = "automatic.toml", snippet = "build_user");` at the
same verified location. See [local types](../examples/local-types/README.md) and
[inherent methods](../examples/inherent-methods/README.md).

The initial public boundary is intentionally narrower than the internal resolver:

- The source must be the invoking package's **verified library root**, conventional or
  explicitly declared in Cargo. Arbitrary module/fixture files and binary roots fail.
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
Source, caller, Cargo manifest, configuration, and optional snapshot contents read by
analysis are tracked through dependency markers. No input file is modified.

Existing `snapshot` guards still protect selected source text only. Add
`context_snapshot` to review the discovered declarations/dependency mappings and the
normalized signature/body/output. `context_report` supplies a read-only accessor for
explicit inspection and manual baseline creation. See the
[format, workflow, and terminal-graph limits](context-snapshots.md); a matching baseline
is not proof of whole-program semantic equivalence.

## Detecting line drift with snapshots

A snapshot is a persistent, reviewed copy of the selected snippet, stored outside
the original source. Unlike compiler-generated functions or build artifacts, it
survives clean builds and can be committed to version control.

1. Select the range and copy those exact lines, retaining indentation, into a
   snapshot file. For the example, `snapshots/add_if.rs` contains:

   ```text
       let c = if a >= 0.0 {
           a + b
       } else {
           a
       };
   ```

2. Set `snapshot` in the inline invocation or TOML definition and commit the snapshot.
3. Run tests. Checkmate compares the current selection to the saved snippet **before
   parsing it**. A difference fails compilation with the source range, snapshot path,
   and first differing line, including expected and actual text.
4. If lines moved, fix `start`/`end`; the snapshot need not change. If you intentionally
   changed the snippet, review the change and explicitly replace the snapshot.

No snapshot is created or updated during compilation—not even when missing.
Snapshots normalize LF/CRLF line endings and allow a final newline; other whitespace
and comments are significant. The guard compares the snippet, not the wrapper's
signature or `output`. It cannot detect a move to a different but identical snippet,
and it does not automatically relocate code. Without `snapshot`, extraction remains
unchecked for drift. No markers or annotations are inserted into the original source.

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
