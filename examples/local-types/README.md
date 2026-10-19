# Function-local type promotion — internal walkthrough

**Not a public `extract_fn!` example yet.** Task 008 builds and tests promotion plans
internally; task 011 still needs to verify invocation placement and expose automatic
context. The configuration here is consumed by the repository's walkthrough test.
The public macro cannot resolve this function-local `User` automatically today.

[`src/lib.rs`](src/lib.rs) is ordinary Rust with no Checkmate markers or annotations:

```rust
pub fn original(name: String, active: bool) -> (String, bool) {
    struct User {
        name: String,
        active: bool,
    }
    let user = User {
        name,
        active,
    };
    (user.name, user.active)
}
```

[`checkmate.toml`](checkmate.toml) selects only the four construction lines, supplies
`fn build_user(name: String, active: bool) -> User`, and uses `user` as the output.
No manual declaration range is supplied. The reviewed [snapshot](snapshots/user.rs)
is checked read-only by the walkthrough.

From the repository root:

```sh
cargo test -p checkmate --lib walkthroughs::local_type_promotion -- --nocapture
cargo test -p checkmate-local-example
```

The first command discovers `User`, plans a uniquely named copy next to the generated
function, rewrites the signature/construction/field types, and prints the test-gated
plan. The second tests the original library function's unchanged tuple-return behavior;
it does not invoke an unpublished automatic-extraction API.

With the generated plan, the caller can inspect fields without naming the generated
type explicitly:

```rust,ignore
let user = build_user("Bob".into(), false);
assert_eq!(user.name, "Bob");
assert!(!user.active);
```

**The copied struct has new nominal identity.** It is not the original function-local
`User`. Compiler-backed tests under `crates/checkmate/src/resolve/promote/tests.rs`
check this with runtime `TypeId` comparisons, inspect returned fields, and compile two
extractions together without name collisions. Transparent aliases follow their rewritten
targets; an alias does not itself create a nominal type.

Other tests cover nested/shadowed types, recursive types, enums, const/type arguments,
output scopes, shorthand fields/patterns, raw/Unicode names, visibility, and generated
items disappearing without `cfg(test)`. Source definitions are never edited. Definitions
selected inside a snippet are lifted only out of its generated copy, avoiding a second
nominal definition inside the generated function.

Unsupported generic locals, captured context, implementation-dependent operations,
restricted visibility, semantic/doc-macro attributes, and `use` declarations inside
selected code/output fail explicitly. Surrounding import aliases remain supported.
Known name collisions fail rather than silently capturing another binding. Generated
names are internal details, not a stable public naming API.

See [task 008](../../docs/tasks/008-local-types.md), the
[safety contract](../../docs/automatic-context.md), and
[coverage/property checks](../../docs/coverage.md).
