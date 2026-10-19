//! Dot calls are approved only at the first receiver candidate: an owned local
//! nominal receiver and an inherent `self` method. Autoref/deref can select traits
//! before an inherent method; mere existence of a same-named method is NOT proof.
use super::{Targets, key};
use crate::diagnostics::{Error, Kind, Origin, Result};
use crate::resolve::ReferenceUse;
use crate::resolve::catalog::{canonical, error};
use crate::resolve::declarations::{Resolved, Resolver};
use crate::scope::{DeclarationId, Lookup, Namespace, ScopeIndex};
use crate::source::ast::SourceRange;
use std::collections::{BTreeMap, BTreeSet};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

pub(super) enum Syntax<'a> {
    File(&'a syn::File),
    Implementation(&'a syn::ItemImpl, DeclarationId),
}
#[derive(Clone, Copy, PartialEq)]
enum Construction {
    Value,
    Call,
    Literal,
}

fn constructed(fields: &syn::Fields, construction: Construction) -> bool {
    // Tuple constructors/variants without a call are function items: Clone and
    // other traits may apply to that function, not to the result nominal type.
    match construction {
        Construction::Value => matches!(fields, syn::Fields::Unit),
        Construction::Call => matches!(fields, syn::Fields::Unnamed(_)),
        Construction::Literal => true,
    }
}

#[derive(Clone, Copy)]
enum Binding<'a> {
    Type(&'a syn::Type),
    Initializer(&'a syn::Expr),
    Receiver(DeclarationId),
}
struct Scan<'a> {
    bindings: BTreeMap<proc_macro2::LineColumn, Binding<'a>>,
    calls: Vec<&'a syn::ExprMethodCall>,
    owner: Option<DeclarationId>,
}
impl<'a> Scan<'a> {
    fn bind(&mut self, pattern: &'a syn::Pat, binding: Binding<'a>) {
        if let syn::Pat::Ident(pattern) = pattern
            && pattern.by_ref.is_none()
            && pattern.subpat.is_none()
        {
            self.bindings.insert(pattern.ident.span().start(), binding);
        }
    }
}
impl<'ast> Visit<'ast> for Scan<'ast> {
    fn visit_fn_arg(&mut self, node: &'ast syn::FnArg) {
        match node {
            syn::FnArg::Typed(argument) => self.bind(&argument.pat, Binding::Type(&argument.ty)),
            syn::FnArg::Receiver(receiver)
                if receiver.reference.is_none() && receiver.colon_token.is_none() =>
            {
                let binding = self
                    .owner
                    .map_or(Binding::Type(&receiver.ty), Binding::Receiver);
                self.bindings
                    .insert(receiver.self_token.span.start(), binding);
            }
            _ => {}
        }
        visit::visit_fn_arg(self, node);
    }
    fn visit_local(&mut self, node: &'ast syn::Local) {
        match &node.pat {
            syn::Pat::Type(pattern) => self.bind(&pattern.pat, Binding::Type(&pattern.ty)),
            pattern => {
                if let Some(initializer) = &node.init {
                    self.bind(pattern, Binding::Initializer(&initializer.expr));
                }
            }
        }
        visit::visit_local(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.calls.push(node);
        visit::visit_expr_method_call(self, node);
    }
}

pub(super) fn check(
    resolver: &Resolver<'_>,
    index: &ScopeIndex,
    targets: &Targets,
    syntax: Syntax<'_>,
) -> Result<Vec<(proc_macro2::LineColumn, DeclarationId)>> {
    let mut scan = Scan {
        bindings: BTreeMap::new(),
        calls: Vec::new(),
        owner: None,
    };
    match syntax {
        Syntax::File(file) => scan.visit_file(file),
        Syntax::Implementation(item, owner) => {
            scan.owner = Some(owner);
            scan.visit_item_impl(item);
        }
    }
    let checker = Checker {
        resolver,
        index,
        targets,
        bindings: scan.bindings,
        original: false,
    };
    let mut approved_calls = Vec::new();
    for call in scan.calls {
        let at = call.method.span().start();
        let owner = checker.receiver(&call.receiver, &mut BTreeSet::new());
        let approved = owner.and_then(|owner| resolver.method(owner, &call.method.to_string()))
            .is_some_and(|method| matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(receiver)) if receiver.reference.is_none() && receiver.colon_token.is_none()));
        if !approved || call.turbofish.is_some() {
            return Err(Error::new(
                call.method.span(),
                format!(
                    "method `{}` requires method/trait resolution: only proven owned local receivers with inherent by-value self methods are supported; use an explicit Type::method call for supported borrowed methods",
                    call.method
                ),
            ).with_kind(Kind::MethodResolution).with_subject(call.method.to_string(), None).with_site(Origin::Source, SourceRange::of(&call.method)));
        }
        approved_calls.push((at, owner.expect("approved receiver")));
    }
    Ok(approved_calls)
}

pub(super) fn verify_source_receivers(
    resolver: &Resolver<'_>,
    calls: &[(proc_macro2::LineColumn, DeclarationId)],
) -> Result<()> {
    if calls.is_empty() {
        return Ok(());
    }
    let mut scan = Scan {
        bindings: BTreeMap::new(),
        calls: Vec::new(),
        owner: None,
    };
    scan.visit_file(&resolver.catalog.source.file);
    let targets = Targets::new();
    let checker = Checker {
        resolver,
        index: resolver.catalog.index,
        targets: &targets,
        bindings: scan.bindings,
        original: true,
    };
    for (at, expected) in calls {
        let call = scan
            .calls
            .iter()
            .find(|call| call.method.span().start() == *at)
            .ok_or_else(|| error("original method site is absent"))?;
        let original = checker.receiver(&call.receiver, &mut BTreeSet::new());
        if original != Some(*expected) {
            return Err(Error::new(
                call.method.span(),
                "supplied input/receiver context does not prove the original method target; receiver identity must agree before and after extraction",
            ).with_kind(Kind::MethodResolution).with_subject(call.method.to_string(), None).with_site(Origin::Source, SourceRange::of(&call.method)));
        }
    }
    Ok(())
}

struct Checker<'a, 'source, 'syntax> {
    resolver: &'a Resolver<'source>,
    index: &'a ScopeIndex,
    targets: &'a Targets,
    bindings: BTreeMap<proc_macro2::LineColumn, Binding<'syntax>>,
    original: bool,
}
impl Checker<'_, '_, '_> {
    fn target(&self, path: &syn::Path, usage: ReferenceUse) -> Option<Resolved> {
        if !self.original {
            return self.targets.get(&key(SourceRange::of(path))).cloned();
        }
        let at = path.span().start();
        let scope = self.index.scope_at(at)?;
        self.resolver.path(scope, path, usage, at).ok()
    }
    fn local(&self, target: &Resolved) -> Option<DeclarationId> {
        if let Resolved::LocalType {
            declaration,
            member: None,
        } = target
            && matches!(
                self.resolver.catalog.item(*declaration).ok()?,
                syn::Item::Struct(_) | syn::Item::Enum(_)
            )
        {
            Some(*declaration)
        } else {
            None
        }
    }
    fn ty(&self, ty: &syn::Type) -> Option<DeclarationId> {
        let syn::Type::Path(ty) = ty else {
            return None;
        };
        if ty.qself.is_some()
            || ty
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return None;
        }
        self.local(&self.target(&ty.path, ReferenceUse::Type)?)
    }
    fn constructor(
        &self,
        path: &syn::Path,
        construction: Construction,
        usage: ReferenceUse,
    ) -> Option<DeclarationId> {
        let Resolved::LocalType {
            declaration,
            member,
        } = self.target(path, usage)?
        else {
            return None;
        };
        // Aliases and generic arguments need normalization, not spelling guesses.
        if path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return None;
        }
        match (self.resolver.catalog.item(declaration).ok()?, member) {
            (syn::Item::Struct(item), None) if constructed(&item.fields, construction) => {
                Some(declaration)
            }
            (syn::Item::Enum(item), Some(name))
                if item.variants.iter().any(|variant| {
                    canonical(&variant.ident.to_string()) == name
                        && constructed(&variant.fields, construction)
                }) =>
            {
                Some(declaration)
            }
            (_, Some(name)) if construction == Construction::Call => {
                let method = self.resolver.method(declaration, &name)?;
                if matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
                    return None;
                }
                let syn::ReturnType::Type(_, ty) = &method.sig.output else {
                    return None;
                };
                let syn::Type::Path(ty) = &**ty else {
                    return None;
                };
                if ty.qself.is_some()
                    || ty
                        .path
                        .segments
                        .iter()
                        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
                {
                    return None;
                }
                let at = ty.span().start();
                let scope = self.resolver.catalog.index.scope_at(at)?;
                let target = self
                    .resolver
                    .path(scope, &ty.path, ReferenceUse::Type, at)
                    .ok()?;
                self.local(&target)
            }
            _ => None,
        }
    }
    fn receiver(
        &self,
        expr: &syn::Expr,
        active: &mut BTreeSet<DeclarationId>,
    ) -> Option<DeclarationId> {
        match expr {
            syn::Expr::Paren(expr) => self.receiver(&expr.expr, active),
            syn::Expr::Group(expr) => self.receiver(&expr.expr, active),
            syn::Expr::Struct(expr) if expr.qself.is_none() => {
                self.constructor(&expr.path, Construction::Literal, ReferenceUse::Constructor)
            }
            syn::Expr::Call(expr) => {
                let syn::Expr::Path(path) = &*expr.func else {
                    return None;
                };
                if path.qself.is_some() {
                    return None;
                }
                self.constructor(&path.path, Construction::Call, ReferenceUse::Value)
            }
            syn::Expr::Path(expr) if expr.qself.is_none() => {
                if let Some(owner) =
                    self.constructor(&expr.path, Construction::Value, ReferenceUse::Value)
                {
                    return Some(owner);
                }
                if expr.path.leading_colon.is_some() || expr.path.segments.len() != 1 {
                    return None;
                }
                let at = expr.span().start();
                let scope = self.index.scope_at(at)?;
                let Lookup::Found(id) = self.index.lookup(
                    scope,
                    Namespace::Value,
                    canonical(&expr.path.segments[0].ident.to_string()),
                    at,
                ) else {
                    return None;
                };
                if !active.insert(id) {
                    return None;
                }
                let binding = self
                    .bindings
                    .get(&self.index.declarations[id.0].range.start)?;
                let owner = match binding {
                    Binding::Type(ty) => self.ty(ty),
                    Binding::Initializer(initializer) => self.receiver(initializer, active),
                    Binding::Receiver(owner) => Some(*owner),
                };
                active.remove(&id);
                owner
            }
            _ => None,
        }
    }
}
