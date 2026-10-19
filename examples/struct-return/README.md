# Struct-return and context-analysis examples

[All examples](../README.md) · [Usage guide](../../docs/usage.md)

This package supplies one ordinary Rust source file for two distinct demonstrations:

1. **Public API:** extract and test snippets today, with explicit imports.
2. **Internal foundations:** inspect what tasks 004–006 can discover, without claiming
   the public macro automatically discovers context yet.

## Source remains ordinary Rust

[`src/lib.rs`](src/lib.rs) defines a public `User` struct and an alias named `Account`.
Inside `demo()`, the selected statements are:

```rust
let user = Account {
    name,
    active,
};
```

There are no Checkmate markers, attributes, or macro calls in the source file.
The containing function still supplies its own `"Alice"` and `true` inputs.

## Public API: return the original struct

[`checkmate.toml`](checkmate.toml) selects those lines and supplies:

```toml
[snippets.build_user]
fn_sig = "fn build_user(name: String, active: bool) -> Account"
module = "src/lib.rs"
start = 17
end = 20
output = "user"
snapshot = "snapshots/user.rs"
```

The integration test in [`tests/extraction.rs`](tests/extraction.rs) imports the type
explicitly and checks that the result has the **original library type**, not a copy:

```rust
use checkmate::extract_fn;
use checkmate_struct_example::models::User as Account;

extract_fn!(config = "checkmate.toml", snippet = "build_user");

#[test]
fn extracted_function_returns_the_original_struct_type() {
    let user: checkmate_struct_example::models::User = build_user("Bob".into(), false);
    assert_eq!(user.name, "Bob");
    assert!(!user.active);
}
```

Run from the repository root:

```sh
cargo test -p checkmate-struct-example
```

The package also demonstrates:

- **Multiple outputs:** `user_fields` extracts the same statements, then returns
  `(user.name, user.active)` as `(String, bool)`.
- **Snapshots:** both extractions check the same reviewed [`snapshots/user.rs`](snapshots/user.rs).
  Source drift fails compilation; compilation never writes the baseline.
- **Methods with ordinary extraction:** `count_values` extracts `let count = values.len();`.
  With the supplied `&[i32]` input type, rustc resolves `.len()` normally.
- **Unchanged original execution:** a separate test verifies that `demo()` still creates
  Alice, independently of the test function's Bob/Carol inputs.

The example manifest permits Clippy's `let_and_return` lint solely because these
named bindings deliberately illustrate `output`. No lint annotations are inserted
into the selected source.

## Internal analysis walkthroughs

Run these **repository-only unit tests**, not a new public Checkmate API:

```sh
cargo test -p checkmate --lib walkthroughs -- --nocapture --test-threads=1
```

The driver is [`crates/checkmate/src/walkthroughs.rs`](../../crates/checkmate/src/walkthroughs.rs).
It loads this package's TOML rather than maintaining a second set of snippet ranges.

### 1. Import origins and aliases

The source contains ordinary imports/re-exports:

```rust
use crate::models::User as Account;

pub mod directory {
    pub use super::models::{self as schema, User as Member};
}
```

The walkthrough prints:

```text
Account -> crate::models::User (source origin; access not inferred)
Member -> crate::models::User (source origin; access not inferred)
schema -> crate::models (source origin; access not inferred)
```

This records where each alias originates and preserves `super`'s meaning. It does
**not** prove that a target is accessible from an arbitrary test module. Source-root
identity is checked against this package's `Cargo.toml`, not guessed from a filename.

### 2. References versus inputs versus locals

For `build_user`, the analysis prints these classifications:

```text
Required inputs: active, name
Signature: Account -> import candidate
Source: Account -> import candidate
Source: name -> explicit input
Source: active -> explicit input
Output: user -> snippet local
```

The `name` and `active` shorthand initializers are value references; field labels
are not separate inputs. The original `String::from("Alice")` initializer is not
copied or executed to obtain `name`. Unknown types/paths outside the indexed context
remain deferred for the later resolution tasks.

### 3. Missing-input diagnostic

A walkthrough intentionally analyzes a signature without `name`:

```rust
fn build_user(active: bool) -> Account
```

It asserts and prints a diagnostic containing:

```text
src/lib.rs:17-20: source 18:9: required input `name` is not supplied by fn_sig
```

The message recommends a typed parameter, rather than silently copying an initializer.
This is an expected-error test; the committed configuration remains valid.

### 4. Output shadowing

Another walkthrough uses the tuple-returning signature with this output expression:

```rust
{ let active = false; (user.name, active) }
```

Its output distinguishes the bindings:

```text
active in the snippet -> explicit input
active in the output block -> output-local binding, not another input
```

The original input is still needed to construct `user`. The output block's local
`active` is a different binding, not an additional input requirement.

### 5. Methods remain deferred in automatic analysis

For the `count_values` snippet, the internal collector prints:

```text
Required input: values (not len)
Deferred: method `len` requires method/trait resolution
```

This does not contradict the passing ordinary extraction test. Ordinary extraction
lets rustc check the supplied function directly; future automatic context discovery
must determine which additional implementations/dependencies to bring along. The
collector deliberately does not pretend to have solved method or trait resolution.

### 6. Reusable original declarations

Task 006 resolves both `Account` references (signature and construction) to the same
original declaration and prints a generated import:

```rust
use ::checkmate_struct_example::models::User as Account;
```

The resolver is explicitly told this is a same-package integration-test context.
It uses the verified library name, does not copy `User`, and leaves final field/type
privacy checks to Rust. Separate compiler-backed unit tests prove the imported type
is the original type and inaccessible items/fields still fail. Task 011 must establish
actual invocation placement before enabling this in `extract_fn!`; the public example
continues to import `Account` explicitly.

## What is not demonstrated yet

Automatic imports in `extract_fn!`, local-type promotion, complete dependency discovery,
and `context = "auto"` are not public features yet. This walkthrough does not complete
[task 014](../../docs/tasks/014-documentation.md), which documents the eventual integrated
feature. The [backlog](../../docs/tasks/README.md) tracks the remaining implementation.
