//! Source-file IO and bounded diagnostics. No source is modified here.

// Ordinary extraction still accepts statement-only files; only automatic mode
// requires whole-file syntax and verified library/caller identity.
pub(crate) mod ast;
pub(crate) mod origin;

use std::path::Path;

use proc_macro2::Span;
use syn::LitStr;

pub(crate) fn read_text(path: &Path, span: Span) -> syn::Result<String> {
    std::fs::read_to_string(path)
        .map_err(|error| syn::Error::new(span, format!("cannot read {}: {error}", path.display())))
}

pub(crate) fn path_literal(path: &Path, span: Span) -> syn::Result<LitStr> {
    let text = path.to_str().ok_or_else(|| {
        syn::Error::new(
            span,
            format!("path must be valid UTF-8: {}", path.display()),
        )
    })?;
    Ok(LitStr::new(text, span))
}

pub(crate) fn preview(lines: &[&str], start: usize) -> String {
    let mut text = lines
        .iter()
        .take(12)
        .enumerate()
        .map(|(offset, line)| format!("  {} | {}", start + offset, bounded_line(line)))
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > 12 {
        text.push_str("\n  ... (remaining selected lines omitted)");
    }
    text
}

pub(crate) fn bounded_line(line: &str) -> String {
    let mut chars = line.chars();
    let mut result: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}
