# Public workflow regression matrix

Task 013 extends native Cargo consumers rather than replacing the focused AST/scope/
resolver tests. Every positive case calls generated functions or checks concrete type
compatibility; successful compilation alone is not a semantic guarantee.

Run from the repository root:

```sh
cargo test -p checkmate --test public_values --test public_failures --test public_rebuilds
cargo test -p checkmate --test automatic --test context_snapshots
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## New native consumers

Paths below are under [`crates/checkmate/tests/`](../crates/checkmate/tests/).

| Test group | Observable checks |
| --- | --- |
| `public_values::original_enums_aliases_and_reexports_keep_concrete_identity` | Unit, tuple, and named-field original enum variants; public reexports through a private module; explicit import aliases; inner imports shadow outer imports; concrete original type assignments and calls; local alias chains remain transparent to original types despite caller decoys |
| `public_values::copied_enum_aliases_diamond_and_recursive_dependencies` | Direct copied enum returns and inherent behavior; struct/tuple returns; typed moves between local aliases and their underlying struct; one shared leaf type across a diamond; recursive `Option<Box<Node>>` edges usable by caller-side allocation; distinct TypeIds across extractions and versus the original |
| `public_values::nested_shadowing_shorthand_and_explicit_inputs_skip_initializers` | Nested same-named local types, outer/module/caller decoys, let-initializer shadowing, preserved shorthand labels, supplied input values, and an observable initializer counter with positive original-execution control |
| `public_failures::copied_nominals_are_neither_each_other_nor_the_original` | Successful field reads and runtime identity checks, followed by Rust `E0308` for incompatible assignments; no guessed generated names |
| `public_failures::original_routes_and_private_members_are_not_widened` | Conservative rejection of an explicitly private source route even if a different public reexport exists; rustc enforces privacy of an original associated method |
| `public_failures::planner_failures_keep_categories_subjects_and_emit_no_partial_api` | Missing shadowed input, generic locals, derives, cfg, location-sensitive macros, transitive private dependencies with ancestry, unsupported borrowed dot calls, unsupported unused methods, and signature shadowing of an original constant |
| `public_rebuilds::reviewed_context_and_external_inputs_rebuild_without_cleaning` | Explicit custom library/test target paths; config-relative paths; coexistence of ordinary and automatic extraction; explicit review/approval; baseline-only/config-only drift; external statement-file changes; location moves/unrelated edits; unselected implementation drift; restored success after explicit updates |
| `public_rebuilds::ordinary_macro_expands_in_a_normal_build_but_its_function_is_test_only` | An active ordinary macro in a normal binary build succeeds without a function reference; an otherwise identical build referencing the generated function fails with `E0425` |

Copied enum inspection uses checked inherent methods or output expressions in the original
lexical context, not undocumented generated type names. Recursive allocation happens in
user-authored test code; this does not claim automatic support for discovering `Box::new`.
Alias transparency is checked by typed uses **within a build**. No TypeId stability across
builds/toolchains is asserted.

## Failure oracles and boundaries

Original libraries in the failure suite first pass `cargo check --lib`. A broken original
must not be mistaken for a correct extraction diagnostic. Expected Checkmate categories
and subjects are checked separately from rustc's type/privacy errors.

The no-partial-API check occupies the requested function and report names through ordinary
macros after a failed automatic expansion. Leaking either API would cause duplicate-item
`E0428`. A positive control verifies that duplicate definitions are diagnosed even beside
a Checkmate `compile_error!`; missing-name secondary diagnostics alone are unreliable
because rustc can suppress them after expansion failures. This oracle checks the extracted
and review APIs, not every possible internal helper item.

Normal production library builds do **not** expand integration-test macros. That isolation
check is not evidence that an expanded automatic function is cfg-gated. The Unix native
compiler wrapper in [`context_snapshots.rs`](../crates/checkmate/tests/context_snapshots.rs)
retains Cargo integration-target identity while removing rustc's `--test`. It checks report
accessor and extracted-function absence **independently**, with an unreferenced successful
build as a control. The new ordinary binary check above needs no wrapper and runs on all
platforms. Existing planner tests additionally inspect cfg gating of every emitted item.

## Rebuild and input preservation

The new suites share [`support/mod.rs`](../crates/checkmate/tests/support/mod.rs). Each
consumer owns one target directory, reused across successful builds, expected failures,
and explicit fixture edits. No step runs `cargo clean`. Cargo operates offline using
already available dependencies; native test fixtures intentionally need not be warning-free.

Before and after **every** Cargo command, the harness inventories all non-target files:
existing bytes must match, deletion fails, and newly created inputs fail. Only Cargo's
initial lockfile and build artifacts under its isolated target directory are exempted.
Intentional edits and baseline approval occur in the outer test harness before compilation.

Config, snippet/context baselines, and ordinary statement fixtures live outside Rust module
trees. The external-body rebuild test updates a runtime-read expected-value file, not a
compiler-tracked Rust expectation: a cached old function would return the wrong result.
Baseline/config-only tests require precise diagnostic transitions, not merely another
successful invocation of Cargo. A source move changes traversal IDs and line ranges while
the reviewed context model remains byte-identical.

Task-013 verification also compares SHA-256 hashes of 26 existing repository example
sources, manifests/configs, snapshots, and extraction fixtures against the task-012 branch.
No original fixture/baseline is updated to make a test pass.

## Editor compatibility follow-up

The [editor support fix](editor-support.md) adds original-input-token caller anchoring
without changing the accepted placement rules:

- [`caller_spans.rs`](../crates/checkmate/tests/caller_spans.rs) exercises all invocation
  delimiters, Unicode/comments, inline/configured metadata, forwarded token trees (local
  and exported wrappers), includes, nested invocations, and recovery without cleaning.
- A focused unit regression requires editor-specific guidance when the physical caller
  file is unavailable. Existing target-class, token, namespace, and privacy tests remain.
- [`scripts/test_rust_analyzer.py`](../scripts/test_rust_analyzer.py) independently drives
  real LSP expansion, function/field hover, enabled diagnostics, unsaved token mismatch,
  and buffer restoration. This opt-in external-tool test is **not** part of Cargo's test count.

Verification: **293 Cargo tests pass** on Rust 1.99.1 and the prior Rust 1.97.1 toolchain;
three remain ignored. The live editor checks pass with the documented experimental
version pairing. Legacy-protocol expansion still fails closed. These results do not
establish compatibility with every editor/version or unsaved-buffer state, nor a new
coverage measurement. Existing example/fixture source, config, and baseline bytes remain unchanged.

## Default snippet-guard follow-up

Omitting `snapshot` now selects `snapshots/<generated-function-name>.rs`; previous
intentionally unguarded native fixtures use explicit `snapshot = false`. Existing explicit
paths and context guards retain their meanings. See [migration and path rules](usage.md#detecting-line-drift-with-snapshots).

- Five option regressions check ordinary/automatic parity, raw and Unicode identifiers,
  signature parsing, configuration-relative defaults, overrides/opt-out, invalid values,
  duplicates, mixed options, and unchanged context-baseline semantics.
- Three native groups in [`default_snapshots.rs`](../crates/checkmate/tests/default_snapshots.rs)
  check real default-file reads, missing-file noncreation, baseline-only rebuilds, restored
  success, source drift before body parsing, explicit path precedence, opt-outs, and
  test-only dependency markers. Their shared inventory checks every compiler invocation
  for unexpected writes. Baselines are explicitly authored in the outer test harness.
- The original [`extraction.rs`](../crates/checkmate/tests/extraction.rs) and its existing
  source/config/snapshot fixtures remain byte-identical. Five additional, manually reviewed
  baselines cover its formerly unguarded functions; they are not generated by compilation.

Verification: **301 Cargo tests pass**, three ignored. Existing repository example/fixture
inputs remain unchanged. The real editor regression still checks expansion, types, negative
input rejection, and recovery separately from Cargo. Counts are not a new coverage measurement.

## Existing complementary checks

- [`automatic.rs`](../crates/checkmate/tests/automatic.rs): library-root/caller placement,
  original/local identity, privacy, renamed/raw macro aliases, CRLF, backend restoration,
  and source/config/snippet/caller/manifest incremental edits.
- [`context_snapshots.rs`](../crates/checkmate/tests/context_snapshots.rs): strict version/
  schema failure, missing baselines, explicit report/copy workflow, alias/context drift,
  report-name safety, and test-only emission.
- [`rebuild.rs`](../crates/checkmate/tests/rebuild.rs): ordinary external-file dependency
  tracking; [`extraction.rs`](../crates/checkmate/tests/extraction.rs): basic public extraction.
- [Public examples](../examples/README.md): original struct identity, copied locals,
  conservative inherent methods, and reviewed context baselines.
- Focused resolver/promotion tests, bounded graph properties, and diagnostic tests remain
  necessary for internal distinctions that native examples alone cannot exhaust.

These are regression results, not formal verification, exhaustive Rust coverage, or a new
line/branch coverage measurement. The published [coverage figures](coverage.md) still apply
only to their task-007 baseline. Task 014 adds the
[complete public automatic example](../examples/automatic-context/README.md); its tests
complement these task-013 regressions without changing their scope.
