# Reviewed automatic-context snapshots

Task 012 adds optional context guards to `context = "auto"`. Ordinary snippet snapshots
remain unchanged. Neither compilation nor the review accessor writes any files.

Start with the [complete public workflow](../examples/automatic-context/README.md),
which reuses an original struct and demonstrates configuration-relative paths and both
guards. For copied nominal context, see the runnable [local-type example](../examples/local-types/context-snapshots.toml),
its [tests](../examples/local-types/tests/context_snapshot.rs), and the explicitly
reviewed [version-1 baseline](../examples/local-types/snapshots/user.context.toml).

## Options

Both options require automatic mode and its existing
[verified placement rules](usage.md#automatic-context-opt-in):

- `context_snapshot = "snapshots/user.context.toml"`: compare the complete supported
  model against a reviewed TOML baseline. Inline paths are package-relative; paths in
  a snippet definition are configuration-relative, just like `snapshot`.
- `context_report = "user_context"`: additionally emit a test-only, zero-argument
  **accessor function** returning the model as `&'static str`. It only returns a literal;
  it does not run the extracted function, execute earlier source statements, print, or
  write a file. Use a distinct Rust identifier (prefer snake_case). Conflicting caller
  source-written bindings, generated-plan identifiers, and standard prelude values are rejected.
  Names emitted by separate macro invocations are not preflighted: duplicates are still
  rejected by Rust (`E0428`), not remapped into a Checkmate diagnostic.

Either can be used alone, or together. Omission of both preserves task-011 automatic
behavior, including cases whose local declarations have no supported stable snapshot
anchor. Omission of `context` preserves ordinary extraction; these options are rejected
there, not silently ignored. Do not mix inline options with `config`/`snippet`.

For example, a configured definition can add:

```toml
context = "auto"
context_report = "user_context"
context_snapshot = "snapshots/user.context.toml"
```

The report is deliberately **not a constant**. Introducing a named constant can change
an identifier binding into a constant pattern in other Rust code. The accessor avoids
that behavior, including in out-of-line caller modules that Checkmate does not read.

## Create or update a baseline explicitly

1. Add `context_report`, initially **without** `context_snapshot`. Keep any existing
   snippet `snapshot` guard. Supply the same signature/selection/output you intend to test.
2. Add a user-authored inspection test in the verified integration-test root:

   ```rust,ignore
   #[test]
   #[ignore]
   fn print_context_for_review() {
       println!("{}", user_context());
   }
   ```

3. Run it explicitly, for example:

   ```sh
   cargo test -p checkmate-local-example --test context_snapshot print_context_for_review -- --ignored --nocapture
   ```

   The **test**, not the compiler, prints the model. Inspect the extraction, source
   identity, copied-versus-reused declarations, implementations, and dependency mappings.
   Copy just the TOML data, not Cargo/test-harness output, into your chosen baseline file.
4. Add/restore `context_snapshot` and run tests. Commit the reviewed baseline and metadata
   together. The report option/test can remain for future inspection or be removed.
5. When a context change is intentional, temporarily remove the context guard **explicitly**
   to inspect the new report. Review the difference, manually replace the baseline, then
   restore the guard. A mismatching/missing guard never falls back to report-only mode.

There is no automatic acceptance flag, baseline writer, migration, or general capture CLI.
The baseline is versioned analysis data, not generated Rust helper definitions, a compiler
binary, or a hash asserted to prove equivalence.

## Version 1 format

The complete file begins with:

```toml
format = "checkmate-context"
version = 1
```

The example baseline shows all required fields. Tables reject unknown fields; unsupported
versions, malformed schemas, and invalid embedded Rust syntax fail with `CM013`.

| Field | Reviewed data |
| --- | --- |
| `extraction` | Normalized supplied signature/visibility, selected statements, and output in one analysis function, **before** helper-name rewriting |
| `required_inputs` | Explicit input names required by the collector |
| `source` | Cargo package name, library target name, package-relative canonical source path, and named source owner |
| `references` | Ordered signature/source/output occurrences: original path, usage, resolved declaration, import route, and associated member when applicable |
| `declarations` | Stable-keyed discovered nodes: original/copied/external role, source declaration syntax, known same-file inherent implementations, dependency references and graph edges |

An empty reference `target` denotes a resolved binding (input, generic, or local), not
an unresolved declaration. Declaration dependency references include the graph's
`indirect`, `layout`, and `expansion` flags; edges retain `unguarded_layout` and
`alias_expansion`. These are the resolver's distinct dependency relations, not a new
storage-layout inference engine.

`copied_struct`/`copied_enum` roles explicitly mean **new nominal identities**.
`copied_alias` remains transparent. Original declarations retain their identity/privacy;
recording their syntax does not copy their executable definitions into the test.

## Stable identities and location moves

Source keys use named lexical ancestry and declaration kind/name, for example
`source:crate/fn:original/struct:User`. Method ancestry uses the **resolved nominal impl
owner**, not an impl header's alias spelling. External keys use their qualified paths.
Neither line/column offsets, traversal IDs, absolute checkout roots, nor generated helper
names are used as metadata identities.

- Independent checkouts with the same logical package/source produce the same model.
- Blank lines, ordinary comments, Rust whitespace, CRLF normalization, unrelated items,
  and movement of whole declarations/impl blocks do not invalidate an otherwise unchanged
  model. Update `start`/`end` yourself when selected lines move; snapshots do not relocate code.
- Declaration/field/literal/doc-attribute content, implementation content, signature/output,
  dependency membership, alias targets, and resolved import routes are compared.
- A file/package/library/named-owner rename is an identity change, not just a line move.
- Anonymous scopes are not assigned fragile positional IDs. Same-named locals in different
  anonymous scopes under the same named owner can be ambiguous. A required ambiguous
  declaration or owner fails closed; unrelated ambiguous locals do not invalidate a model.
  Use snippet-only guards/ordinary automatic mode for these cases rather than adding
  markers or renaming the original source to satisfy a guessed identity.
- Context snapshots currently require the canonical source to remain under its package
  root. Unsupported external/symlink layouts are not assigned absolute-path identities.

Embedded Rust is normalized as syntax: ordinary comments and formatting are ignored,
while literal contents and literal documentation attributes remain significant (subject
to Rust's CRLF normalization). The ordinary `snapshot` guard still compares selected
source text and can be used alongside the context guard for stricter textual review.

## Limits and diagnostics

This is the **approved discovery graph**, not a whole-program semantic fingerprint.
Reusable originals remain graph terminals. Their direct item syntax and known same-file
inherent impl syntax are recorded, but their private/transitive dependencies are not
recursively expanded. Standard/external nodes record paths, not dependency source trees.
Toolchain/package/dependency versions, edition, build environment, features, runtime
inputs, and all compiler semantics are not frozen by this format. Original source literals may themselves
contain absolute paths; only metadata identities are checkout-independent.

A successful comparison is not proof of semantic equivalence, stable ABI, stable `TypeId`
across builds, privacy, or correct typing. Rust still checks generated code. Unsupported
analysis fails before baseline comparison and never emits a partial plan.

`CM013` diagnostics summarize added, removed, and changed declarations, reference/edge
mapping changes, extraction changes, or source-identity changes. Reports remain bounded
by the [diagnostic contract](diagnostics.md); use the accessor for the full review data.
The context baseline joins source, caller, manifest, config, and snippet baseline as an
`include_str!` compilation dependency. Even a baseline-only edit triggers revalidation.
