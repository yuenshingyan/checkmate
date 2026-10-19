# Test coverage and generated properties

## Measured baseline: task 007

Revision: `c9eafeb606348ef6df1c81038a945b425a32259a` (before task 008).
An isolated `git archive` checkout ran all **156 workspace tests** successfully;
one documentation test was ignored. All archived source/config/snapshot bytes were
checked against the committed archive after testing and remained unchanged.

| Scope | Covered mapped lines | Line coverage |
| --- | --- | --- |
| Library implementation files | 2,936 / 3,214 | **91.35%** |
| Workspace, including tests/examples | 5,843 / 6,147 | **95.05%** |

The library filter excludes `tests/`, `tests.rs`, `walkthroughs.rs`, and `examples/`.
It is file-based, not semantic removal of inline test code. These figures count code
represented in LLVM coverage mappings, **not every tracked source line**, and include
test-gated analysis foundations. They do not measure the new task-008 implementation.
See the [per-file library report](coverage/task-007-library.txt).

Tools: Rust/Cargo 1.97.1 on aarch64 macOS, Rust LLVM 22.1.6, and existing Apple LLVM
21.0.0 `llvm-cov`/`llvm-profdata` via `xcrun`. Merge/report completed without diagnostics
despite that version difference; matching LLVM versions are preferable for future runs.
Neither `cargo-llvm-cov` nor `llvm-tools-preview` was installed. No installation or
toolchain changes were needed.

**Branch coverage was not measured.** Stable rustc rejected
`-C instrument-coverage=branch`; `-Z coverage-options=branch` required nightly.
Zero mapped branches in the report is not 0% or 100% coverage.

### Reproduction outline

Use a clean, disposable checkout of the revision being measured, not a concurrently
edited working tree. On the measured macOS toolchain:

```sh
REV=c9eafeb606348ef6df1c81038a945b425a32259a
DIR=$(mktemp -d)
git archive "$REV" | tar -x -C "$DIR"
cd "$DIR"
mkdir -p coverage/profiles
CARGO_TARGET_DIR="$DIR/target" \
RUSTFLAGS="-C instrument-coverage" \
LLVM_PROFILE_FILE="$DIR/coverage/profiles/%p-%m.profraw" \
  cargo test --workspace --locked --offline --message-format=json > coverage/tests.log
xcrun llvm-profdata merge -sparse coverage/profiles/*.profraw -o coverage/merged.profdata
```

Extract the workspace test executable paths from Cargo's `compiler-artifact` JSON
records. Include the Checkmate proc-macro dynamic library as another coverage object:
its expansion code runs inside rustc, not only inside the test executable. The baseline
used six test executables and that dynamic library, and merged compiler/subprocess
profiles too. Pass all seven objects to `llvm-cov report`:

```sh
# Replace each placeholder with an artifact path from that exact instrumented build.
xcrun llvm-cov report TEST_EXECUTABLE \
  -object OTHER_TEST_EXECUTABLE -object CHECKMATE_PROC_MACRO_LIBRARY \
  -instr-profile=coverage/merged.profdata \
  '-ignore-filename-regex=/(\.cargo|\.rustup|rustc)/|/tests/|/tests\.rs$|/walkthroughs\.rs$|/examples/'
```

Supply every remaining test executable with another `-object`; the shortened example
above is not a complete artifact list. For workspace coverage, omit the test/example
exclusions. Do not combine stale objects/profiles from different builds.

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
privacy, identity changes, and rejection paths. Coverage measures execution, not the
strength of assertions or semantic equivalence.
