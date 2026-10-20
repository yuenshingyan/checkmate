# Test coverage and generated properties

## Current measured refresh

These are **mapped-line measurements**, not test counts or formal guarantees. Both runs
used isolated `git archive` checkouts, Rust/Cargo **1.99.1** on aarch64 macOS, and its
matching **LLVM 23.1.1** `llvm-tools-preview` component. No selected source, configuration,
or snapshot bytes were changed by measurement. The global Rust default and library
MSRV (1.88) remain unchanged.

| Revision | Workspace tests | Checkmate implementation-file scope | Workspace, including tests/examples |
| --- | --- | --- | --- |
| Merged CI baseline [`945885e`](https://github.com/yuenshingyan/checkmate/commit/945885e483fd7776d49e3a2f13c998c4864f408f) | 305 passed, 3 ignored | **5,725 / 6,256 = 91.51%** | **13,068 / 13,649 = 95.74%** |
| Targeted regressions [`3a55fa1`](https://github.com/yuenshingyan/checkmate/commit/3a55fa15aecd9371a585cf16be2904d88cbe8f0c) | 311 passed, 3 ignored | **5,751 / 6,256 = 91.93%** | **13,258 / 13,813 = 95.98%** |

The second revision changes tests/tooling, not production Rust behavior. Documentation
and these reports are published afterward, avoiding a self-referential measured commit.
The same implementation-file denominator gained **26 covered mapped lines** (about
**0.42 percentage points**). The workspace percentage includes new test code; it is not
the implementation metric.

Reviewable artifacts:

- [Before: per-file lines](coverage/945885e-library.txt)
- [After: per-file lines](coverage/3a55fa1-library.txt)
- [Machine-readable measurement record](coverage/refresh.json)

### Meaningful gaps addressed

Six new regression groups exercise these behaviors, rather than manufacturing invalid
internal graphs merely to raise a percentage:

- **Source identity:** an explicit binary with no path must have exactly one inferred
  root. Missing and competing roots fail; both supported single-root layouts succeed.
- **Caller identity:** the equivalent explicit integration-test inference cannot guess
  from a crate name. Rejections preserve inputs; positive controls verify the exact root.
- **Source manifest fields:** malformed `autobins` and library paths produce their own
  structured field-specific errors. Earlier malformed fixtures could be rejected for
  their missing edition before reaching these validators.
- **Caller manifest fields:** malformed auto-target switches, missing target names, and
  non-string paths fail the intended placement validators, without modifying inputs.
- **Context review:** valid-schema TOML containing malformed tokens in reference paths
  or routes is rejected, leaving both the model and fixture bytes unchanged.
- **Copied method patterns:** `Self`, named-type, and transparent-alias struct patterns
  preserve field bindings and owner identity. Original and generated programs compile;
  runtime assertions compare values for 0, 7, and 255.

The new tests are in [source origin](../crates/checkmate/src/source/origin/tests.rs),
[placement](../crates/checkmate/src/automatic/tests.rs),
[context baselines](../crates/checkmate/src/snapshot/context/tests.rs), and
[copied implementations](../crates/checkmate/src/resolve/promote/tests/implementations.rs).
No production bug was found in these cases. Existing compiler and editor checks remain
separate evidence; the coverage run does not invoke rust-analyzer or measure Python.

## Scope and limitations

- **Library is file-based, not pure production-only coverage.** It selects Checkmate's
  mapped implementation files, excluding `tests/`, `tests.rs`, `*_tests.rs`,
  `walkthroughs.rs`, `resolve/graph/properties.rs`, and the test-only
  `diagnostics/integration.rs`. Example packages are excluded. Inline `cfg(test)` code
  remains, notably the tests in `snapshot/identity.rs` and test-only analysis helpers
  in `scope/imports.rs` and other implementation files.
  There are 27 implementation-scope files and 70 workspace mapped files in each run.
- The workspace scope is restricted to files within the archived workspace packages.
  Dependencies, standard-library sources, and temporary generated consumer files are
  not included. Counts come from LLVM's per-file `summary.lines`, **not physical lines**
  or an ad-hoc interpretation of coverage segments.
- Both runs used **25 workspace coverage objects**, including the proc-macro dynamic
  library, and merged **570 raw profiles**. The dynamic library matters because macro
  code executes inside rustc, not only inside the unit-test executable.
- Cargo child processes inherit instrumentation and the absolute profile destination.
  Direct `rustc` probes do **not** automatically consume `RUSTFLAGS`. Temporary native
  objects deleted by their harnesses are not recovered. Merging their available profiles
  against retained workspace objects does **not** establish complete generated-code or
  native-consumer coverage; different artifact identities may lack matching mappings.
  A zero count is therefore not sufficient evidence that no existing test exercises a
  behavior. Gap triage also reviewed existing tests and reachable preconditions.
- The runner rejects nonzero LLVM merge/report commands and their diagnostics. It also
  checks visible LLVM profile-runtime errors; errors suppressed inside successful child
  harnesses cannot all be observed at the outer boundary. No merge/report diagnostics
  occurred in either reported run.
- All archived input entries were inventoried before and after testing: **143 files**
  before, **145 files** after adding the two Python tools. Both inventories matched,
  including bytes and modes. Build/profile/report artifacts live outside the source tree.
- **Branch coverage was not measured.** Rust 1.99.1 still rejects
  `-C instrument-coverage=branch`; no nightly flags or zero-branch totals are presented
  as a branch-coverage percentage. Region/function counters are not substituted for it.
- Remaining gaps include defensive invariants, earlier-rejected unsupported forms, and
  platform/error paths. The 505 uncovered mapped implementation-scope lines are not a
  ranked bug list. Coverage shows execution, not assertion strength, privacy proofs,
  semantic equivalence, or exhaustive Rust support.

The baseline's retained full export was reclassified to exclude the test-only diagnostics
integration module before comparison; the same final filter was applied to both revisions.
The task-007 figures below use an older codebase, toolchain, and filter and are **not a
like-for-like trend comparison** with these measurements.

## Reproduce the current measurements

Use Python **3.12+**, Git, rustup, and a full repository checkout. Install matching LLVM
tools explicitly; no Cargo plugin or nightly compiler is required:

```sh
rustup component add llvm-tools-preview --toolchain 1.99.1
cargo fetch --locked
python3 -B -m unittest discover -s scripts -p 'test_*helpers.py'

OUT=$(mktemp -d)
python3 -B scripts/measure_coverage.py \
  --revision 945885e483fd7776d49e3a2f13c998c4864f408f \
  --toolchain 1.99.1 --output "$OUT/before"
python3 -B scripts/measure_coverage.py \
  --revision 3a55fa15aecd9371a585cf16be2904d88cbe8f0c \
  --toolchain 1.99.1 --output "$OUT/after"
```

The output directory must not exist and must be outside the checkout. The runner archives
an immutable commit, sets `RUSTUP_TOOLCHAIN` for subprocesses, and invokes
`cargo test --workspace --locked --offline --message-format=json` with
`RUSTFLAGS='-C instrument-coverage'` and a fresh `%p-%m.profraw` destination. It discovers
workspace executable/proc-macro artifacts from Cargo JSON, merges profiles through an
input list, and selects the measured toolchain's LLVM tools rather than falling back to
Xcode or arbitrary PATH tools. It rejects conflicting compiler environment overrides;
normal Cargo configuration and the dependency cache are still inherited and disclosed.
Native fixtures retain their existing offline first-build lockfile behavior.

Each output contains commands and separate stdout/stderr logs, tool versions, object/profile
inventories, before/after source inventories, a full LLVM JSON export, LLVM text reports,
a relative-path per-file table, `summary.json`, and `artifactmanifest.json`. A failed run
raises an error and must not be published as a successful measurement. Use a new output
directory rather than combining stale profiles or objects. The full raw export is about
24 MB here and is not committed; the bounded record above includes its digest for audit.

[CI](ci.md) runs the ten Python assertion/report-helper tests, but does not run this
expensive coverage measurement on every PR. The six coverage-helper tests validate object
selection, file/scope boundaries, line aggregation, test-count parsing, and input inventories;
the other four test the separate editor harness. These counts are not Rust test counts.

## Historical baseline: task 007

Revision: `c9eafeb606348ef6df1c81038a945b425a32259a` (before task 008).
An isolated checkout ran **156 workspace tests**, with one ignored documentation test;
archived source/config/snapshot bytes were unchanged.

| Scope | Covered mapped lines | Line coverage |
| --- | --- | --- |
| Library implementation files, historical filter | 2,936 / 3,214 | **91.35%** |
| Workspace, including tests/examples | 5,843 / 6,147 | **95.05%** |

See the retained [per-file report](coverage/task-007-library.txt). That file-based filter
excluded `tests/`, `tests.rs`, `walkthroughs.rs`, and `examples/`. These figures do not
measure task 008 or later work. The original tools were Rust/Cargo 1.97.1, Rust LLVM
22.1.6, and Apple LLVM 21.0.0 via `xcrun`; merge/report completed without diagnostics
despite the version difference. The current refresh instead uses matching LLVM tools.
Neither baseline measured branch coverage.

## Bounded generated properties added in task 008

`resolve/graph/properties.rs` adds dependency-free, deterministic generated tests:

- **14,788 graph models**, checking cycle detection against an independent transitive-
  closure oracle and validating returned cycle witnesses.
- **64 generated Rust sources**, distinguishing alias expansion, unguarded layout,
  and constraint-only dependency edges.
- **Eight generated DAGs**, each checked across six root permutations, duplicate roots,
  and two independent checkouts, for deterministic order and deduplication.

Run with:

```sh
cargo test -p checkmate --lib resolve::graph::properties
```

These are bounded exhaustive/generated checks, not random fuzzing or a proof for all
Rust programs. Compiler-backed promotion tests separately check generated behavior,
privacy, identity changes, and rejection paths. Keep these claims distinct from the
mapped-line measurements above.
