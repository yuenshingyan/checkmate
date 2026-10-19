use crate::diagnostics::{Error, Kind, Origin, Result};
use std::collections::BTreeSet;

use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::{Dependency, LocalKind};
use crate::resolve::ReferenceUse;
use crate::resolve::catalog::error;
use crate::resolve::declarations::{Resolved, Resolver};
use crate::scope::{DeclarationId, DeclarationKind};
use crate::source::ast::SourceRange;

pub(super) fn dependencies(
    resolver: &Resolver<'_>,
    owner: DeclarationId,
) -> Result<Vec<Dependency>> {
    let item = resolver.catalog.item(owner)?;
    let (generics, kind) = match item {
        syn::Item::Struct(item) => (&item.generics, LocalKind::Struct),
        syn::Item::Enum(item) => (&item.generics, LocalKind::Enum),
        syn::Item::Type(item) => (&item.generics, LocalKind::Alias),
        _ => {
            return Err(error(
                "only local structs, enums, and aliases have discoverable copied context",
            ));
        }
    };
    if !generics.params.is_empty() {
        return Err(Error::new(
            item.span(),
            "generic local declarations are outside the initial promotion subset",
        )
        .with_kind(Kind::UnsafePromotion)
        .with_site(Origin::Source, SourceRange::of(item)));
    }
    let mut visitor = Collector {
        resolver,
        owner,
        kind,
        indirect: false,
        layout: true,
        expansion: true,
        lifetimes: Vec::new(),
        dependencies: Vec::new(),
        error: None,
    };
    visitor.visit_item(item);
    match visitor.error {
        Some(error) => Err(error),
        None => Ok(visitor.dependencies),
    }
}

struct Collector<'a, 'source> {
    resolver: &'a Resolver<'source>,
    owner: DeclarationId,
    kind: LocalKind,
    indirect: bool,
    layout: bool,
    expansion: bool,
    lifetimes: Vec<BTreeSet<String>>,
    dependencies: Vec<Dependency>,
    error: Option<Error>,
}

impl Collector<'_, '_> {
    fn fail(&mut self, node: &impl Spanned, message: &str) {
        self.fail_error(
            node,
            Error::new(node.span(), message).with_kind(Kind::UnsafePromotion),
        );
    }

    fn fail_error(&mut self, node: &impl Spanned, failure: Error) {
        if self.error.is_none() {
            let at = node.span().start();
            let message = format!("source {}:{}: {failure}", at.line, at.column + 1);
            self.error = Some(
                failure
                    .context(message)
                    .with_site(Origin::Source, SourceRange::of(node)),
            );
        }
    }

    fn record(&mut self, path: &syn::Path, usage: ReferenceUse) -> Option<Resolved> {
        if self.error.is_some() {
            return None;
        }
        let at = path.span().start();
        let result = if path.is_ident("Self")
            && self.kind != LocalKind::Alias
            && usage != ReferenceUse::Value
        {
            Ok(Resolved::LocalType {
                declaration: self.owner,
                member: None,
            })
        } else {
            self.resolver
                .catalog
                .index
                .scope_at(at)
                .ok_or_else(|| error("type dependency has no original scope"))
                .and_then(|scope| self.resolver.path(scope, path, usage, at))
        };
        match result {
            Ok(target) => {
                self.dependencies.push(Dependency {
                    path: path.to_token_stream().to_string(),
                    range: SourceRange::of(path),
                    usage,
                    target: target.clone(),
                    indirect: self.indirect,
                    layout: self.layout,
                    expansion: self.expansion,
                });
                Some(target)
            }
            Err(err) => {
                let message = format!("{}: {err}", path.to_token_stream());
                self.fail_error(path, err.context(message));
                None
            }
        }
    }

    fn with_indirection(&mut self, visit: impl FnOnce(&mut Self)) {
        let previous = self.indirect;
        self.indirect = true;
        visit(self);
        self.indirect = previous;
    }

    fn with_lifetimes(
        &mut self,
        bound: Option<&syn::BoundLifetimes>,
        visit: impl FnOnce(&mut Self),
    ) {
        let mut names = BTreeSet::new();
        if let Some(bound) = bound {
            for parameter in &bound.lifetimes {
                match parameter {
                    syn::GenericParam::Lifetime(parameter) => {
                        names.insert(parameter.lifetime.ident.to_string());
                    }
                    _ => {
                        self.fail(bound, "higher-ranked binders must contain only lifetimes");
                        return;
                    }
                }
            }
        }
        self.lifetimes.push(names);
        visit(self);
        self.lifetimes.pop();
    }
}

impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if !node.path().is_ident("doc") {
            self.fail(
                node,
                "non-documentation attributes on required local context are unsupported",
            );
        }
    }

    fn visit_type(&mut self, node: &'ast syn::Type) {
        if self.error.is_some() {
            return;
        }
        match node {
            syn::Type::Array(_)
            | syn::Type::BareFn(_)
            | syn::Type::Group(_)
            | syn::Type::Never(_)
            | syn::Type::Paren(_)
            | syn::Type::Path(_)
            | syn::Type::Ptr(_)
            | syn::Type::Reference(_)
            | syn::Type::Slice(_)
            | syn::Type::TraitObject(_)
            | syn::Type::Tuple(_) => visit::visit_type(self, node),
            _ => self.fail(
                node,
                "opaque/inferred/impl-trait type syntax is not a self-contained dependency",
            ),
        }
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node.qself.is_some() {
            self.fail(node, "qualified associated types require trait resolution");
            return;
        }
        let Some(target) = self.record(&node.path, ReferenceUse::Type) else {
            return;
        };
        let indirect = indirect_container(&target);
        for (segment_index, segment) in node.path.segments.iter().enumerate() {
            match &segment.arguments {
                syn::PathArguments::None => {}
                syn::PathArguments::AngleBracketed(arguments) => {
                    for (argument_index, argument) in arguments.args.iter().enumerate() {
                        // Box/Vec/Rc/Arc store only their FIRST type argument
                        // indirectly. An allocator argument may be stored inline.
                        if indirect
                            && segment_index + 1 == node.path.segments.len()
                            && argument_index == 0
                            && matches!(argument, syn::GenericArgument::Type(_))
                        {
                            self.with_indirection(|this| this.visit_generic_argument(argument));
                        } else {
                            self.visit_generic_argument(argument);
                        }
                    }
                }
                syn::PathArguments::Parenthesized(arguments) => {
                    self.visit_parenthesized_generic_arguments(arguments)
                }
            }
        }
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

    fn visit_type_reference(&mut self, node: &'ast syn::TypeReference) {
        self.with_indirection(|this| visit::visit_type_reference(this, node));
    }

    fn visit_type_ptr(&mut self, node: &'ast syn::TypePtr) {
        self.with_indirection(|this| visit::visit_type_ptr(this, node));
    }

    fn visit_type_bare_fn(&mut self, node: &'ast syn::TypeBareFn) {
        self.with_lifetimes(node.lifetimes.as_ref(), |this| {
            this.with_indirection(|this| visit::visit_type_bare_fn(this, node))
        });
    }

    fn visit_trait_bound(&mut self, node: &'ast syn::TraitBound) {
        let previous = self.layout;
        self.layout = false;
        self.with_lifetimes(node.lifetimes.as_ref(), |this| {
            this.record(&node.path, ReferenceUse::Type);
            visit::visit_trait_bound(this, node);
        });
        self.layout = previous;
    }

    fn visit_where_clause(&mut self, node: &'ast syn::WhereClause) {
        let previous = (self.layout, self.expansion);
        self.layout = false;
        self.expansion = false;
        visit::visit_where_clause(self, node);
        (self.layout, self.expansion) = previous;
    }

    fn visit_predicate_type(&mut self, node: &'ast syn::PredicateType) {
        self.with_lifetimes(node.lifetimes.as_ref(), |this| {
            visit::visit_predicate_type(this, node)
        });
    }

    fn visit_lifetime(&mut self, node: &'ast syn::Lifetime) {
        let name = node.ident.to_string();
        if name != "static"
            && !self
                .lifetimes
                .iter()
                .rev()
                .any(|scope| scope.contains(&name))
        {
            self.fail(
                node,
                "lifetime depends on an enclosing/unresolved binder; no lifetime is invented",
            );
        }
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        if self.error.is_some() {
            return;
        }
        let previous = self.layout;
        self.layout = false;
        match node {
            syn::Expr::Lit(_) | syn::Expr::Path(_) | syn::Expr::Unary(_) | syn::Expr::Binary(_)
            | syn::Expr::Paren(_) | syn::Expr::Group(_) | syn::Expr::Cast(_) => visit::visit_expr(self, node),
            syn::Expr::Block(block) if block.label.is_none() && block.block.stmts.len() == 1 && matches!(block.block.stmts[0], syn::Stmt::Expr(_, None)) => visit::visit_expr_block(self, block),
            _ => self.fail(node, "unsupported dependent constant expression; helper calls, statements, and macros are not evaluated or copied"),
        }
        self.layout = previous;
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if node.qself.is_some() {
            self.fail(
                node,
                "qualified associated constants require trait resolution",
            );
            return;
        }
        if let Some(target) = self.record(&node.path, ReferenceUse::Value)
            && !matches!(target, Resolved::Original { declaration, member: None, .. }
                if self.resolver.catalog.index.declarations[declaration.0].kind == DeclarationKind::Constant)
        {
            self.fail(node, "constant expressions may only reuse original named constants; copied type values and other value kinds require further analysis");
            return;
        }
        visit::visit_expr_path(self, node);
    }
}

fn indirect_container(target: &Resolved) -> bool {
    let Resolved::External(path) = target else {
        return false;
    };
    matches!(
        path.segments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["std", "boxed", "Box"]
            | ["std", "vec", "Vec"]
            | ["std", "rc", "Rc"]
            | ["std", "sync", "Arc"]
            | ["std" | "core", "marker", "PhantomData"]
    )
}
