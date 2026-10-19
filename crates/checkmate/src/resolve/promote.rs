//! Test-gated local-type promotion plans. No original source or baseline is written.
//! Invocation placement and public integration remain task 011 responsibilities.

use crate::diagnostics::{Error, Result};
mod rewrite;

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::visit::{self, Visit};
use syn::visit_mut::VisitMut;

use super::References;
use super::catalog::canonical;
use crate::diagnostics::{Kind, Origin};
use syn::spanned::Spanned;
fn error(message: &str) -> Error {
    super::catalog::error(message).with_kind(Kind::UnsafePromotion)
}
use super::declarations::Resolver;
use super::graph::{Graph, LocalKind, NodeId, NodeKind};
use crate::scope::DeclarationId;
use crate::source::ast::{Selection, SourceRange};

pub(crate) struct PromotionPlan {
    pub items: Vec<syn::Item>,
    pub names: BTreeMap<NodeId, syn::Ident>,
    // Explicit disclosure, not a claim of interoperability with original locals.
    // Transparent aliases follow their targets; only structs/enums are nominal.
    pub copied_nominal_types: BTreeSet<DeclarationId>,
}

impl PromotionPlan {
    pub fn build(
        resolver: &Resolver<'_>,
        references: &References,
        selection: &Selection<'_>,
        occupied: &BTreeSet<String>,
    ) -> Result<Self> {
        let graph = Graph::build(resolver, references, selection)?;
        let [syn::Item::Fn(function)] = references.generated.file.items.as_slice() else {
            return Err(error(
                "promotion requires the collected single-function analysis shell",
            ));
        };
        validate(function).map_err(|err| {
            let site = super::analysis_site(
                SourceRange::from_span(err.span()),
                selection.range,
                references.snippet_start,
            );
            let err = err.with_site(site.origin, site.range);
            if site.origin == Origin::Source {
                for (id, node) in &graph.nodes {
                    if matches!(node.kind, NodeKind::Local(_))
                        && let NodeId::Source(declaration) = id
                        && let Ok(item) = resolver.catalog.item(*declaration)
                        && SourceRange::of(item).contains(site.range)
                    {
                        return graph.diagnostic(resolver, selection.range, id, err);
                    }
                }
            }
            err
        })?;
        let function_name = canonical(&function.sig.ident.to_string()).to_owned();
        if occupied.iter().any(|name| canonical(name) == function_name) {
            return Err(error(
                "extracted function name conflicts with a known invocation identifier",
            ));
        }
        let mut reserved: BTreeSet<_> = occupied
            .iter()
            .map(|name| canonical(name).to_owned())
            .collect();
        reserved.extend(
            resolver
                .catalog
                .index
                .declarations
                .iter()
                .map(|item| item.name.clone()),
        );
        reserved.extend(
            resolver
                .catalog
                .index
                .imports
                .iter()
                .map(|import| import.alias.clone()),
        );
        struct Identifiers<'a>(&'a mut BTreeSet<String>);
        impl<'ast> Visit<'ast> for Identifiers<'_> {
            fn visit_ident(&mut self, ident: &'ast syn::Ident) {
                self.0.insert(canonical(&ident.to_string()).to_owned());
            }
        }
        Identifiers(&mut reserved).visit_item_fn(function);
        // Length-prefixed byte encoding is injective, including raw/unicode names.
        // A local type/import in one extraction cannot collide with a peer's names
        // merely because concatenated function/declaration spellings look alike.
        let encoded: String = function_name
            .bytes()
            .map(|byte| format!("{byte:02X}"))
            .collect();
        let prefix = format!("CheckmateF{}X{encoded}", function_name.len());
        let mut names = BTreeMap::new();
        for (external_index, id) in graph.emission_order.iter().enumerate() {
            let suffix = match id {
                NodeId::Source(id) => format!("D{}", id.0),
                NodeId::External(_) => format!("E{external_index}"),
            };
            let name = format!("{prefix}{suffix}");
            if !reserved.insert(name.clone()) {
                return Err(graph.diagnostic(resolver, selection.range, id, error(&format!(
                    "generated name `{name}` conflicts with a known identifier; choose another extraction name"
                ))));
            }
            names.insert(id.clone(), syn::Ident::new(&name, Span::call_site()));
        }
        let mut items = Vec::new();
        let mut copied_nominal_types = BTreeSet::new();
        let mut removals = BTreeSet::new();
        for id in &graph.emission_order {
            let node = &graph.nodes[id];
            let name = &names[id];
            (|| -> Result<()> {
            match &node.kind {
                NodeKind::Original(path) => items.push(syn::parse2(path.import_as(name)?)?),
                NodeKind::External => {
                    let NodeId::External(path) = id else {
                        return Err(error("external graph node has no qualified path"));
                    };
                    items.push(syn::parse2(path.import_as(name)?)?);
                }
                NodeKind::Local(kind) => {
                    let NodeId::Source(declaration) = id else {
                        return Err(error("local graph node has no declaration identity"));
                    };
                    let mut item = resolver.catalog.item(*declaration)?.clone();
                    validate(&item).map_err(|err| {
                        let entry = &resolver.catalog.index.declarations[declaration.0];
                        let range = SourceRange::from_span(err.span());
                        err.with_site(Origin::Source, range)
                            .with_subject(&entry.name, Some(entry.kind))
                    })?;
                    let mut rewriter = rewrite::Rewriter::new(&names);
                    for reference in &node.references {
                        if SourceRange::of(&item).contains(reference.range) {
                            rewriter.add(reference.range, reference.target.clone())?;
                        }
                    }
                    rewriter.visit_item_mut(&mut item);
                    rewriter.finish()?;
                    match &mut item {
                        syn::Item::Struct(item) => item.ident = name.clone(),
                        syn::Item::Enum(item) => item.ident = name.clone(),
                        syn::Item::Type(item) => item.ident = name.clone(),
                        _ => return Err(error("unsupported declaration reached promotion")),
                    }
                    if *kind != LocalKind::Alias {
                        copied_nominal_types.insert(*declaration);
                    }
                    let range = resolver.catalog.index.declarations[declaration.0].range;
                    if selection.range.contains(range) {
                        let original = resolver.catalog.item(*declaration)?;
                        if !selection.range.contains(SourceRange::of(original)) {
                            return Err(error(
                                "a promoted declaration must be wholly inside or outside the selection",
                            ));
                        }
                        let shift = |at: proc_macro2::LineColumn| proc_macro2::LineColumn {
                            line: references.snippet_start + at.line - selection.range.start.line,
                            column: at.column,
                        };
                        removals.insert((shift(range.start), shift(range.end)));
                    }
                    items.push(item);
                    for implementation in resolver
                        .implementations
                        .get(declaration)
                        .into_iter()
                        .flatten()
                    {
                        let range = SourceRange::of(*implementation);
                        let mut item = syn::Item::Impl((*implementation).clone());
                        validate(&item).map_err(|err| {
                            let entry = &resolver.catalog.index.declarations[declaration.0];
                            let range = SourceRange::from_span(err.span());
                            err.with_site(Origin::Source, range)
                                .with_subject(&entry.name, Some(entry.kind))
                        })?;
                        let mut rewriter = rewrite::Rewriter::new(&names);
                        for reference in &node.references {
                            if range.contains(reference.range) {
                                rewriter.add(reference.range, reference.target.clone())?;
                            }
                        }
                        rewriter.visit_item_mut(&mut item);
                        rewriter.finish()?;
                        items.push(item);
                    }
                }
            }
            Ok(())
            })().map_err(|err| graph.diagnostic(resolver, selection.range, id, err))?;
        }
        let mut function = function.clone();
        let mut rewriter = rewrite::Rewriter::new(&names);
        for (reference, target) in references.occurrences.iter().zip(&graph.root_references) {
            rewriter.add(reference.generated_range, target.clone())?;
        }
        rewriter.visit_item_fn_mut(&mut function);
        rewriter.finish()?;
        // Lift definitions out of the generated copy only. Leaving a second
        // definition in the body would introduce another nominal identity.
        rewrite::remove_promoted(&mut function, removals)?;
        items.push(syn::Item::Fn(function));
        Ok(Self {
            items,
            names,
            copied_nominal_types,
        })
    }

    pub fn tokens(&self) -> TokenStream {
        let items = &self.items;
        quote! { #( #[cfg(test)] #items )* }
    }
}

// Relocated visibility restrictions and semantic attributes can change meaning.
// Preserve inherited/public visibility verbatim; never add `pub` to a field.
fn validate(node: &impl Inspect) -> Result<()> {
    let mut policy = Policy { failure: None };
    node.inspect(&mut policy);
    match policy.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

trait Inspect {
    fn inspect(&self, policy: &mut Policy);
}
impl Inspect for syn::Item {
    fn inspect(&self, policy: &mut Policy) {
        policy.visit_item(self);
    }
}
impl Inspect for syn::ItemFn {
    fn inspect(&self, policy: &mut Policy) {
        policy.visit_item_fn(self);
    }
}

struct Policy {
    failure: Option<Error>,
}
impl<'ast> Visit<'ast> for Policy {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.failure.get_or_insert_with(|| Error::new(item.span(), "use declarations inside the selected code/output require import-relocation analysis; select statements after surrounding imports").with_kind(Kind::UnsafePromotion));
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if !(attribute.path().is_ident("doc")
            && matches!(&attribute.meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(value) if matches!(value.lit, syn::Lit::Str(_)))))
        {
            self.failure.get_or_insert_with(|| Error::new(attribute.span(), "relocated context supports only literal documentation attributes; semantic attributes and doc macros require analysis").with_kind(Kind::UnsafePromotion));
        }
    }
    fn visit_visibility(&mut self, visibility: &'ast syn::Visibility) {
        if matches!(visibility, syn::Visibility::Restricted(_)) {
            self.failure.get_or_insert_with(|| Error::new(visibility.span(), "restricted visibility changes meaning when relocating local context; visibility is never widened").with_kind(Kind::UnsafePromotion));
        }
        visit::visit_visibility(self, visibility);
    }
}

#[cfg(test)]
mod tests;
