//! Reusable paths and local type identities. No declaration is copied here.
//! Invocation placement is an internal caller obligation until task 011 verifies it.

use proc_macro2::{LineColumn, TokenStream};
use quote::quote;
use std::collections::BTreeMap;
use syn::spanned::Spanned;

use super::catalog::{Catalog, canonical, error};
use super::{DeferredKind, ReferenceOrigin, ReferenceTarget, ReferenceUse, References};
use crate::scope::imports::ImportId;
use crate::scope::{DeclarationId, DeclarationKind, Lookup, Namespace, ScopeId, ScopeKind};
use crate::source::ast::{ParsedSource, Selection, SourceRange};
use crate::source::origin::{SourceOrigin, TargetKind};

/// These are not inferred from a filename or exposed as macro options. The caller
/// must establish the actual invocation target before selecting either placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReuseSite {
    SameTarget,
    PackageIntegrationTest,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct QualifiedPath {
    pub absolute: bool,
    pub segments: Vec<String>,
}

impl QualifiedPath {
    pub fn syntax(&self) -> syn::Result<syn::Path> {
        // `crate` is a path keyword, not an identifier accepted by parse_str.
        let mut text = self
            .segments
            .iter()
            .map(|name| {
                if name == "crate" {
                    Ok(name.clone())
                } else {
                    syn::parse_str::<syn::Ident>(name)
                        .or_else(|_| syn::parse_str(&format!("r#{name}")))
                        .map(|id| id.to_string())
                }
            })
            .collect::<syn::Result<Vec<_>>>()?
            .join("::");
        if self.absolute {
            text.insert_str(0, "::");
        }
        syn::parse_str(&text)
    }

    pub fn import_as(&self, alias: &syn::Ident) -> syn::Result<TokenStream> {
        let path = self.syntax()?;
        Ok(quote!(use #path as #alias;))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Resolved {
    Binding,
    Original {
        declaration: DeclarationId,
        base: QualifiedPath,
        member: Option<String>,
    },
    LocalType {
        declaration: DeclarationId,
        member: Option<String>,
    },
    External(QualifiedPath),
}

impl Resolved {
    /// Base imports do not contain type/const arguments. Those stay in the input
    /// AST; generation must rename the resolved root without dropping arguments.
    pub fn import_as(&self, alias: &syn::Ident) -> syn::Result<TokenStream> {
        match self {
            Self::Original { base, member, .. } => {
                let mut path = base.clone();
                path.segments.extend(member.iter().cloned());
                path.import_as(alias)
            }
            Self::External(path) => path.import_as(alias),
            _ => Err(error(
                "bindings and local types do not have reusable imports",
            )),
        }
    }
}

struct Found {
    declaration: DeclarationId,
    member: Option<String>,
    routes: Vec<Vec<String>>,
}

enum Target {
    Item(Found),
    External(QualifiedPath),
}

pub(crate) struct Resolver<'a> {
    pub catalog: Catalog<'a>,
    origin: &'a SourceOrigin,
    site: ReuseSite,
    pub implementations: BTreeMap<DeclarationId, Vec<&'a syn::ItemImpl>>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        source: &'a ParsedSource,
        index: &'a crate::scope::ScopeIndex,
        origin: &'a SourceOrigin,
        site: ReuseSite,
    ) -> syn::Result<Self> {
        let actual = origin
            .package_root
            .join(&source.path)
            .canonicalize()
            .map_err(|e| error(&format!("cannot verify original source: {e}")))?;
        if actual != origin.source {
            return Err(error("source origin belongs to a different indexed file"));
        }
        if std::fs::read_to_string(&actual).map_err(|e| error(&e.to_string()))? != source.text {
            return Err(error(
                "parsed source no longer matches the verified original file",
            ));
        }
        if site == ReuseSite::PackageIntegrationTest && origin.target_kind != TargetKind::Library {
            return Err(error(
                "a binary target cannot be reused as an integration-test library",
            ));
        }
        if origin
            .dependency_candidates
            .iter()
            .any(|name| matches!(name.as_str(), "std" | "core"))
            || (site == ReuseSite::PackageIntegrationTest
                && matches!(origin.target_name.as_str(), "std" | "core"))
        {
            return Err(error(
                "standard-library roots are shadowed by Cargo target/dependency names; prelude mapping is not proven",
            ));
        }
        let mut resolver = Self {
            catalog: Catalog::new(source, index)?,
            origin,
            // Header discovery identifies owners; it is not a reuse/privacy decision.
            site: ReuseSite::SameTarget,
            implementations: BTreeMap::new(),
        };
        for implementation in &resolver.catalog.implementations {
            let syn::Type::Path(ty) = &*implementation.self_ty else {
                return Err(error("implementation requires a nominal self type"));
            };
            let at = ty.span().start();
            let scope = index
                .scope_at(at)
                .ok_or_else(|| error("implementation has no lexical scope"))?;
            let target = resolver.path(scope, &ty.path, ReferenceUse::Type, at)?;
            let (Resolved::LocalType { declaration, .. } | Resolved::Original { declaration, .. }) =
                target
            else {
                return Err(error(
                    "implementation owner is not a same-file nominal declaration",
                ));
            };
            let generics = match resolver.catalog.item(declaration)? {
                syn::Item::Struct(item) => &item.generics,
                syn::Item::Enum(item) => &item.generics,
                _ => {
                    return Err(error(
                        "implementations through type aliases require additional owner analysis",
                    ));
                }
            };
            if !generics.params.is_empty() {
                return Err(error("generic implementation owners are unsupported"));
            }
            resolver
                .implementations
                .entry(declaration)
                .or_default()
                .push(implementation);
        }
        for (owner, implementations) in &resolver.implementations {
            let mut names = std::collections::BTreeSet::new();
            for implementation in implementations {
                for item in &implementation.items {
                    if let syn::ImplItem::Fn(method) = item
                        && !names.insert(canonical(&method.sig.ident.to_string()).to_owned())
                    {
                        return Err(error(
                            "duplicate inherent method for one declaration identity",
                        ));
                    }
                }
            }
            if let syn::Item::Enum(item) = resolver.catalog.item(*owner)?
                && item
                    .variants
                    .iter()
                    .any(|variant| names.contains(canonical(&variant.ident.to_string())))
            {
                return Err(error(
                    "enum variant and inherent method share a name; associated target is ambiguous",
                ));
            }
        }
        resolver.site = site;
        Ok(resolver)
    }

    pub fn references(
        &self,
        references: &References,
        selection: &Selection<'_>,
    ) -> syn::Result<Vec<Resolved>> {
        for pending in &references.deferred_operations {
            if pending.kind == DeferredKind::QualifiedTrait {
                return Err(self.diagnostic(selection, &pending.site, &pending.description));
            }
        }
        let resolved = references.occurrences.iter().map(|reference| {
            let resolve = || {
                match reference.target {
                    ReferenceTarget::Input(_) | ReferenceTarget::GeneratedLocal(_) => return Ok(Resolved::Binding),
                    ReferenceTarget::SnippetLocal(id) => {
                        let kind = self.catalog.index.declarations[id.0].kind;
                        if !matches!(kind, DeclarationKind::Struct | DeclarationKind::TupleStruct | DeclarationKind::UnitStruct | DeclarationKind::Enum | DeclarationKind::TypeAlias) {
                            return Ok(Resolved::Binding);
                        }
                    }
                    ReferenceTarget::GeneratedImport(_) => return Err(error("imports introduced by the output need a verified invocation context")),
                    _ => {}
                }
                // Bare generic arguments bypassed ordinary classification in task
                // 005. Respect an explicitly supplied generic's shell ownership.
                if reference.usage == ReferenceUse::TypeOrConst
                    && reference.site.origin != ReferenceOrigin::Source
                    && let Some(scope) = references.generated_index.scope_at(reference.site.range.start)
                    && let Lookup::Found(id) = references.generated_index.lookup(scope, Namespace::Type, &reference.root, reference.site.range.start)
                    && references.generated_index.declarations[id.0].kind == DeclarationKind::TypeParameter
                {
                    return Ok(Resolved::Binding);
                }
                let path: syn::Path = syn::parse_str(&reference.path)?;
                if reference.site.origin == ReferenceOrigin::Source && path.is_ident("Self")
                    && reference.usage != ReferenceUse::Value
                    && let Some(declaration) = self.catalog.nominal_at(reference.site.range.start)
                {
                    let generics = match self.catalog.item(declaration)? {
                        syn::Item::Struct(item) => &item.generics,
                        syn::Item::Enum(item) => &item.generics,
                        _ => unreachable!("nominal owner"),
                    };
                    if !generics.params.is_empty() {
                        return Err(error("implicit generic Self cannot be relocated without its original instantiation"));
                    }
                    return self.finish(Target::Item(Found { declaration, member: None, routes: Vec::new() }));
                }
                let (scope, at) = if reference.site.origin == ReferenceOrigin::Source {
                    let at = reference.site.range.start;
                    (self.catalog.index.scope_at(at).ok_or_else(|| error("reference has no original lexical scope"))?, at)
                } else {
                    let scope = self.catalog.index.scopes.iter().position(|scope| scope.kind == ScopeKind::Block && scope.range == SourceRange::of(selection.block))
                        .map(ScopeId).ok_or_else(|| error("selected block has no original lexical scope"))?;
                    let at = if reference.site.origin == ReferenceOrigin::Signature { selection.range.start }
                        else { selection.statements.last().expect("nonempty selection").span().end() };
                    (scope, at)
                };
                self.path(scope, &path, reference.usage, at)
            };
            resolve().map_err(|err| self.diagnostic(selection, &reference.site, &format!("`{}`: {err}", reference.path)))
        }).collect::<syn::Result<Vec<_>>>()?;
        super::implementations::check_root_methods(self, references, &resolved, selection)?;
        Ok(resolved)
    }

    pub fn method(&self, owner: DeclarationId, name: &str) -> Option<&'a syn::ImplItemFn> {
        self.implementations
            .get(&owner)?
            .iter()
            .flat_map(|item| &item.items)
            .find_map(|item| {
                if let syn::ImplItem::Fn(method) = item
                    && canonical(&method.sig.ident.to_string()) == canonical(name)
                {
                    Some(method)
                } else {
                    None
                }
            })
    }

    fn self_owner(&self, at: LineColumn) -> syn::Result<Option<DeclarationId>> {
        if let Some(owner) = self.catalog.nominal_at(at) {
            let generics = match self.catalog.item(owner)? {
                syn::Item::Struct(item) => &item.generics,
                syn::Item::Enum(item) => &item.generics,
                _ => unreachable!(),
            };
            if !generics.params.is_empty() {
                return Err(error(
                    "implicit generic Self cannot be relocated without its original instantiation",
                ));
            }
            return Ok(Some(owner));
        }
        let implementation = self
            .implementations
            .iter()
            .flat_map(|(owner, items)| items.iter().map(move |item| (*owner, *item)))
            .filter(|(_, item)| {
                let range = SourceRange::of(*item);
                range.start <= at && at < range.end
            })
            .max_by_key(|(_, item)| item.span().start());
        let Some((owner, implementation)) = implementation else {
            return Ok(None);
        };
        let mut current = self.catalog.index.scope_at(at);
        let mut functions = 0;
        while let Some(id) = current {
            let scope = &self.catalog.index.scopes[id.0];
            if scope.range == SourceRange::of(implementation) {
                return Ok(Some(owner));
            }
            if scope.kind == ScopeKind::Function {
                functions += 1;
            }
            if functions > 1
                || matches!(
                    scope.kind,
                    ScopeKind::Item | ScopeKind::Const | ScopeKind::Module
                )
            {
                return Err(error("Self crosses an implementation capture boundary"));
            }
            current = scope.parent;
        }
        Err(error("Self has no verified implementation scope"))
    }

    pub(super) fn diagnostic(
        &self,
        selection: &Selection<'_>,
        site: &super::ReferenceSite,
        message: &str,
    ) -> syn::Error {
        error(&format!(
            "{}:{}-{}: {:?} {}:{}: {message}",
            self.catalog.index.source_path.display(),
            selection.range.start.line,
            selection.range.end.line - 1,
            site.origin,
            site.range.start.line,
            site.range.start.column + 1
        ))
    }

    pub fn path(
        &self,
        scope: ScopeId,
        path: &syn::Path,
        usage: ReferenceUse,
        at: LineColumn,
    ) -> syn::Result<Resolved> {
        let names: Vec<_> = path
            .segments
            .iter()
            .map(|segment| canonical(&segment.ident.to_string()).to_owned())
            .collect();
        if names.first().is_some_and(|name| name == "Self") && path.leading_colon.is_none() {
            let owner = self
                .self_owner(at)?
                .ok_or_else(|| error("Self has no supported nominal owner"))?;
            let usage = if usage == ReferenceUse::TypeOrConst {
                ReferenceUse::Type
            } else {
                usage
            };
            let target = self.resolve_item(owner, &names[1..], usage, at, &mut Vec::new())?;
            return self.finish(target);
        }
        if usage == ReferenceUse::TypeOrConst {
            let ty = self.walk(
                scope,
                &names,
                path.leading_colon.is_some(),
                ReferenceUse::Type,
                at,
                &mut Vec::new(),
            );
            let value = self.walk(
                scope,
                &names,
                path.leading_colon.is_some(),
                ReferenceUse::Value,
                at,
                &mut Vec::new(),
            );
            return match (ty, value) {
                (Ok(Target::Item(a)), Ok(Target::Item(b)))
                    if a.declaration == b.declaration && a.member == b.member =>
                {
                    self.finish(Target::Item(a))
                }
                (Ok(_), Ok(_)) => Err(error(
                    "ambiguous type-versus-const argument; make the argument's meaning explicit",
                )),
                (Ok(ty), Err(_)) => self.finish(ty),
                (Err(_), Ok(value)) => self.finish(value),
                (Err(ty), Err(value)) => Err(error(&format!(
                    "unresolved type/const argument: {ty}; {value}"
                ))),
            };
        }
        let target = self.walk(
            scope,
            &names,
            path.leading_colon.is_some(),
            usage,
            at,
            &mut Vec::new(),
        )?;
        self.finish(target)
    }

    fn finish(&self, target: Target) -> syn::Result<Resolved> {
        let Target::Item(mut found) = target else {
            let Target::External(path) = target else {
                unreachable!()
            };
            return Ok(Resolved::External(path));
        };
        let item = self.catalog.item(found.declaration)?;
        let Some(canonical) = self.catalog.global_path(found.declaration)? else {
            if matches!(
                item,
                syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Type(_)
            ) {
                return Ok(Resolved::LocalType {
                    declaration: found.declaration,
                    member: found.member,
                });
            }
            return Err(error(
                "function-local non-type items cannot be reused or copied as declaration context",
            ));
        };
        if self.site == ReuseSite::PackageIntegrationTest
            && !self.catalog.item_is_public(found.declaration)?
        {
            return Err(error(
                "original declaration is not public; it cannot be copied to bypass privacy",
            ));
        }
        let route = if self.site == ReuseSite::SameTarget || self.catalog.public_route(&canonical) {
            canonical
        } else {
            found.routes.sort();
            found.routes.into_iter().find(|route| self.catalog.public_route(route)).ok_or_else(|| error("original declaration has no proven public route; inaccessible module-level items are never copied"))?
        };
        let mut segments = match self.site {
            ReuseSite::SameTarget => vec!["crate".into()],
            ReuseSite::PackageIntegrationTest => vec![self.origin.target_name.clone()],
        };
        segments.extend(route);
        Ok(Resolved::Original {
            declaration: found.declaration,
            base: QualifiedPath {
                absolute: self.site == ReuseSite::PackageIntegrationTest,
                segments,
            },
            member: found.member,
        })
    }

    fn walk(
        &self,
        scope: ScopeId,
        names: &[String],
        absolute: bool,
        usage: ReferenceUse,
        at: LineColumn,
        imports: &mut Vec<ImportId>,
    ) -> syn::Result<Target> {
        let head = names
            .first()
            .ok_or_else(|| error("empty declaration path"))?;
        if absolute {
            return self.external(names, usage);
        }
        if matches!(head.as_str(), "crate" | "self" | "super") {
            let count = if head == "super" {
                names
                    .iter()
                    .take_while(|name| name.as_str() == "super")
                    .count()
            } else {
                1
            };
            let mut prefix = if head == "crate" {
                Vec::new()
            } else {
                self.catalog.index.module_path(scope)?
            };
            if head == "super" {
                for _ in 0..count {
                    prefix
                        .pop()
                        .ok_or_else(|| error("super path escapes the source crate"))?;
                }
            }
            prefix.extend_from_slice(&names[count..]);
            return self.in_module(ScopeId(0), &prefix, usage, at, imports);
        }
        let namespace = if names.len() > 1 || usage != ReferenceUse::Value {
            Namespace::Type
        } else {
            Namespace::Value
        };
        match self.catalog.index.lookup(scope, namespace, head, at) {
            Lookup::Found(id) => self.resolve_item(id, &names[1..], usage, at, imports),
            Lookup::Import(id) => self.resolve_import(id, &names[1..], usage, imports),
            Lookup::ForbiddenCapture(_) => Err(error(
                "declaration path would capture an enclosing runtime/generic binding",
            )),
            Lookup::Missing => {
                if let Some(mut path) = prelude(
                    head,
                    if names.len() > 1 {
                        ReferenceUse::Type
                    } else {
                        usage
                    },
                ) {
                    if names.len() > 1 {
                        // Do not guess whether a type-qualified value is inherent
                        // or supplied by a trait imported in the original scope.
                        let variant = matches!(
                            (head.as_str(), names.get(1).map(String::as_str)),
                            ("Option", Some("Some" | "None")) | ("Result", Some("Ok" | "Err"))
                        );
                        if names.len() != 2 || !variant || usage != ReferenceUse::Value {
                            return Err(error(
                                "associated members require further type/trait resolution",
                            ));
                        }
                        path.segments.extend_from_slice(&names[1..]);
                    }
                    Ok(Target::External(path))
                } else if self.origin.extern_candidates.contains(head) {
                    self.external(names, usage)
                } else {
                    Err(error(&format!(
                        "unknown declaration `{head}`; no indexed binding or proven prelude mapping"
                    )))
                }
            }
        }
    }

    fn in_module(
        &self,
        scope: ScopeId,
        names: &[String],
        usage: ReferenceUse,
        at: LineColumn,
        imports: &mut Vec<ImportId>,
    ) -> syn::Result<Target> {
        let head = names
            .first()
            .ok_or_else(|| error("a module is not a reusable type/value declaration"))?;
        let namespace = if names.len() > 1 || usage != ReferenceUse::Value {
            Namespace::Type
        } else {
            Namespace::Value
        };
        match self.catalog.index.lookup(scope, namespace, head, at) {
            Lookup::Found(id) if self.catalog.index.declarations[id.0].scope == scope => {
                self.resolve_item(id, &names[1..], usage, at, imports)
            }
            Lookup::Import(id) if self.catalog.index.imports[id.0].scope == scope => {
                self.resolve_import(id, &names[1..], usage, imports)
            }
            _ => Err(error(&format!(
                "unknown member `{head}` in the indexed module"
            ))),
        }
    }

    fn resolve_item(
        &self,
        id: DeclarationId,
        suffix: &[String],
        usage: ReferenceUse,
        at: LineColumn,
        imports: &mut Vec<ImportId>,
    ) -> syn::Result<Target> {
        let item = self.catalog.item(id)?;
        if let syn::Item::Mod(_) = item {
            return self.in_module(self.catalog.module_scope(id)?, suffix, usage, at, imports);
        }
        let mut member = None;
        if !suffix.is_empty() {
            if suffix.len() == 1
                && usage == ReferenceUse::Value
                && self.method(id, &suffix[0]).is_some()
            {
                member = Some(suffix[0].clone());
            } else if let syn::Item::Enum(item) = item {
                let variant = item
                    .variants
                    .iter()
                    .find(|variant| canonical(&variant.ident.to_string()) == suffix[0]);
                if suffix.len() != 1 || variant.is_none() {
                    return Err(error(
                        "unknown enum variant or unsupported associated member",
                    ));
                }
                let variant = variant.unwrap();
                if !matches!(
                    (usage, &variant.fields),
                    (
                        ReferenceUse::Value,
                        syn::Fields::Unit | syn::Fields::Unnamed(_)
                    ) | (ReferenceUse::Constructor, _)
                ) {
                    return Err(error(
                        "enum variant is not a type or a matching constructor",
                    ));
                }
                member = Some(suffix[0].clone());
            } else {
                return Err(error(
                    "associated members require further type/trait resolution",
                ));
            }
        } else {
            let matches = match usage {
                ReferenceUse::Type => matches!(
                    item,
                    syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Type(_)
                ),
                ReferenceUse::Constructor => {
                    matches!(item, syn::Item::Struct(_) | syn::Item::Type(_))
                }
                ReferenceUse::Value => {
                    matches!(
                        item,
                        syn::Item::Fn(_) | syn::Item::Const(_) | syn::Item::Static(_)
                    ) || matches!(item, syn::Item::Struct(item) if !matches!(item.fields, syn::Fields::Named(_)))
                }
                ReferenceUse::TypeOrConst => unreachable!(),
            };
            if !matches {
                return Err(error(
                    "original item does not supply the requested namespace/constructor",
                ));
            }
        }
        Ok(Target::Item(Found {
            declaration: id,
            member,
            routes: Vec::new(),
        }))
    }

    fn resolve_import(
        &self,
        id: ImportId,
        suffix: &[String],
        usage: ReferenceUse,
        imports: &mut Vec<ImportId>,
    ) -> syn::Result<Target> {
        if imports.contains(&id) {
            return Err(error(
                "cyclic import aliases while resolving original declarations",
            ));
        }
        let import = &self.catalog.index.imports[id.0];
        let mut names: Vec<_> = import
            .path
            .segments
            .iter()
            .map(|name| canonical(name).to_owned())
            .collect();
        names.extend_from_slice(suffix);
        imports.push(id);
        let result = self.walk(
            import.scope,
            &names,
            import.path.absolute,
            usage,
            import.range.start,
            imports,
        );
        imports.pop();
        let mut target = result?;
        if let Target::Item(found) = &mut target
            && matches!(
                self.catalog.index.scopes[import.scope.0].kind,
                ScopeKind::File | ScopeKind::Module
            )
        {
            let mut route = self.catalog.index.module_path(import.scope)?;
            route.push(import.alias.clone());
            route.extend_from_slice(suffix);
            // A variant-only re-export does not provide an importable enum base.
            if let Some(member) = &found.member {
                if suffix.last() == Some(member) {
                    route.pop();
                    found.routes.push(route);
                }
            } else {
                found.routes.push(route);
            }
        }
        Ok(target)
    }

    fn external(&self, names: &[String], usage: ReferenceUse) -> syn::Result<Target> {
        if !names
            .first()
            .is_some_and(|root| self.origin.extern_candidates.contains(root))
        {
            return Err(error(
                "external root is not a declared dependency or standard-library candidate",
            ));
        }
        if self.site == ReuseSite::PackageIntegrationTest
            && !matches!(names[0].as_str(), "std" | "core")
        {
            return Err(error(
                "dependency identity across library/integration-test targets is not proven; matching Cargo aliases alone are insufficient",
            ));
        }
        if usage == ReferenceUse::Value {
            return Err(error(
                "external value/associated-item paths need semantic resolution; no helper or trait context is guessed",
            ));
        }
        Ok(Target::External(QualifiedPath {
            absolute: true,
            segments: names.to_vec(),
        }))
    }
}

fn prelude(name: &str, usage: ReferenceUse) -> Option<QualifiedPath> {
    let primitive = matches!(
        name,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    );
    let path = match usage {
        ReferenceUse::Type if primitive => format!("core::primitive::{name}"),
        ReferenceUse::Type => match name {
            "String" => "std::string::String",
            "Vec" => "std::vec::Vec",
            "Box" => "std::boxed::Box",
            "Option" => "core::option::Option",
            "Result" => "core::result::Result",
            "Clone" => "core::clone::Clone",
            "Copy" => "core::marker::Copy",
            "Sized" => "core::marker::Sized",
            "Send" => "core::marker::Send",
            "Sync" => "core::marker::Sync",
            "Default" => "core::default::Default",
            _ => return None,
        }
        .to_owned(),
        ReferenceUse::Value => match name {
            "Some" => "core::option::Option::Some",
            "None" => "core::option::Option::None",
            "Ok" => "core::result::Result::Ok",
            "Err" => "core::result::Result::Err",
            _ => return None,
        }
        .to_owned(),
        _ => return None,
    };
    Some(QualifiedPath {
        absolute: true,
        segments: path.split("::").map(str::to_owned).collect(),
    })
}

#[cfg(test)]
pub(super) mod tests;
