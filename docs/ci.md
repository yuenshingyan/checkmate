# CI and compatibility checks

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs on pushes, pull requests,
manual dispatch, and a weekly schedule. It does not publish packages, approve baselines,
or modify branch protection. Actions are commit-pinned, permissions are read-only,
and checkout does not persist credentials.

## Checks

| Job | Toolchain/platform | Checks |
| --- | --- | --- |
| Rust matrix | Rust 1.88.0 and 1.99.1 on Ubuntu 24.04, macOS 14, Windows 2022 | Full locked workspace tests, including temporary native consumers, plus the basic executable |
| Moving compiler | Latest stable on Ubuntu 24.04 | Same regressions; detects compatibility drift without changing the pinned editor pair |
| Quality | Rust 1.99.1 on Ubuntu 24.04 | Package-scoped formatting, warning-denying Clippy and rustdoc, Python LSP assertion-helper tests |
| Editor | Rust 1.99.1 plus rust-analyzer 0.3.3081 on Ubuntu 24.04 | Real LSP expansions, hovers, positive diagnostics, unsaved-token rejection, and recovery |
| `CI required` | Aggregate | Fails unless every Rust matrix leg, quality job, and editor job succeeded |

The matrix specifies runner labels, not a blanket promise for every architecture or OS
version. Rust compiler host details are printed in each Rust job. A configured job is
not evidence of success: consult its run result, especially when adding a new platform.
The library MSRV stays **1.88**; the newer compiler is required only for the tested
experimental editor pairing. Locally, **305 workspace tests passed** (three ignored)
on both Rust 1.88.0 and 1.99.1 on macOS after the portable-wrapper changes; hosted runs
supply the Linux/Windows evidence.

`RUSTUP_TOOLCHAIN` is set at job scope, rather than only using `cargo +VERSION`.
Temporary consumers and direct `rustc` subprocesses therefore use the selected compiler,
even though they execute outside the checkout. The global Rust default is not changed.
`--locked` protects the workspace dependency resolution. Native fixtures intentionally
create their own lockfiles on their first offline build; their existing input-inventory
checks remain active. This is not a separately pinned dependency universe per fixture.

Rust tests and builds use two workers each to limit concurrent native compilation on
hosted runners. Matrix failures do not cancel other legs. Jobs have finite time limits;
Rust/editor test output is uploaded for 14 days even after failures. Compiler
installation failures remain failures, not skipped compatibility checks.

## Editor artifact integrity

The editor job downloads the official Linux x86-64 release asset from
[rust-analyzer's 2026-10-12 release](https://github.com/rust-lang/rust-analyzer/releases/tag/2026-10-12),
corresponding to commit `2ad81e72b7e978cb800dffe1bcce5c9f98032acf` and extension version
0.3.3081. Its compressed SHA256 is pinned in the workflow:

```text
040cab4e2abc490fadd95c3f6cbc60b941a78761dbc3acf0ec66fd7a97f31db3
```

The job verifies that digest **before executing** the binary, then checks the exact
server/compiler version strings and macro-server presence. This Linux asset reports
`rust-analyzer 0.3.3081-standalone` without the commit/date suffix printed by the macOS
build; its separately verified digest pins the artifact, rather than relying on that
shorter version label alone. A missing or changed artifact
fails; there is no latest-release fallback. The digest must not be replaced merely to
make a download pass. Review upstream provenance and rerun the actual LSP tests when
updating either half of the pair. Rustup installs the versioned compiler using its normal
distribution verification; the workflow does not independently pin all Rust component
archives.

The LSP harness is POSIX-only. Windows compiler coverage does **not** establish Windows
editor support. The existing locally tested macOS pairing remains documented in
[editor support](editor-support.md). No diagnostics are suppressed and no editor flag
bypasses caller identity checks. Four Python helper tests cover platform-dependent
`use ::path`/`use::path` pretty-printing while retaining alias resolution, string-literal
exclusion, and wrong-type rejection. They do not substitute for the actual LSP session.

## Input integrity and Windows

[`.gitattributes`](../.gitattributes) requests LF checkouts for text, preventing a Windows
checkout from silently changing selected source or baseline bytes. CRLF regression cases
still construct their own explicit temporary inputs. No selected source or snapshot
contents were rewritten to add CI.

Every Rust, quality, and editor job checks for tracked changes and untracked files after
running, even on failure. Ignored build artifacts are allowed. Native fixture inventories
independently check inputs around compiler invocations. These are bounded integrity
checks, not a filesystem sandbox or protection against malicious tests.

Native-rustc test executables use the platform executable suffix. Dependency-marker tests
compare parsed string-literal paths, not Rust-escaped token text (Windows backslashes
otherwise cause a false failure). The non-test-emission
probe uses a compiled Rust wrapper on every platform instead of a Unix shell script;
it removes only `--test` and checks that both the extracted function and context accessor
are absent. Windows does not silently skip that contract.

## Requiring the checks

Once this workflow is merged and its jobs have registered, configure the `master`
branch rule/ruleset to require **`CI required`**, with GitHub Actions as the expected
source. Requiring an up-to-date branch also ensures changes to the base are tested.
Do not require only one matrix leg, and do not use an optional/continue-on-error editor
job to claim compatibility. The aggregate runs even when prerequisites fail or are
skipped, and accepts only success for each job group.

The workflow file alone **does not enforce merge protection**. Enabling a repository rule
is a separate administrative operation; no existing repository rules are overwritten by
this change. Keep the aggregate job name stable, or update the rule when renaming it.

## Local reproduction

```sh
RUSTUP_TOOLCHAIN=1.88.0 cargo test --workspace --locked
RUSTUP_TOOLCHAIN=1.99.1 cargo test --workspace --locked
cargo fmt -p checkmate --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
python3 -B scripts/test_editor_helpers.py
python3 scripts/test_rust_analyzer.py /path/to/verified/rust-analyzer --timeout 180
```

Install the relevant toolchain first with `rustup toolchain install VERSION --profile
minimal`. On PowerShell, set `$env:RUSTUP_TOOLCHAIN = "1.88.0"` before invoking Cargo;
the POSIX inline environment syntax above is not PowerShell syntax. The workflow uses
Bash explicitly for cross-platform Rust jobs. To validate workflow syntax locally, run
[`actionlint`](https://github.com/rhysd/actionlint) from the repository root.

These checks are native/compiler/editor regressions, not formal guarantees or a new
coverage measurement. The [published coverage baseline](coverage.md) remains historical.
