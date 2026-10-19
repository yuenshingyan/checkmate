# Automatic context: supported subset and safety contract

**Status:** the 14-task milestone is implemented, documented, and tested on stacked
branches. Public placement remains limited to verified library roots and package
integration-test roots. See [usage and restrictions](usage.md#automatic-context-opt-in),
the [complete example](../examples/automatic-context/README.md), and the
[regression matrix](regression-tests.md). Milestone completion does not imply full
Rust/compiler inference or a merge into `master`.

## Public boundary

The opt-in is `context = "auto"`, accepted both inline and as a string in a
TOML snippet definition. Omission keeps today's extraction behavior. No other
context value is initially accepted. Do not mix inline fields into a config invocation.

```rust,ignore
// At the root of a verified Cargo integration test.
checkmate::extract_fn! {
    fn_sig = "fn build_user(name: String) -> User",
    module = "src/lib.rs",
    start = 12,
    end = 15,
    output = user,
    context = "auto",
}
```

Users still supply the source range, complete function signature and input types,
and optional output expression. They still author all tests. Automatic discovery
only supplies required declaration context. Ordinary extraction remains capable of
reading standalone statement fixtures that are not complete Rust files; automatic
mode requires a parseable source file and a structurally valid selection.

## Invariants

1. Never write the original source, configuration, or baseline during compilation.
2. Never widen original visibility or use unsafe access to bypass privacy.
3. Never run earlier statements, capture their current values, or invent inputs.
4. Resolve identifiers by scope, namespace, and declaration identity, not spelling alone.
5. Preserve original type identity when reusing an accessible original declaration.
6. Copied function-local structs/enums have new nominal identity; aliases follow
   their rewritten targets and do not themselves create nominal types. Do not substitute copies for an
   inaccessible module-level type or claim equivalence with the original type.
7. Reject ambiguous or unsupported dependencies with a diagnostic. No silent fallback
   to a different extraction or partial dependency plan.
8. Deterministic discovery and emission: no random generated names, hash-map iteration
   order, absolute checkout paths in stable identities, or dependency traversal races.
9. Successful compilation is necessary, not proof of semantic equivalence. Test the
   transformations and their runtime behavior independently.

## Supported/unsupported matrix for the first milestone

These are the analysis limits. Public invocation placement is narrower than the
internal `SameTarget` resolver API and is checked separately; see usage above.

| Construct | Policy | Reason / boundary |
| --- | --- | --- |
| Contiguous complete statements or tail expression in one block | Supported | There is a single lexical environment and execution order. |
| Partial statements, selected function/module delimiters, unrelated blocks | Reject | A line range must not silently expand or change execution context. |
| Local bindings declared in the selection | Supported | Preserve their patterns and lexical scopes. |
| Surrounding locals and function parameters | Require matching explicit generated-function parameters | Their values cannot be reconstructed without executing surrounding code. |
| Same-file module-level struct, enum, alias, function, or constant with a known accessible path | Reuse original | Retains identity/implementation; rustc still enforces visibility. |
| Inaccessible or ambiguously mapped original item | Reject | Copying it would change privacy or identity. |
| Explicit `use` trees/aliases and inline module paths | Resolve where origin is known | Rewrite source-relative paths before moving code. |
| Glob imports, ambiguous external-file/module identity, `#[path]` modules | Reject when required | Syntax alone cannot establish the intended original target. |
| Known unshadowed prelude paths | Leave to Rust only under a documented proven mapping | Do not misidentify a locally shadowed prelude name as a built-in. |
| Self-contained function-local non-generic structs/enums/type aliases | Copy and rename resolved references | Such declarations cannot be imported from outside their function. |
| Generic local declarations, enclosing generic captures, dependent const expressions | Reject initially | Relocation requires additional generic/const context analysis. |
| Self-contained, non-generic inherent implementations for copied types | Explicitly analyzed subset in task 009 | Methods must not disappear; all body dependencies must be resolved. |
| Trait-based method/associated-item resolution, trait impl copying, blanket impls | Reject for copied context | No compiler inference or coherence model is available here. |
| Bare field names and method labels | Not standalone free-variable references | They are not lexical variable lookups. Method calls still require separate safety checks. |
| Macro-produced declarations, macros in copied declarations or moved snippets | Reject initially | Token expansion may introduce names or depend on its original context. |
| `file!`, `line!`, `module_path!`, relative `include!` and similar macros | Reject in automatic mode | Relocation changes their meaning; ordinary mode retains its documented limitations. |
| `cfg`, `cfg_attr`, procedural attributes, derives on required copied declarations or enclosing selected scopes | Reject initially | Parsing is not conditional compilation or macro expansion. |
| Documentation comments / `#[doc = ...]` | Preserve | They do not supply semantic dependency resolution. |
| Other semantic attributes on required relocated context | Reject unless explicitly proven safe | Avoid changing representation, linking, or generated behavior accidentally. |
| Out-of-line module/dependency source loading, full compiler or rust-analyzer integration | Deferred | Requires a larger compilation-context model. |

Unrelated opaque syntax need not become a dependency. However, if an unsupported
binding form or item can affect lookup in an indexed enclosing scope, record a
barrier or stop with an explicit error; never ignore it and claim complete resolution.
The whole-file index may conservatively reject a file before selection-specific
filtering exists. This limitation must be reported, not hidden.

## Scope and input semantics

Track the type, value, and macro namespaces separately. A named-field struct occupies
the type namespace; tuple/unit structs also introduce a value constructor. Items
are visible throughout their containing lexical item scope. Let bindings are visible
only after their initializer (and outside their `let ... else` failure block).
Closures, match arms, loops, if-let conditions, and nested blocks have distinct scopes.
Item functions and modules form capture boundaries: local values must not leak into
item bodies as though those items were closures.

For example:

```rust
let name = String::from("Alice");
let user = User { name }; // selected statement
```

`fn build_user(name: String) -> User` explicitly supplies `name`. With no such input,
report `required input name is not supplied` at the reference and selected source
range. Do not copy or run `String::from("Alice")`. Do not infer an input's type.
Pattern-bound inputs must be compared by actual bound identifiers, not string splitting.

## Original versus copied types

For a known accessible `crate::models::User`, resolve the selected construction and
return type to that exact original declaration. A test can pass the result to an
existing function that accepts `crate::models::User`.

For a self-contained `User` declared inside the original function, create a uniquely
named test-only type beside the generated function in its invocation scope. Rewrite
only references resolved to that declaration, including the output and signature.
Tests in that scope can inspect its fields, but a copied struct/enum is a **new nominal
type**. A copied type alias is transparent and follows its rewritten target; when that
target is a reused original type, its identity remains original. Reject dependency
plans that require interoperability with the unnameable original type. Do not copy
inaccessible module-level types as if they were function-local declarations.

Generated names should be derived from the extraction's function identity and stable
declaration IDs, with deterministic collision checks against generated peers and
visible invocation identifiers where known. Never introduce a fixed helper name that
causes two otherwise distinct extractions to collide.

## Error and baseline contract

A resolver error identifies the source file, selected range, unresolved or unsupported
reference, and dependency chain when available. Distinguish missing explicit inputs,
unknown declarations, unknown module paths, unsafe promotion, macros/attributes, and
unsupported inference. Rust-generated borrow/type/privacy diagnostics remain rustc's
responsibility; do not promise to remap every compiler span.

Tasks 010–011 implement this as a [structured diagnostic boundary](diagnostics.md):
reports include snippet identity, typed categories, optional subjects/sites/chains, bounded
UTF-8-safe output, and remedies. Ordinary-mode errors have not been replaced or relabeled.

Automatic mode has the same default snippet guard as ordinary mode: omitted
`snapshot` uses `snapshots/<generated-function-name>.rs` from parsed `fn_sig`, not the
snippet key/module/range. Raw `r#type` becomes `type.rs`; Unicode names are retained.
Inline defaults are package-relative; TOML defaults are configuration-directory-relative,
like explicit paths. `snapshot = "path"` wins, `false` disables the guard, and
`true`/empty strings are invalid. Same names in one base share a path; use explicit
distinct paths for different bodies. Missing defaults report the exact resolved path;
missing/mismatching guards never fall back or automatically move ranges.
See [migration from unchecked omission](usage.md#migration-from-unchecked-omission).

The separate [versioned context baseline](context-snapshots.md) remains optional,
automatic-only, and has no default. Context-only guarding requires `snapshot = false`.
Existing explicit snippet paths remain valid. Discovered declaration contents and dependency
mappings use logical identities rather than checkout paths or transient IDs. Ambiguous
anchors fail instead of guessing. A test-only accessor provides review data; compilation
never creates, writes, updates, approves, or prints either kind of baseline. Original nodes remain terminals,
so this is not a fingerprint of all private dependencies or compiler/runtime behavior.

## Completion evidence

Task 001's acceptance criteria are covered by the matrix, the two type-identity cases,
the default-mode boundary, and the explicit input/error contracts above. Implementation
claims are gated by the corresponding [task acceptance tests](tasks/README.md).
