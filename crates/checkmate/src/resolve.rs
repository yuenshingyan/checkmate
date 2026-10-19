//! Scope-aware reference collection and conservative original-declaration resolution.
//!
//! Collection records deferred operations; resolution either proves a reusable
//! path/local identity or reports unsupported context. Nothing infers input types,
//! runs initializers, or copies original declarations. Import emission is test-only.

pub(crate) mod catalog;
mod collect;
pub(crate) mod declarations;

use std::collections::BTreeSet;

use quote::ToTokens;
use syn::{Expr, Signature};

use crate::scope::imports::ImportId;
use crate::scope::{DeclarationId, ScopeIndex};
use crate::source::ast::{ParsedSource, Selection, SourceRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceUse {
    Type,
    Constructor,
    Value,
    TypeOrConst,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceOrigin {
    Source,
    Signature,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceSite {
    pub origin: ReferenceOrigin,
    // Source references use original-file positions. Signature/output positions
    // refer to the normalized analysis shell, never to the original source file.
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
// A target identifies the lexical root, not the final member of a qualified path.
// Remaining segments, namespace ambiguity, and accessibility belong to task 006.
pub(crate) enum ReferenceTarget {
    SnippetLocal(DeclarationId),
    Input(String),
    Declaration(DeclarationId),
    Import(ImportId),
    GeneratedLocal(DeclarationId),
    GeneratedImport(ImportId),
    Deferred,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Reference {
    pub path: String,
    pub root: String,
    pub usage: ReferenceUse,
    pub target: ReferenceTarget,
    pub site: ReferenceSite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeferredKind {
    Method,
    QualifiedTrait,
    QualifiedPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeferredOperation {
    pub kind: DeferredKind,
    pub description: String,
    pub site: ReferenceSite,
}

pub(crate) struct References {
    pub occurrences: Vec<Reference>,
    pub required_inputs: BTreeSet<String>,
    pub deferred_operations: Vec<DeferredOperation>,
    // Retain the shell and index so Generated* IDs and their locations have an
    // explicit owner for later graph/generation passes. Original IDs belong to
    // the caller's source index.
    pub generated: ParsedSource,
    pub generated_index: ScopeIndex,
}

pub(crate) fn collect_references(
    source: &ParsedSource,
    index: &ScopeIndex,
    selection: &Selection<'_>,
    signature: &Signature,
    output: Option<&Expr>,
) -> syn::Result<References> {
    if index.source_path != source.path {
        return Err(syn::Error::new(
            signature.ident.span(),
            "reference collection index does not belong to the selected source",
        ));
    }
    let start = selection.range.start.line;
    let end = selection.range.end.line - 1;
    let selected: Vec<_> = source
        .text
        .lines()
        .skip(start - 1)
        .take(end - start + 1)
        .collect();
    let prefix = format!("{} {{\n", signature.to_token_stream());
    let snippet_start = prefix.chars().filter(|&ch| ch == '\n').count() + 1;
    let snippet_end = snippet_start + selected.len() - 1;
    let text = format!(
        "{prefix}{}\n{}\n}}",
        selected.join("\n"),
        output
            .map(|expr| expr.to_token_stream().to_string())
            .unwrap_or_default()
    );
    let context_error = |error: syn::Error| {
        syn::Error::new(
            error.span(),
            format!(
                "{}:{start}-{end}: cannot analyze supplied signature/snippet/output together: {error}",
                source.path.display(),
            ),
        )
    };
    let generated = ParsedSource::parse("<extracted analysis>", text).map_err(context_error)?;
    let pattern_constants = collect::pattern_constants(index, selection, snippet_start);
    let generated_index = ScopeIndex::build_with_pattern_constants(&generated, &pattern_constants)
        .map_err(context_error)?;
    let collected = collect::visit(
        source,
        index,
        selection,
        &generated,
        &generated_index,
        snippet_start,
        snippet_end,
    )?;
    Ok(References {
        occurrences: collected.occurrences,
        required_inputs: collected.required_inputs,
        deferred_operations: collected.deferred_operations,
        generated,
        generated_index,
    })
}

#[cfg(test)]
mod tests;
