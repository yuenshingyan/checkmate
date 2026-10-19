//! Span-keyed rewriting of resolved syntax, never a search-and-replace of names.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::LineColumn;
use quote::ToTokens;
use syn::visit_mut::{self, VisitMut};

use crate::resolve::catalog::{error, item_ident};
use crate::resolve::declarations::Resolved;
use crate::resolve::graph::NodeId;
use crate::source::ast::SourceRange;

type Key = (LineColumn, LineColumn);
fn key(range: SourceRange) -> Key {
    (range.start, range.end)
}

pub(super) struct Rewriter<'a> {
    names: &'a BTreeMap<NodeId, syn::Ident>,
    pending: BTreeMap<Key, Resolved>,
    failure: Option<syn::Error>,
}

impl<'a> Rewriter<'a> {
    pub fn new(names: &'a BTreeMap<NodeId, syn::Ident>) -> Self {
        Self {
            names,
            pending: BTreeMap::new(),
            failure: None,
        }
    }

    pub fn add(&mut self, range: SourceRange, target: Resolved) -> syn::Result<()> {
        if let Some(previous) = self.pending.insert(key(range), target.clone())
            && previous != target
        {
            return Err(error(
                "one reference location has conflicting declaration identities",
            ));
        }
        Ok(())
    }

    fn replace(&self, path: &mut syn::Path, target: Resolved) -> syn::Result<()> {
        let (id, member) = match target {
            Resolved::Binding => return Ok(()),
            Resolved::Original {
                declaration,
                member,
                ..
            }
            | Resolved::LocalType {
                declaration,
                member,
            } => (NodeId::Source(declaration), member),
            Resolved::External(path) => (NodeId::External(path), None),
        };
        let name = self.names.get(&id).ok_or_else(|| {
            error("resolved declaration is absent from the completed promotion graph")
        })?;
        let mut replacement = syn::Path::from(name.clone());
        if let Some(member) = member {
            let ident =
                syn::parse_str(&member).or_else(|_| syn::parse_str(&format!("r#{member}")))?;
            replacement.segments.push(syn::PathSegment {
                ident,
                arguments: syn::PathArguments::None,
            });
        }
        // Qualified roots may grow or shrink. Keep arguments on the corresponding
        // final type/variant segments after recursively rewriting their children.
        // Never silently discard arguments attached to a removed prefix.
        let keep = path.segments.len().min(replacement.segments.len());
        let drop = path.segments.len() - keep;
        if path
            .segments
            .iter()
            .take(drop)
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(error(
                "generic arguments on a removed path prefix require additional associated-item analysis",
            ));
        }
        let offset = replacement.segments.len() - keep;
        for (old, new) in path
            .segments
            .iter()
            .skip(drop)
            .zip(replacement.segments.iter_mut().skip(offset))
        {
            new.arguments = old.arguments.clone();
        }
        *path = replacement;
        Ok(())
    }

    pub fn finish(self) -> syn::Result<()> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.pending.is_empty() {
            return Err(error(
                "not every resolved reference was rewritten; refusing a partial promotion plan",
            ));
        }
        Ok(())
    }
}

impl VisitMut for Rewriter<'_> {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        let site = key(SourceRange::of(path));
        visit_mut::visit_path_mut(self, path);
        if let Some(target) = self.pending.remove(&site)
            && let Err(error) = self.replace(path, target)
        {
            self.failure.get_or_insert(error);
        }
    }

    fn visit_pat_mut(&mut self, pattern: &mut syn::Pat) {
        if let syn::Pat::Ident(ident) = pattern {
            let site = key(SourceRange::of(&ident.ident));
            if let Some(target) = self.pending.remove(&site)
                && target != Resolved::Binding
            {
                if ident.by_ref.is_some() || ident.mutability.is_some() || ident.subpat.is_some() {
                    self.failure.get_or_insert_with(|| {
                        error(
                            "binding syntax cannot be rewritten as a constant/constructor pattern",
                        )
                    });
                    return;
                }
                let mut path = syn::Path::from(ident.ident.clone());
                if let Err(error) = self.replace(&mut path, target) {
                    self.failure.get_or_insert(error);
                    return;
                }
                *pattern = syn::Pat::Path(syn::PatPath {
                    attrs: ident.attrs.clone(),
                    qself: None,
                    path,
                });
                return;
            }
        }
        visit_mut::visit_pat_mut(self, pattern);
    }

    fn visit_field_value_mut(&mut self, field: &mut syn::FieldValue) {
        let before = field.expr.to_token_stream().to_string();
        visit_mut::visit_field_value_mut(self, field);
        if field.colon_token.is_none() && before != field.expr.to_token_stream().to_string() {
            field.colon_token = Some(Default::default());
        }
    }

    fn visit_field_pat_mut(&mut self, field: &mut syn::FieldPat) {
        let before = field.pat.to_token_stream().to_string();
        visit_mut::visit_field_pat_mut(self, field);
        if field.colon_token.is_none() && before != field.pat.to_token_stream().to_string() {
            field.colon_token = Some(Default::default());
        }
    }
}

pub(super) fn remove_promoted(
    function: &mut syn::ItemFn,
    pending: BTreeSet<Key>,
) -> syn::Result<()> {
    struct Remove {
        pending: BTreeSet<Key>,
    }
    impl VisitMut for Remove {
        fn visit_block_mut(&mut self, block: &mut syn::Block) {
            block.stmts.retain(|statement| {
                if let syn::Stmt::Item(item) = statement
                    && matches!(
                        item,
                        syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Type(_)
                    )
                    && let Some(ident) = item_ident(item)
                {
                    return !self
                        .pending
                        .remove(&(ident.span().start(), ident.span().end()));
                }
                true
            });
            visit_mut::visit_block_mut(self, block);
        }
    }
    let mut visitor = Remove { pending };
    visitor.visit_item_fn_mut(function);
    if !visitor.pending.is_empty() {
        return Err(error(
            "a selected promoted declaration was not removed from the generated copy",
        ));
    }
    Ok(())
}
