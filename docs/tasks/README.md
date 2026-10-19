# Automatic-context resolution backlog

This backlog tracks work toward automatic context discovery; completion of a
foundation task does not enable the public feature. The current macro still requires
external types/imports to be available at its invocation site. See the
[design contract](../automatic-context.md) for the agreed implementation boundary.

## Constraints

- Never insert markers, annotations, or wrappers into the original source.
- Users define tests and supply the function signature, input types, and output.
- Never execute earlier statements to reconstruct surrounding local values.
- Reuse accessible original declarations where possible; never bypass privacy.
- Copied local types have different identity. Do not hide that limitation.
- Reject ambiguous/unsupported context instead of guessing its meaning.
- Keep existing inline/config extraction behavior unchanged unless users opt in.
- Snapshots are explicitly reviewed and never written by compilation.

## Ordered tasks

| Done | Task | Dependencies |
| --- | --- | --- |
| [x] | [001 — Define the supported subset and safety contract](001-safety-contract.md) | None |
| [x] | [002 — Parse source files and locate selected syntax](002-source-selection.md) | 001 |
| [x] | [003 — Index lexical scopes and bindings](003-scope-index.md) | 002 |
| [x] | [004 — Index module paths and explicit imports](004-import-index.md) | 003 |
| [ ] | [005 — Collect references and required inputs](005-reference-collection.md) | 003 |
| [ ] | [006 — Resolve reusable original declarations](006-original-declarations.md) | 004, 005 |
| [ ] | [007 — Build the dependency graph](007-dependency-graph.md) | 006 |
| [ ] | [008 — Promote self-contained local types](008-local-types.md) | 007 |
| [ ] | [009 — Handle a conservative implementation subset](009-implementations.md) | 008 |
| [ ] | [010 — Report unsupported context precisely](010-diagnostics.md) | 006, 008, 009 |
| [ ] | [011 — Integrate an opt-in automatic-context pipeline](011-pipeline.md) | 007–010 |
| [ ] | [012 — Protect discovered context with snapshots](012-context-snapshots.md) | 011 |
| [ ] | [013 — Add end-to-end regression coverage](013-regression-tests.md) | 011, 012 |
| [ ] | [014 — Document the feature and add examples](014-documentation.md) | 013 |

Each task includes acceptance tests. Update its status and this checklist only when
those criteria are met. New implementation modules such as `scope.rs` and `resolve.rs`
are proposed, not empty placeholders to create in advance.

## Implementation branches

Each branch contains one task commit on top of its dependencies; `master` is unchanged.
Review/merge in this order (or compare each branch against its listed base):

| Task | Branch | Base |
| --- | --- | --- |
| 001 | `task/001-safety-contract` | `master` |
| 002 | `task/002-source-selection` | `task/001-safety-contract` |
| 003 | `task/003-scope-index` | `task/002-source-selection` |
| 004 | `task/004-import-index` | `task/003-scope-index` |

Tasks 002–004 are tested internal foundations, deliberately test-gated until
011 adds the public opt-in pipeline. The macro still does not accept `context = "auto"`.
Next: task 005 (reference collection).

## Out of scope for this milestone

Full rustc/rust-analyzer integration, arbitrary macro expansion, trait/type inference,
automatic capture of runtime locals, arbitrary cross-crate source copying, IDE plugins,
and test-case generation. An editor-independent CLI can be planned separately after
the extraction and snapshot APIs stabilize.
