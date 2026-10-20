# rust-analyzer support

Automatic context needs real source locations from the macro host. Cargo/rustc provides
these directly; rust-analyzer's default **legacy macro protocol does not**. An editor
`CM003` saying the invocation file is unavailable does not mean `module` or the selected
line range is wrong. Changing the original source or disabling diagnostics is not a fix.

## Tested workspace setup

This repository configures the following development/editor pairing:

- Rust **1.99.1**, pinned by [`rust-toolchain.toml`](../rust-toolchain.toml), including
  `rust-src`, Clippy, and rustfmt. The Checkmate library's compiler MSRV remains **1.88**;
  the editor pairing is an additional development requirement, not a new library MSRV.
- rust-analyzer **0.3.3081**, commit `2ad81e72b7` (2026-10-11), the tested VS Code extension.
- [`.vscode/settings.json`](../.vscode/settings.json) enables the experimental
  **bidirectional postcard** macro protocol through the language server's environment:

```json
{
    "rust-analyzer.server.extraEnv": {
        "RUST_ANALYZER_USE_POSTCARD": "1"
    }
}
```

Open the repository root in VS Code, allow the workspace settings, and run
**rust-analyzer: Restart server** after pulling these changes. If the toolchain has not
been installed yet, run `cargo --version` at the repository root to let rustup install
it first. This is a project-local toolchain selection; the global Rust default need not
change. Do not configure an older custom `rust-analyzer.procMacro.server` for this setup.

No `macro-error` suppression or ignored Checkmate macro is needed. If you previously
added either workaround, remove it: hiding errors does not restore generated-function
analysis. Projects using Checkmate outside this workspace need their own compatible
editor/server setup; this workspace configuration is not installed by the library.

**The protocol is experimental, not a stable compatibility promise.** The tested editor
with the previously installed Rust 1.97.1 server fails postcard negotiation and silently
falls back to the legacy protocol; setting the environment variable alone is insufficient.
Other versions may work, but must be verified rather than assumed compatible. The server
comes from the selected Rust toolchain, not just the VS Code extension.

## Why the macro also changed

In the tested rust-analyzer version, `Span::call_site()` has the macro *definition*'s
source coordinates. Checkmate now anchors caller capture to the first **original input
token** instead, before parsing or reconstructing tokens. This supplies an actual argument
location in both rustc and a metadata-capable editor host.

A token span is not sufficient proof by itself. All existing checks still apply:

- The physical file must be the unique Cargo integration-test root for this package/target.
- The location must be inside a literal, fully qualified, root-level Checkmate invocation.
- Its complete tokens must match the saved caller source.
- Namespace, target-class, attribute, snapshot, and discovery checks must succeed.

No editor environment flag enables a bypass. Missing physical locations still fail
closed; Checkmate does not substitute display filenames, scan for matching snippets,
infer a caller from a target name, or emit approximate stubs. Forwarded macros, includes,
and nested invocations remain unsupported. An empty invocation still fails argument parsing.

## Saved files and editor buffers

Checkmate analyzes files **on disk**, including the caller, source, configuration, and
baselines. rust-analyzer may have unsaved buffer contents. Unsaved changes to macro
arguments can therefore fail the caller-token agreement check until saved. Changes outside
an invocation may not be reflected in Checkmate's analysis until saved, even when expansion
succeeds. Save relevant files before relying on the result; **Cargo tests remain authoritative**.
There is no whole-buffer synchronization or automatic baseline approval.

## Verification

Run from the repository root, using the actual rust-analyzer executable you want to test:

```sh
python3 scripts/test_rust_analyzer.py /path/to/rust-analyzer
cargo test -p checkmate --test caller_spans
cargo test --workspace
```

The Python 3.9+ harness uses POSIX process groups for cleanup (tested on macOS). It drives
the real language server, rather than merely checking that Cargo passes or hiding an
editor diagnostic: six expansions, nine function/field hovers, error-free diagnostics,
an unsaved-token mismatch error, then recovery after restoring the buffer. It explicitly
enables rust-analyzer's experimental native diagnostics for the negative control. See
the script for exact assertions. It uses the workspace's server environment (strict JSON
settings) and does not edit the examples or their baselines. Native tests independently verify literal argument
locations and continued rejection of forwarded, nested, and included invocations.

For a quick macro-expansion diagnostic scan (not a substitute for the LSP test):

```sh
RUST_ANALYZER_USE_POSTCARD=1 /path/to/rust-analyzer diagnostics examples/struct-return
```

The CLI diagnostic scan does not enable all test bodies, whereas editor requests exercise
generated functions in the test context. Do not equate a clean CLI scan with complete IDE
or compiler correctness. To inspect negotiation, set `RA_LOG=proc_macro_api=info` and use
rust-analyzer's `--log-file` option: the tested setup reports `BidirectionalPostcardPrototype`,
not `LegacyJson`.

## Automated compatibility check

The [CI workflow](ci.md) provisions the corresponding official Linux x86-64 release
from the fixed `2026-10-12` tag, verifies a committed SHA256 before execution, and runs
this same harness with Rust 1.99.1. Missing artifacts, changed digests, negotiation or
analysis failures fail the job; no latest-version or diagnostic-suppression fallback is
allowed. The editor job is a prerequisite of `CI required`, separate from the compiler
matrix. Its run result supplies Linux evidence; Windows compiler jobs do not test the
POSIX-only editor harness. See the CI guide for merge-protection setup and upgrade rules.

## Upstream details

These version-specific implementation details explain the compatibility boundary:

- [Protocol selection](https://github.com/rust-lang/rust-analyzer/blob/2ad81e72b7/crates/proc-macro-api/src/process.rs): the postcard switch, negotiation, and legacy fallback.
- [Server span methods](https://github.com/rust-lang/rust-analyzer/blob/2ad81e72b7/crates/proc-macro-srv/src/server_impl/rust_analyzer_span.rs): physical paths/locations require callbacks absent from the legacy protocol.
- [Expansion globals](https://github.com/rust-lang/rust-analyzer/blob/2ad81e72b7/crates/hir-expand/src/lib.rs): call-site hygiene is currently applied to definition coordinates.

This setup provides bounded editor compatibility for the existing automatic subset; it
does not add full Rust/compiler inference, arbitrary source discovery, or new placement modes.
