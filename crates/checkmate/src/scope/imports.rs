//! Explicit imports and source-relative path provenance, not visibility or type proof.

use proc_macro2::{LineColumn, Span};
use quote::ToTokens;
use syn::spanned::Spanned;

use super::*;
use crate::source::origin::SourceOrigin;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImportId(pub usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImportPath {
    pub absolute: bool,
    // Retain original identifier spelling, including raw identifiers.
    pub segments: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Import {
    pub scope: ScopeId,
    pub alias: String,
    pub path: ImportPath,
    pub type_only: bool,
    pub visibility: String,
    pub range: SourceRange,
}

impl Import {
    pub(super) fn could_bind(&self, namespace: Namespace) -> bool {
        !self.type_only || namespace == Namespace::Type
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PathOrigin {
    // Source crate, not necessarily the crate containing the macro invocation.
    Crate(Vec<String>),
    // Extern-prelude candidate, not proof it is enabled or visible.
    External(Vec<String>),
    // A block-local named item cannot be assigned a reusable crate path.
    Declaration {
        id: DeclarationId,
        suffix: Vec<String>,
    },
}

impl super::build::Builder {
    pub(super) fn register_import(&mut self, node: &syn::ItemUse) {
        let mut leaves = Vec::new();
        if let Err(error) = flatten(&node.tree, Vec::new(), &mut leaves) {
            self.fail(error.span(), &error.to_string());
            return;
        }
        for (alias, segments, type_only) in leaves {
            let alias_name = canonical(&alias.to_string()).to_owned();
            let conflicts = self
                .index
                .imports
                .iter()
                .any(|import| import.scope == self.current && import.alias == alias_name)
                || self.index.scopes[self.current.0]
                    .bindings
                    .iter()
                    .any(|binding| {
                        (!type_only || binding.namespace == Namespace::Type)
                            && self.index.declarations[binding.declaration.0].name == alias_name
                    });
            if conflicts {
                self.fail(alias.span(), &format!("import `{alias_name}` overlaps an existing name; namespace disambiguation is not yet supported"));
                return;
            }
            self.index.imports.push(Import {
                scope: self.current,
                alias: alias_name,
                path: ImportPath {
                    absolute: node.leading_colon.is_some(),
                    segments,
                },
                type_only,
                visibility: node.vis.to_token_stream().to_string(),
                range: SourceRange::of(node),
            });
        }
    }
}

type Leaf = (syn::Ident, Vec<String>, bool);

fn flatten(tree: &syn::UseTree, prefix: Vec<String>, leaves: &mut Vec<Leaf>) -> syn::Result<()> {
    match tree {
        syn::UseTree::Path(node) => {
            let mut path = prefix;
            path.push(node.ident.to_string());
            flatten(&node.tree, path, leaves)?;
        }
        syn::UseTree::Name(node) => {
            let is_self = node.ident == "self";
            let mut path = prefix;
            let alias = if is_self {
                let name = path.last().ok_or_else(|| {
                    syn::Error::new(node.span(), "bare self import has no target")
                })?;
                syn::parse_str::<syn::Ident>(name)?
            } else {
                path.push(node.ident.to_string());
                node.ident.clone()
            };
            leaves.push((alias, path, is_self));
        }
        syn::UseTree::Rename(node) => {
            if node.rename == "_" {
                return Err(syn::Error::new(
                    node.span(),
                    "underscore imports may affect trait resolution and are not supported",
                ));
            }
            let is_self = node.ident == "self";
            let mut path = prefix;
            if !is_self {
                path.push(node.ident.to_string());
            }
            if path.is_empty() {
                return Err(syn::Error::new(
                    node.span(),
                    "bare self import has no target",
                ));
            }
            leaves.push((node.rename.clone(), path, is_self));
        }
        syn::UseTree::Group(node) => {
            for tree in &node.items {
                flatten(tree, prefix.clone(), leaves)?;
            }
        }
        syn::UseTree::Glob(node) => {
            return Err(syn::Error::new(
                node.span(),
                "glob imports require namespace expansion and are not supported",
            ));
        }
    }
    Ok(())
}

impl ScopeIndex {
    pub fn module_path(&self, mut scope: ScopeId) -> syn::Result<Vec<String>> {
        let mut path = Vec::new();
        loop {
            let current = &self.scopes[scope.0];
            if current.kind == ScopeKind::Module {
                let parent = current.parent.expect("module has a parent");
                if !matches!(
                    self.scopes[parent.0].kind,
                    ScopeKind::File | ScopeKind::Module
                ) {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        "block-local module has no reusable crate-relative path",
                    ));
                }
                let declaration = self
                    .declarations
                    .iter()
                    .find(|declaration| {
                        declaration.kind == DeclarationKind::Module
                            && declaration.scope == parent
                            && current.range.contains(declaration.range)
                    })
                    .ok_or_else(|| {
                        syn::Error::new(
                            Span::call_site(),
                            "module declaration is missing from the lexical index",
                        )
                    })?;
                path.push(declaration.name.clone());
            }
            match current.parent {
                Some(parent) => scope = parent,
                None => break,
            }
        }
        path.reverse();
        Ok(path)
    }

    fn verify_origin(&self, origin: &SourceOrigin) -> syn::Result<()> {
        let source = origin
            .package_root
            .join(&self.source_path)
            .canonicalize()
            .map_err(|e| {
                syn::Error::new(
                    Span::call_site(),
                    format!("cannot verify indexed source identity: {e}"),
                )
            })?;
        if source != origin.source {
            return Err(syn::Error::new(
                Span::call_site(),
                "source origin belongs to a different indexed file",
            ));
        }
        Ok(())
    }

    pub fn import_origin(&self, id: ImportId, origin: &SourceOrigin) -> syn::Result<PathOrigin> {
        self.verify_origin(origin)?;
        self.expand_import(id, origin, &mut Vec::new())
    }

    pub fn path_origin(
        &self,
        scope: ScopeId,
        path: &syn::Path,
        namespace: Namespace,
        at: LineColumn,
        origin: &SourceOrigin,
    ) -> syn::Result<PathOrigin> {
        self.verify_origin(origin)?;
        if path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(syn::Error::new(
                path.span(),
                "generic path normalization requires further type analysis",
            ));
        }
        self.normalize(
            scope,
            &ImportPath {
                absolute: path.leading_colon.is_some(),
                segments: path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
            },
            namespace,
            at,
            origin,
            &mut Vec::new(),
        )
    }

    fn expand_import(
        &self,
        id: ImportId,
        origin: &SourceOrigin,
        visiting: &mut Vec<ImportId>,
    ) -> syn::Result<PathOrigin> {
        let import = &self.imports[id.0];
        if visiting.contains(&id) {
            let mut chain: Vec<_> = visiting
                .iter()
                .map(|id| self.imports[id.0].alias.as_str())
                .collect();
            chain.push(&import.alias);
            return Err(syn::Error::new(
                Span::call_site(),
                format!("cyclic import aliases: {}", chain.join(" -> ")),
            ));
        }
        visiting.push(id);
        let result = self.normalize(
            import.scope,
            &import.path,
            Namespace::Type,
            import.range.start,
            origin,
            visiting,
        );
        visiting.pop();
        result.map_err(|error| {
            syn::Error::new(
                error.span(),
                format!(
                    "{}:{}: import `{}`: {error}",
                    self.source_path.display(),
                    import.range.start.line,
                    import.alias
                ),
            )
        })
    }

    fn normalize(
        &self,
        scope: ScopeId,
        path: &ImportPath,
        namespace: Namespace,
        at: LineColumn,
        origin: &SourceOrigin,
        visiting: &mut Vec<ImportId>,
    ) -> syn::Result<PathOrigin> {
        let segments: Vec<_> = path
            .segments
            .iter()
            .map(|name| canonical(name).to_owned())
            .collect();
        let fail = |message: &str| syn::Error::new(Span::call_site(), message);
        let head = segments.first().ok_or_else(|| fail("empty path"))?;
        let root_len = if head == "super" {
            segments.iter().take_while(|name| *name == "super").count()
        } else {
            1
        };
        if (path.absolute && matches!(head.as_str(), "crate" | "self" | "super"))
            || segments[root_len..]
                .iter()
                .any(|name| matches!(name.as_str(), "crate" | "self" | "super"))
        {
            return Err(fail("misplaced crate/self/super path segment"));
        }
        if path.absolute {
            return Ok(PathOrigin::External(segments));
        }
        match head.as_str() {
            "crate" => Ok(PathOrigin::Crate(segments[1..].to_vec())),
            "self" => {
                let mut path = self.module_path(scope)?;
                path.extend_from_slice(&segments[1..]);
                Ok(PathOrigin::Crate(path))
            }
            "super" => {
                let mut path = self.module_path(scope)?;
                let parents = segments.iter().take_while(|name| *name == "super").count();
                for _ in 0..parents {
                    path.pop()
                        .ok_or_else(|| fail("super path escapes the source crate root"))?;
                }
                path.extend_from_slice(&segments[parents..]);
                Ok(PathOrigin::Crate(path))
            }
            _ => {
                let lookup_ns = if segments.len() > 1 {
                    Namespace::Type
                } else {
                    namespace
                };
                match self.lookup(scope, lookup_ns, head, at) {
                    Lookup::Import(id) => {
                        let mut path = self.expand_import(id, origin, visiting)?;
                        match &mut path {
                            PathOrigin::Crate(path) | PathOrigin::External(path) => {
                                path.extend_from_slice(&segments[1..])
                            }
                            PathOrigin::Declaration { suffix, .. } => {
                                suffix.extend_from_slice(&segments[1..])
                            }
                        }
                        Ok(path)
                    }
                    Lookup::Found(id) => {
                        let declaration = &self.declarations[id.0];
                        if declaration.kind.is_runtime_binding()
                            || declaration.kind.is_generic_binding()
                        {
                            return Err(fail(
                                "path root is a value/generic binding, not a reusable declaration",
                            ));
                        }
                        if matches!(
                            self.scopes[declaration.scope.0].kind,
                            ScopeKind::File | ScopeKind::Module
                        ) {
                            let mut path = self.module_path(declaration.scope)?;
                            path.push(declaration.name.clone());
                            path.extend_from_slice(&segments[1..]);
                            Ok(PathOrigin::Crate(path))
                        } else {
                            Ok(PathOrigin::Declaration {
                                id,
                                suffix: segments[1..].to_vec(),
                            })
                        }
                    }
                    Lookup::ForbiddenCapture(_) => {
                        Err(fail("path root would capture an enclosing item parameter"))
                    }
                    Lookup::Missing if origin.extern_candidates.contains(head) => {
                        Ok(PathOrigin::External(segments))
                    }
                    Lookup::Missing => Err(fail(
                        "ambiguous path root: no indexed declaration, import, or declared extern-prelude candidate",
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
