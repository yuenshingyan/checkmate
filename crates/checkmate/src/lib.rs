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

// Automatic-context foundations are not enabled in the public macro until task 011.
#[cfg(test)]
mod diagnostics;
#[cfg(test)]
mod resolve;
#[cfg(test)]
mod scope;

use std::path::Path;

use arguments::Invocation;
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Extract a source range into a function available only under `cfg(test)`.
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
    let invocation = parse_macro_input!(input as Invocation);
    let result = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| syn::Error::new(invocation.span(), "CARGO_MANIFEST_DIR is not set"))
        .and_then(|root| generate::expand(invocation, Path::new(&root)));
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod walkthroughs;
