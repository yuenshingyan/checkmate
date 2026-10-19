//! Borrow original named-item syntax without copying or changing declarations.

use crate::diagnostics::{Error, Kind, Result};
use std::collections::BTreeMap;

use proc_macro2::{LineColumn, Span};
use syn::visit::{self, Visit};

use crate::scope::{DeclarationId, DeclarationKind, ScopeId, ScopeIndex, ScopeKind};
use crate::source::ast::{ParsedSource, SourceRange};

pub(crate) struct Catalog<'a> {
    pub index: &'a ScopeIndex,
    pub source: &'a ParsedSource,
    pub implementations: Vec<&'a syn::ItemImpl>,
    items: Vec<Option<&'a syn::Item>>,
}

impl<'a> Catalog<'a> {
    pub fn new(source: &'a ParsedSource, index: &'a ScopeIndex) -> Result<Self> {
        if source.path != index.source_path {
            return Err(
                error("catalog and lexical index belong to different source files")
                    .with_kind(Kind::InternalInvariant),
            );
        }
        struct Items<'a> {
            identities: BTreeMap<(LineColumn, LineColumn, String), usize>,
            items: Vec<Option<&'a syn::Item>>,
            implementations: Vec<&'a syn::ItemImpl>,
        }
        impl<'ast> Visit<'ast> for Items<'ast> {
            fn visit_item(&mut self, item: &'ast syn::Item) {
                if let Some(ident) = item_ident(item) {
                    let range = SourceRange::of(ident);
                    let key = (
                        range.start,
                        range.end,
                        canonical(&ident.to_string()).to_owned(),
                    );
                    if let Some(&id) = self.identities.get(&key) {
                        self.items[id] = Some(item);
                    }
                }
                if let syn::Item::Impl(item) = item {
                    self.implementations.push(item);
                }
                visit::visit_item(self, item);
            }
        }
        let mut visitor = Items {
            identities: index
                .declarations
                .iter()
                .enumerate()
                .map(|(id, item)| ((item.range.start, item.range.end, item.name.clone()), id))
                .collect(),
            items: vec![None; index.declarations.len()],
            implementations: Vec::new(),
        };
        visitor.visit_file(&source.file);
        Ok(Self {
            index,
            source,
            items: visitor.items,
            implementations: visitor.implementations,
        })
    }

    pub fn nominal_at(&self, at: LineColumn) -> Option<DeclarationId> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(id, item)| {
                let item = (*item)?;
                if !matches!(item, syn::Item::Struct(_) | syn::Item::Enum(_)) {
                    return None;
                }
                let range = SourceRange::of(item);
                (range.start <= at && at < range.end).then_some((range.start, DeclarationId(id)))
            })
            .max_by_key(|(start, _)| *start)
            .map(|(_, id)| id)
    }

    pub fn item(&self, id: DeclarationId) -> Result<&'a syn::Item> {
        self.items
            .get(id.0)
            .and_then(|item| *item)
            .ok_or_else(|| error("declaration is not an indexed named item"))
    }

    pub fn module_scope(&self, id: DeclarationId) -> Result<ScopeId> {
        let declaration = &self.index.declarations[id.0];
        self.index
            .scopes
            .iter()
            .position(|scope| {
                scope.kind == ScopeKind::Module
                    && scope.parent == Some(declaration.scope)
                    && scope.range.contains(declaration.range)
            })
            .map(ScopeId)
            .ok_or_else(|| error("module body is unavailable in the same-file catalog"))
    }

    pub fn global_path(&self, id: DeclarationId) -> Result<Option<Vec<String>>> {
        let declaration = &self.index.declarations[id.0];
        if matches!(
            self.index.scopes[declaration.scope.0].kind,
            ScopeKind::File | ScopeKind::Module
        ) {
            let mut path = self.index.module_path(declaration.scope)?;
            path.push(declaration.name.clone());
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    pub fn item_is_public(&self, id: DeclarationId) -> Result<bool> {
        Ok(matches!(
            item_visibility(self.item(id)?),
            Some(syn::Visibility::Public(_))
        ))
    }

    // Checks only a syntactic route, never fields, trait methods, or type privacy.
    // rustc remains authoritative for all generated references.
    pub fn public_route(&self, route: &[String]) -> bool {
        let mut scope = ScopeId(0);
        for (offset, name) in route.iter().enumerate() {
            let last = offset + 1 == route.len();
            if last {
                return self
                    .index
                    .declarations
                    .iter()
                    .enumerate()
                    .any(|(id, item)| {
                        item.scope == scope
                            && item.name == *name
                            && self.item_is_public(DeclarationId(id)).unwrap_or(false)
                    })
                    || self.index.imports.iter().any(|import| {
                        import.scope == scope && import.alias == *name && import.visibility == "pub"
                    });
            }
            if let Some((id, declaration)) =
                self.index
                    .declarations
                    .iter()
                    .enumerate()
                    .find(|(_, item)| {
                        item.scope == scope
                            && item.name == *name
                            && item.kind == DeclarationKind::Module
                    })
            {
                let id = DeclarationId(id);
                if !self.item_is_public(id).unwrap_or(false) {
                    return false;
                }
                if declaration.kind != DeclarationKind::Module {
                    return false;
                }
                let Ok(child) = self.module_scope(id) else {
                    return false;
                };
                scope = child;
            } else {
                return false;
            }
        }
        false
    }
}

pub(crate) fn item_ident(item: &syn::Item) -> Option<&syn::Ident> {
    match item {
        syn::Item::Fn(item) => Some(&item.sig.ident),
        syn::Item::Struct(item) => Some(&item.ident),
        syn::Item::Enum(item) => Some(&item.ident),
        syn::Item::Type(item) => Some(&item.ident),
        syn::Item::Const(item) => Some(&item.ident),
        syn::Item::Static(item) => Some(&item.ident),
        syn::Item::Mod(item) => Some(&item.ident),
        syn::Item::Macro(item) => item.ident.as_ref(),
        _ => None,
    }
}

pub(crate) fn item_visibility(item: &syn::Item) -> Option<&syn::Visibility> {
    match item {
        syn::Item::Fn(item) => Some(&item.vis),
        syn::Item::Struct(item) => Some(&item.vis),
        syn::Item::Enum(item) => Some(&item.vis),
        syn::Item::Type(item) => Some(&item.vis),
        syn::Item::Const(item) => Some(&item.vis),
        syn::Item::Static(item) => Some(&item.vis),
        syn::Item::Mod(item) => Some(&item.vis),
        _ => None,
    }
}

pub(crate) fn canonical(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

pub(crate) fn error(message: &str) -> Error {
    Error::new(Span::call_site(), message)
}
