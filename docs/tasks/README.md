# Automatic-context resolution backlog

Task 011 exposes the tested foundations through opt-in `context = "auto"` for verified
library-root and package integration-test-root extractions. Ordinary extraction still
requires external context at its invocation site. See the
[design contract](../automatic-context.md) and [public placement limits](../usage.md#automatic-context-opt-in).

## Constraints

- Never insert markers, annotations, or wrappers into the original source.
- Users define tests and supply the function signature, input types, and output.
- Never execute earlier statements to reconstruct surrounding local values.
- Reuse accessible original declarations where possible; never bypass privacy.
- Copied local structs/enums have different nominal identity; aliases follow their targets. Do not hide that limitation.
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
| [x] | [005 — Collect references and required inputs](005-reference-collection.md) | 003, 004 |
| [x] | [006 — Resolve reusable original declarations](006-original-declarations.md) | 004, 005 |
| [x] | [007 — Build the dependency graph](007-dependency-graph.md) | 006 |
| [x] | [008 — Promote self-contained local types](008-local-types.md) | 007 |
| [x] | [009 — Handle a conservative implementation subset](009-implementations.md) | 008 |
| [x] | [010 — Report unsupported context precisely](010-diagnostics.md) | 006, 008, 009 |
| [x] | [011 — Integrate an opt-in automatic-context pipeline](011-pipeline.md) | 007–010 |
| [x] | [012 — Protect discovered context with snapshots](012-context-snapshots.md) | 011 |
| [x] | [013 — Add end-to-end regression coverage](013-regression-tests.md) | 011, 012 |
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
| 005 | `task/005-reference-collection` | `task/004-import-index` |
| Current examples | `examples/context-walkthroughs` | `task/005-reference-collection` |
| 006 | `task/006-original-declarations` | `examples/context-walkthroughs` |
| 007 | `task/007-dependency-graph` | `task/006-original-declarations` |
| 008 | `task/008-local-types` | `task/007-dependency-graph` |
| 009 | `task/009-implementations` | `task/008-local-types` |
| 010 | `task/010-diagnostics` | `task/009-implementations` |
| 011 | `task/011-pipeline` | `task/010-diagnostics` |
| 012 | `task/012-context-snapshots` | `task/011-pipeline` |
| 013 | `task/013-regression-tests` | `task/012-context-snapshots` |

Tasks 002–010 were tested internal foundations; task 011 enables their production
analysis path for the verified public opt-in. Historical task-specific descriptions
record their boundaries at implementation time, not the current public feature status.
Task 012 adds reviewed context snapshots without compilation writes.
Task 013 adds [native workflow regressions](../regression-tests.md).
Next: task 014 (integrated documentation and examples).
The [current examples](../../examples/README.md) illustrate these foundations and the
existing public API; they do not mark task 014's integrated-feature documentation complete.

## Out of scope for this milestone

Full rustc/rust-analyzer integration, arbitrary macro expansion, trait/type inference,
automatic capture of runtime locals, arbitrary cross-crate source copying, IDE plugins,
and test-case generation. An editor-independent CLI can be planned separately after
the extraction and snapshot APIs stabilize.
