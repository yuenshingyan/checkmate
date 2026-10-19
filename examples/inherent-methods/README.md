# Inherent implementations — internal walkthrough

**This is not a public automatic-extraction API.** Task 009 checks and emits internal
plans; task 011 must still integrate opt-in automatic context and verify invocation
placement. The example crate itself has no Checkmate dependency.

[`src/lib.rs`](src/lib.rs) is ordinary, unannotated Rust. Its function-local `Counter`
has a constructor, a mutable borrowed helper, and an owned `finish` method. The original
function constructs a counter and returns its incremented value.

[`checkmate.toml`](checkmate.toml) selects only:

```rust
    let counter = Counter::new(n);
```

The supplied signature is `fn finish_counter(n: u8) -> u8`, and the output expression is
`counter.finish()`. No declaration or implementation range is supplied. The internal
planner discovers the type and its entire inherent implementation, checks all method
signatures/bodies, and rewrites their resolved type references. `Self::increment(&mut
self)` remains a qualified borrowed-method call; `finish` uses a proven owned receiver.

From the repository root:

```sh
cargo test -p checkmate --lib walkthroughs::inherent_implementations -- --nocapture
cargo test -p checkmate --lib resolve::promote::tests::implementations
cargo test -p checkmate-inherent-example
```

The walkthrough prints the test-gated plan and verifies the reviewed
[snapshot](snapshots/counter.rs) and unchanged original source. A compiler-backed test
loads this same source, compiles its generated function, and compares its results to
the original for inputs 0, 7, and 254. The example package test tests only the original
function; it does not call an unpublished macro mode.

## Deliberate limits

- Copied structs/enums still have **new nominal identity**; retaining methods does not
  make them interchangeable with the original function-local types.
- Every method of a required copied type is checked, including unused methods. Its
  dependencies join the graph as behavior dependencies, not storage-layout edges.
- Non-generic inherent constructors and qualified calls such as `Counter::read(&value)`
  are supported. Receiver/argument compatibility and privacy remain rustc checks.
- Dot calls require a proven **owned local nominal value** and an inherent **by-value
  `self`** method at Rust's first receiver candidate. Source calls must retain the same
  proven receiver identity after extraction. Proof uses explicit types, direct
  constructors/constructor return declarations, or simple move aliases—not general
  type inference or initializer execution.
- Borrowed/dereferenced receivers and automatic borrowing of `&self`/`&mut self` methods
  are rejected: traits can win lookup before those inherent methods. Use an explicit
  qualified call where the original source already has one; the tool does not alter
  the original source or silently convert dot calls to qualified calls.
- A bare tuple constructor or tuple variant is a function item, **not** a constructed
  value. Its trait methods cannot be approved as the result type's inherent methods.
- Trait impls, derives, macros, generic/const/async/unsafe/ABI methods, dependent lifetime
  context, unsupported attributes, nested method-body items/imports, and selected/output
  impl declarations fail conservatively. Alias normalization and arbitrary receiver
  inference remain unsupported. Whole-file indexing may reject unrelated unsupported
  impls too.

See [task 009](../../docs/tasks/009-implementations.md) and the
[safety contract](../../docs/automatic-context.md).
