//! Generate test-only functions from unmodified source snippets.
//!
//! Use inline options, or `extract_fn!(config = "checkmate.toml", snippet = "name")`.
//! Line numbers are one-based and inclusive. Snippet snapshots guard against stale
//! ranges by default without adding anything to the original source; `snapshot = false`
//! explicitly opts out.

mod arguments;
mod config;
mod generate;
mod snapshot;
mod source;

mod automatic;
mod diagnostics;
mod resolve;
mod scope;

use std::path::Path;

use arguments::Invocation;
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Extract a source range into a function available only under `cfg(test)`.
///
/// Omitted `context` defaults to automatic discovery; `context = "auto"` remains valid.
/// Set `context = "explicit"` for ordinary extraction and supply required context in
/// the test scope. Automatic discovery requires a verified library root and a literal
/// fully qualified invocation at a package
/// integration-test crate root (Rust 1.88+ and Cargo required). Editor expansion
/// additionally needs physical source locations from a compatible macro host;
/// see `docs/editor-support.md` for the tested rust-analyzer/server configuration.
///
/// Bare imported macro names, binary sources, and unit-test ordinary usage require
/// `context = "explicit"`. For existing ordinary usage, add it to inline metadata or
/// the TOML snippet definition, never alongside `config`/`snippet` in a call. Keep
/// selected production snippets, source macro definitions, and snapshot contents
/// unchanged during this migration; no source annotations are needed.
///
/// Both modes require your explicit typed inputs, inclusive line range, and tests;
/// you also choose an optional output expression. Earlier initializers are never executed or captured. Automatic mode
/// is not full type/trait inference: generic local context, macro expansion, and
/// general method dispatch remain unsupported. Copied non-generic inherent methods
/// are checked even when unused; dot-call support requires a proven owned nominal
/// receiver and inherent by-value `self` method.
///
/// Reused accessible originals retain identity and privacy. Copied function-local
/// structs/enums have new nominal identity, not interoperability with the original;
/// aliases remain transparent to their rewritten targets. Unsupported discovery
/// errors rather than falling back or bypassing privacy.
///
/// In both modes, omitted `snapshot` guards selected source text with
/// `snapshots/<generated-function-name>.rs` from a valid parsed `fn_sig`, independent
/// of the config snippet key, source module, or ranges. Raw `r#type` maps to `type.rs`;
/// Unicode identifiers are retained. Deriving the default requires a valid `fn_sig`.
/// An explicit `snapshot = "path"` wins; `snapshot = false` disables it. `true` and empty strings
/// are invalid. Same names in one path base share a default: use explicit distinct
/// paths for different bodies. Missing defaults fail with the exact resolved path;
/// missing/mismatching guards never fall back to unchecked extraction.
///
/// Previously unchecked omissions now require a manually created/reviewed default
/// baseline, a retained explicit custom path, or `snapshot = false`. Do not edit
/// original source to satisfy a guard. Snippet guards are not whole-program fingerprints
/// and do not automatically relocate ranges.
///
/// Automatic mode additionally accepts an optional versioned `context_snapshot`
/// (no default) and `context_report = "invoice_context"`. Both are valid with omitted
/// `context` and rejected with `context = "explicit"`. To guard context without
/// guarding the snippet, explicitly set `snapshot = false`.
/// `context_report` emits an accessor returning a TOML literal, not a printer or writer.
/// Print it explicitly in a user-authored test, manually review/copy the TOML, then
/// enable/restore the guard. Context baselines cover the supported discovery graph,
/// not whole-program equivalence. Compilation never modifies source or config, nor
/// creates, writes, updates, approves, or prints baseline files.
///
/// Inline paths and the `config` path are invoking-package-relative; paths inside
/// TOML definitions are relative to that configuration file's directory. Default
/// snippet paths use these same bases as explicit paths. Do not mix inline
/// options with `config`/`snippet`.
///
/// The following are alternative contextual fragments, not one standalone doctest.
/// Choose the corresponding package/example; do not paste all invocations into one file.
/// Ordinary and automatic workflows are tested in the workspace's `examples/basic`
/// and `examples/automatic-context` packages. From the repository root, run
/// `cargo test -p checkmate-auto-example`; to print review data explicitly, run
/// `cargo test -p checkmate-auto-example --test automatic print_context_for_review -- --ignored --nocapture`.
/// See `docs/usage.md`, `docs/context-snapshots.md`, and `docs/diagnostics.md` for
/// limits, manual review, and CM004/CM006/CM011/CM013 remedies without source edits.
///
/// ```ignore
/// use checkmate::extract_fn;
///
/// extract_fn! {
///     context = "explicit",
///     fn_sig = "fn add_if(a: f32, b: f32) -> f32",
///     module = "src/main.rs",
///     start = 7,
///     end = 11,
///     output = c,
///     snapshot = "snapshots/add_if.rs",
/// }
///
/// // Alternatively, load a TOML definition containing context = "explicit":
/// extract_fn!(config = "checkmate.toml", snippet = "add_if");
///
/// // Automatic example: at the root of examples/automatic-context/tests/automatic.rs.
/// // Its config supplies context = "auto", explicit inputs/range/output, and guards.
/// checkmate::extract_fn!(config = "metadata/snippets.toml", snippet = "invoice");
/// ```
#[proc_macro]
pub fn extract_fn(input: TokenStream) -> TokenStream {
    // proc_macro2's file-parser backend switch is global to this macro library.
    // Keep even the ordinary path outside another expansion's analysis window.
    static EXPANSION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _expansion = EXPANSION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let caller = automatic::Caller::capture(&input);
    let invocation = parse_macro_input!(input as Invocation);
    let result = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| syn::Error::new(invocation.span(), "CARGO_MANIFEST_DIR is not set"))
        .and_then(|root| generate::expand_with_caller(invocation, Path::new(&root), Some(caller)));
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod walkthroughs;
