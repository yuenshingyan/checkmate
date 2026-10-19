//! Checked inherent implementations for copied nominal types. No trait inference.
use crate::diagnostics::{Error, Kind, Result};
mod methods;

use quote::ToTokens;
use std::collections::BTreeMap;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::catalog::error;
use super::declarations::{Resolved, Resolver};
use super::graph::Dependency;
use super::{ReferenceOrigin, ReferenceSite, ReferenceUse, References};
use crate::scope::{DeclarationId, DeclarationKind, Lookup, Namespace};
use crate::source::ast::{Selection, SourceRange};

pub(crate) fn dependencies(
    resolver: &Resolver<'_>,
    owner: DeclarationId,
) -> Result<Vec<Dependency>> {
    let mut result = Vec::new();
    for item in resolver.implementations.get(&owner).into_iter().flatten() {
        let mut collector = Collector {
            resolver,
            references: Vec::new(),
            failure: None,
        };
        collector.visit_item_impl(item);
        if let Some(failure) = collector.failure {
            return Err(failure);
        }
        let targets = collector
            .references
            .iter()
            .map(|reference| (key(reference.range), reference.target.clone()))
            .collect();
        methods::check(
            resolver,
            resolver.catalog.index,
            &targets,
            methods::Syntax::Implementation(item, owner),
        )?;
        result.extend(collector.references);
    }
    Ok(result)
}

pub(super) fn check_root_methods(
    resolver: &Resolver<'_>,
    references: &References,
    resolved: &[Resolved],
    selection: &Selection<'_>,
) -> Result<()> {
    let targets = references
        .occurrences
        .iter()
        .zip(resolved)
        .map(|(reference, target)| (key(reference.generated_range), target.clone()))
        .collect();
    let calls = methods::check(
        resolver,
        &references.generated_index,
        &targets,
        methods::Syntax::File(&references.generated.file),
    )
    .map_err(|err| {
        let mut range = SourceRange::from_span(err.span());
        let origin = if range.start.line < references.snippet_start {
            ReferenceOrigin::Signature
        } else if range.start.line - references.snippet_start
            < selection.range.end.line - selection.range.start.line
        {
            range.start.line =
                selection.range.start.line + range.start.line - references.snippet_start;
            range.end.line = selection.range.start.line + range.end.line - references.snippet_start;
            ReferenceOrigin::Source
        } else {
            ReferenceOrigin::Output
        };
        resolver.diagnostic(selection, &ReferenceSite { origin, range }, err)
    })?;
    let source_calls = calls
        .into_iter()
        .filter_map(|(at, owner)| {
            let offset = at.line.checked_sub(references.snippet_start)?;
            (offset < selection.range.end.line - selection.range.start.line).then_some((
                proc_macro2::LineColumn {
                    line: selection.range.start.line + offset,
                    column: at.column,
                },
                owner,
            ))
        })
        .collect::<Vec<_>>();
    methods::verify_source_receivers(resolver, &source_calls).map_err(|err| {
        resolver.diagnostic(
            selection,
            &ReferenceSite {
                origin: ReferenceOrigin::Source,
                range: SourceRange::from_span(err.span()),
            },
            err,
        )
    })
}

type Key = (proc_macro2::LineColumn, proc_macro2::LineColumn);
fn key(range: SourceRange) -> Key {
    (range.start, range.end)
}
type Targets = BTreeMap<Key, Resolved>;

struct Collector<'a, 'source> {
    resolver: &'a Resolver<'source>,
    references: Vec<Dependency>,
    failure: Option<Error>,
}
impl Collector<'_, '_> {
    fn fail(&mut self, node: &impl Spanned, message: &str) {
        self.fail_error(
            node,
            Error::new(node.span(), message).with_kind(Kind::UnsafePromotion),
        );
    }
    fn fail_error(&mut self, node: &impl Spanned, failure: Error) {
        if self.failure.is_none() {
            let at = node.span().start();
            let message = format!("implementation {}:{}: {failure}", at.line, at.column + 1);
            self.failure = Some(
                failure
                    .context(message)
                    .with_site(ReferenceOrigin::Source, SourceRange::of(node)),
            );
        }
    }
    fn record(&mut self, path: &syn::Path, usage: ReferenceUse) {
        if self.failure.is_some() {
            return;
        }
        let at = path.span().start();
        let Some(scope) = self.resolver.catalog.index.scope_at(at) else {
            self.fail(path, "reference has no original scope");
            return;
        };
        let index = self.resolver.catalog.index;
        let runtime = if usage == ReferenceUse::Value
            && path.leading_colon.is_none()
            && path.segments.len() == 1
        {
            match index.lookup(
                scope,
                Namespace::Value,
                &path.segments[0].ident.to_string(),
                at,
            ) {
                Lookup::Found(id)
                    if matches!(
                        index.declarations[id.0].kind,
                        DeclarationKind::Parameter
                            | DeclarationKind::Local
                            | DeclarationKind::Pattern
                    ) =>
                {
                    Some(Ok(Resolved::Binding))
                }
                Lookup::ForbiddenCapture(_) => Some(Err(error(
                    "implementation cannot capture an enclosing runtime/generic binding",
                ))),
                _ => None,
            }
        } else {
            None
        };
        let target = runtime.unwrap_or_else(|| self.resolver.path(scope, path, usage, at));
        match target {
            Ok(target) => self.references.push(Dependency {
                path: path.to_token_stream().to_string(),
                range: SourceRange::of(path),
                usage,
                target,
                // Method signatures/bodies are behavior dependencies, never storage
                // layout or alias-RHS expansion edges, including references to Self.
                indirect: false,
                layout: false,
                expansion: false,
            }),
            Err(err) => {
                let message = format!("{}: {err}", path.to_token_stream());
                self.fail_error(path, err.context(message));
            }
        }
    }
}
impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node.qself.is_some() {
            self.fail(node, "qualified associated types require trait resolution");
            return;
        }
        self.record(&node.path, ReferenceUse::Type);
        visit::visit_type_path(self, node);
    }
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if node.qself.is_some() {
            self.fail(node, "qualified associated values require trait resolution");
            return;
        }
        self.record(&node.path, ReferenceUse::Value);
        visit::visit_expr_path(self, node);
    }
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        if node.qself.is_some() {
            self.fail(node, "qualified constructors require trait resolution");
            return;
        }
        self.record(&node.path, ReferenceUse::Constructor);
        visit::visit_expr_struct(self, node);
    }
    fn visit_pat_struct(&mut self, node: &'ast syn::PatStruct) {
        if node.qself.is_some() {
            self.fail(node, "qualified patterns require trait resolution");
            return;
        }
        self.record(&node.path, ReferenceUse::Constructor);
        visit::visit_pat_struct(self, node);
    }
    fn visit_pat_tuple_struct(&mut self, node: &'ast syn::PatTupleStruct) {
        if node.qself.is_some() {
            self.fail(node, "qualified patterns require trait resolution");
            return;
        }
        self.record(&node.path, ReferenceUse::Value);
        visit::visit_pat_tuple_struct(self, node);
    }
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        if !self
            .resolver
            .catalog
            .index
            .declarations
            .iter()
            .any(|declaration| declaration.range == SourceRange::of(&node.ident))
        {
            self.record(&syn::Path::from(node.ident.clone()), ReferenceUse::Value);
        }
        visit::visit_pat_ident(self, node);
    }
    fn visit_generic_argument(&mut self, node: &'ast syn::GenericArgument) {
        if let syn::GenericArgument::Type(syn::Type::Path(path)) = node
            && path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
            && matches!(path.path.segments[0].arguments, syn::PathArguments::None)
        {
            self.record(&path.path, ReferenceUse::TypeOrConst);
        } else {
            visit::visit_generic_argument(self, node);
        }
    }
    fn visit_type(&mut self, node: &'ast syn::Type) {
        if matches!(
            node,
            syn::Type::Infer(_)
                | syn::Type::ImplTrait(_)
                | syn::Type::TraitObject(_)
                | syn::Type::Macro(_)
                | syn::Type::Verbatim(_)
        ) {
            self.fail(
                node,
                "opaque/inferred/trait types in copied implementations require further analysis",
            );
        } else {
            visit::visit_type(self, node);
        }
    }
    fn visit_lifetime(&mut self, node: &'ast syn::Lifetime) {
        if node.ident != "static" && node.ident != "_" {
            self.fail(
                node,
                "named lifetime context in implementations is unsupported",
            );
        }
    }
    fn visit_trait_bound(&mut self, node: &'ast syn::TraitBound) {
        self.fail(
            node,
            "trait bounds in copied implementations are unsupported",
        );
    }
    fn visit_item(&mut self, node: &'ast syn::Item) {
        self.fail(node, "nested items/imports in copied implementation bodies require independent relocation analysis");
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.fail(
            node,
            "macros in copied implementations require expansion analysis",
        );
    }
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if !(node.path().is_ident("doc")
            && matches!(&node.meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(value) if matches!(value.lit, syn::Lit::Str(_)))))
        {
            self.fail(
                node,
                "copied implementations support only literal documentation attributes",
            );
        }
    }
}
