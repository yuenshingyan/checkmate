//! Generate test-only functions from unmodified source snippets.
//!
//! Use inline options, or `extract_fn!(config = "checkmate.toml", snippet = "name")`.
//! Line numbers are one-based and inclusive. Optional snapshot files guard against
//! stale ranges without adding anything to the original source.

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
/// Omission of `context` keeps ordinary extraction. `context = "auto"` discovers
/// supported context only for verified library roots and literal fully qualified
/// invocations at package integration-test roots. Copied function-local structs
/// and enums have new nominal identity; original declarations retain theirs.
/// Source, configuration, and snapshots are never modified by compilation.
///
/// ```ignore
/// use checkmate::extract_fn;
///
/// extract_fn! {
///     fn_sig = "fn add_if(a: f32, b: f32) -> f32",
///     module = "src/main.rs",
///     start = 7,
///     end = 11,
///     output = c,
///     snapshot = "snapshots/add_if.rs",
/// }
///
/// // Alternatively, load a named definition from an external TOML file:
/// extract_fn!(config = "checkmate.toml", snippet = "other_snippet");
/// ```
#[proc_macro]
pub fn extract_fn(input: TokenStream) -> TokenStream {
    // proc_macro2's file-parser backend switch is global to this macro library.
    // Keep even the ordinary path outside another expansion's analysis window.
    static EXPANSION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _expansion = EXPANSION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let caller = automatic::Caller::capture(input.to_string());
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
