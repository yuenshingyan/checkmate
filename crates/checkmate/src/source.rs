//! Source-file IO and bounded diagnostics. No source is modified here.

// The automatic-context foundation stays test-only until task 011 wires the
// opt-in pipeline. Ordinary extraction must still accept statement-only files.
#[cfg(test)]
pub(crate) mod ast;
#[cfg(test)]
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
