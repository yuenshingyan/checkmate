# Function-local type promotion

Task 011 adds public automatic extraction in [`tests/automatic.rs`](tests/automatic.rs),
using [`automatic.toml`](automatic.toml). It discovers `User` without a manual context
import or declaration range. The original [`checkmate.toml`](checkmate.toml), including
its historical comments, remains unchanged as the task-008 internal walkthrough input.

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
plan. The second tests both the unchanged original function and public automatic
extraction with field inspection and a comparison against the original tuple result.

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

Unsupported generic locals, captured context, unproven implementation-dependent operations,
restricted visibility, semantic/doc-macro attributes, and `use` declarations inside
selected code/output fail explicitly. Surrounding import aliases remain supported.
Known name collisions fail rather than silently capturing another binding. Generated
names are internal details, not a stable public naming API.

Task 009 adds a separate [inherent-methods walkthrough](../inherent-methods/README.md)
for the conservative supported implementation subset.

## Reviewed context baseline

Task 012 adds [`context-snapshots.toml`](context-snapshots.toml),
[`tests/context_snapshot.rs`](tests/context_snapshot.rs), and the reviewed
[`snapshots/user.context.toml`](snapshots/user.context.toml). They reuse the unchanged
source and snippet baseline above. The new guard records `User`'s declaration and
resolved dependencies, explicitly labeled as a copied struct with new nominal identity.

```sh
cargo test -p checkmate-local-example --test context_snapshot
cargo test -p checkmate-local-example --test context_snapshot print_context_for_review -- --ignored --nocapture
```

The first checks the reviewed context and returned fields. The second prints the model
through the generated `user_context()` accessor; neither command writes a baseline.
For an intentional update, explicitly omit `context_snapshot`, inspect/copy the new
TOML, then restore the guard. Missing/mismatching baselines fail rather than auto-update.
See the [review workflow and format limits](../../docs/context-snapshots.md).

See [task 008](../../docs/tasks/008-local-types.md), the
[safety contract](../../docs/automatic-context.md), and
[coverage/property checks](../../docs/coverage.md).
