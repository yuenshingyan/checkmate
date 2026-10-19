use proc_macro2::{LineColumn, Span};
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::*;
use crate::scope::{DeclarationKind, Lookup, Namespace, ScopeId, ScopeKind};

pub(super) fn pattern_constants(
    index: &ScopeIndex,
    selection: &Selection<'_>,
    snippet_start: usize,
) -> BTreeSet<(LineColumn, String)> {
    struct Constants<'a> {
        index: &'a ScopeIndex,
        source_start: usize,
        generated_start: usize,
        found: BTreeSet<(LineColumn, String)>,
    }
    impl<'ast> Visit<'ast> for Constants<'_> {
        fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
            let at = node.ident.span().start();
            let name = node.ident.to_string().trim_start_matches("r#").to_owned();
            if node.by_ref.is_none()
                && node.mutability.is_none()
                && node.subpat.is_none()
                && let Some(scope) = self.index.scope_at(at)
                && let Lookup::Found(id) = self.index.lookup(scope, Namespace::Value, &name, at)
                && matches!(
                    self.index.declarations[id.0].kind,
                    DeclarationKind::Constant | DeclarationKind::UnitStruct
                )
            {
                self.found.insert((
                    LineColumn {
                        line: self.generated_start + at.line - self.source_start,
                        column: at.column,
                    },
                    name,
                ));
            }
            visit::visit_pat_ident(self, node);
        }
    }
    let mut visitor = Constants {
        index,
        source_start: selection.range.start.line,
        generated_start: snippet_start,
        found: BTreeSet::new(),
    };
    for statement in selection.statements {
        visitor.visit_stmt(statement);
    }
    visitor.found
}

pub(super) struct Collected {
    pub occurrences: Vec<Reference>,
    pub required_inputs: BTreeSet<String>,
    pub deferred_operations: Vec<DeferredOperation>,
}

pub(super) fn visit(
    source: &ParsedSource,
    index: &ScopeIndex,
    selection: &Selection<'_>,
    generated: &ParsedSource,
    generated_index: &ScopeIndex,
    snippet_start: usize,
    snippet_end: usize,
) -> syn::Result<Collected> {
    let selected_scope = index
        .scopes
        .iter()
        .position(|scope| {
            scope.kind == ScopeKind::Block && scope.range == SourceRange::of(selection.block)
        })
        .ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "selected block is absent from the source index",
            )
        })?;
    let wrapper = generated_index
        .scopes
        .iter()
        .position(|scope| scope.kind == ScopeKind::Function && scope.parent == Some(ScopeId(0)))
        .ok_or_else(|| {
            syn::Error::new(Span::call_site(), "analysis wrapper has no function scope")
        })?;
    let inputs = generated_index
        .declarations
        .iter()
        .filter(|decl| decl.kind == DeclarationKind::Parameter && decl.scope == ScopeId(wrapper))
        .map(|decl| decl.name.clone())
        .collect();
    let mut collector = Collector {
        source,
        index,
        selection,
        generated_index,
        snippet_start,
        snippet_end,
        selected_scope: ScopeId(selected_scope),
        wrapper: ScopeId(wrapper),
        inputs,
        result: Collected {
            occurrences: Vec::new(),
            required_inputs: BTreeSet::new(),
            deferred_operations: Vec::new(),
        },
        error: None,
    };
    collector.visit_file(&generated.file);
    match collector.error {
        Some(error) => Err(error),
        None => Ok(collector.result),
    }
}

struct Collector<'a> {
    source: &'a ParsedSource,
    index: &'a ScopeIndex,
    selection: &'a Selection<'a>,
    generated_index: &'a ScopeIndex,
    snippet_start: usize,
    snippet_end: usize,
    selected_scope: ScopeId,
    wrapper: ScopeId,
    inputs: BTreeSet<String>,
    result: Collected,
    error: Option<syn::Error>,
}

impl Collector<'_> {
    fn original_position(&self, at: LineColumn) -> LineColumn {
        LineColumn {
            line: self.selection.range.start.line + at.line - self.snippet_start,
            column: at.column,
        }
    }

    fn site(&self, span: Span) -> ReferenceSite {
        let range = SourceRange::from_span(span);
        if range.start.line >= self.snippet_start && range.start.line <= self.snippet_end {
            ReferenceSite {
                origin: ReferenceOrigin::Source,
                range: SourceRange {
                    start: self.original_position(range.start),
                    end: self.original_position(range.end),
                },
            }
        } else {
            ReferenceSite {
                origin: if range.start.line < self.snippet_start {
                    ReferenceOrigin::Signature
                } else {
                    ReferenceOrigin::Output
                },
                range,
            }
        }
    }

    fn fail(&mut self, span: Span, message: &str) {
        if self.error.is_some() {
            return;
        }
        let site = self.site(span);
        let location = match site.origin {
            ReferenceOrigin::Source => format!(
                "source {}:{}",
                site.range.start.line,
                site.range.start.column + 1
            ),
            ReferenceOrigin::Signature => "supplied signature".into(),
            ReferenceOrigin::Output => "output expression".into(),
        };
        self.error = Some(syn::Error::new(
            span,
            format!(
                "{}:{}-{}: {location}: {message}",
                self.source.path.display(),
                self.selection.range.start.line,
                self.selection.range.end.line - 1,
            ),
        ));
    }

    fn generated_lookup(&self, span: Span, namespace: Namespace, name: &str) -> Lookup {
        self.generated_index
            .scope_at(span.start())
            .map_or(Lookup::Missing, |scope| {
                self.generated_index
                    .lookup(scope, namespace, name, span.start())
            })
    }

    fn original_lookup(&self, site: &ReferenceSite, namespace: Namespace, name: &str) -> Lookup {
        let (scope, at) = match site.origin {
            ReferenceOrigin::Source => (
                self.index
                    .scope_at(site.range.start)
                    .unwrap_or(self.selected_scope),
                site.range.start,
            ),
            ReferenceOrigin::Signature => (self.selected_scope, self.selection.range.start),
            ReferenceOrigin::Output => (
                self.selected_scope,
                self.selection.statements.last().unwrap().span().end(),
            ),
        };
        self.index.lookup(scope, namespace, name, at)
    }

    fn original_declaration(&self, generated: DeclarationId) -> Option<DeclarationId> {
        let declaration = &self.generated_index.declarations[generated.0];
        if declaration.range.start.line < self.snippet_start
            || declaration.range.start.line > self.snippet_end
        {
            return None;
        }
        let start = self.original_position(declaration.range.start);
        self.index
            .declarations
            .iter()
            .position(|original| {
                original.range.start == start
                    && original.name == declaration.name
                    && original.kind == declaration.kind
            })
            .map(DeclarationId)
    }

    fn original_import(&self, generated: ImportId) -> Option<ImportId> {
        let import = &self.generated_index.imports[generated.0];
        if import.range.start.line < self.snippet_start
            || import.range.start.line > self.snippet_end
        {
            return None;
        }
        let start = self.original_position(import.range.start);
        self.index
            .imports
            .iter()
            .position(|original| original.range.start == start && original.alias == import.alias)
            .map(ImportId)
    }

    fn input(&mut self, name: &str, span: Span) -> Option<ReferenceTarget> {
        if self.inputs.contains(name) {
            self.result.required_inputs.insert(name.into());
            Some(ReferenceTarget::Input(name.into()))
        } else {
            self.fail(span, &format!("required input `{name}` is not supplied by fn_sig; add a typed parameter instead of copying or running its initializer"));
            None
        }
    }

    fn source_target(
        &mut self,
        lookup: Lookup,
        generated: Lookup,
        name: &str,
        site: &ReferenceSite,
        span: Span,
    ) -> Option<ReferenceTarget> {
        match lookup {
            Lookup::Found(id) => {
                let declaration = &self.index.declarations[id.0];
                if self.selection.range.contains(declaration.range) {
                    return Some(ReferenceTarget::SnippetLocal(id));
                }
                if matches!(
                    declaration.kind,
                    DeclarationKind::Parameter | DeclarationKind::Local | DeclarationKind::Pattern
                ) {
                    if site.origin == ReferenceOrigin::Signature {
                        self.fail(span, &format!("runtime value `{name}` cannot supply declaration context to a function signature"));
                        return None;
                    }
                    if matches!(generated, Lookup::ForbiddenCapture(_)) {
                        self.fail(
                            span,
                            &format!(
                                "`{name}` would capture a parameter across an item/const boundary"
                            ),
                        );
                        return None;
                    }
                    return self.input(name, span);
                }
                if site.origin == ReferenceOrigin::Source && self.signature_shadows(generated) {
                    self.fail(span, &format!("supplied signature would shadow original declaration `{name}`; do not substitute a parameter or generic for an original item"));
                    return None;
                }
                Some(ReferenceTarget::Declaration(id))
            }
            Lookup::Import(id) => {
                if site.origin == ReferenceOrigin::Source && self.signature_shadows(generated) {
                    self.fail(
                        span,
                        &format!("supplied signature would shadow original import `{name}`"),
                    );
                    None
                } else {
                    Some(ReferenceTarget::Import(id))
                }
            }
            Lookup::ForbiddenCapture(_) => {
                self.fail(
                    span,
                    &format!("`{name}` crosses an original item/const capture boundary"),
                );
                None
            }
            Lookup::Missing => match generated {
                Lookup::Found(id)
                    if self.generated_index.declarations[id.0].kind
                        == DeclarationKind::Parameter
                        && self.generated_index.declarations[id.0].scope == self.wrapper =>
                {
                    self.input(name, span)
                }
                Lookup::ForbiddenCapture(_) => {
                    self.fail(
                        span,
                        &format!("`{name}` would capture an enclosing generated binding"),
                    );
                    None
                }
                _ => Some(ReferenceTarget::Deferred),
            },
        }
    }

    fn signature_shadows(&self, lookup: Lookup) -> bool {
        match lookup {
            Lookup::Found(id) | Lookup::ForbiddenCapture(id) => {
                self.generated_index.declarations[id.0].range.start.line < self.snippet_start
            }
            _ => false,
        }
    }

    fn fallback_is_captured(&self, span: Span, lookup: Lookup) -> bool {
        let Lookup::Found(id) = lookup else {
            return false;
        };
        let kind = self.index.declarations[id.0].kind;
        let runtime = matches!(
            kind,
            DeclarationKind::Parameter | DeclarationKind::Local | DeclarationKind::Pattern
        );
        let generic = matches!(
            kind,
            DeclarationKind::TypeParameter | DeclarationKind::ConstParameter
        );
        let mut current = self.generated_index.scope_at(span.start());
        while let Some(scope) = current {
            if scope == self.wrapper {
                break;
            }
            let scope = &self.generated_index.scopes[scope.0];
            if matches!(
                scope.kind,
                ScopeKind::Function | ScopeKind::Item | ScopeKind::Const
            ) && (runtime || generic)
                || scope.kind == ScopeKind::InlineConst && runtime
            {
                return true;
            }
            current = scope.parent;
        }
        false
    }

    fn target(&mut self, path: &syn::Path, usage: ReferenceUse) -> Option<ReferenceTarget> {
        let span = path.span();
        let site = self.site(span);
        let name = path.segments.first()?.ident.to_string();
        let name = name.strip_prefix("r#").unwrap_or(&name);
        if usage == ReferenceUse::TypeOrConst
            || path.leading_colon.is_some()
            || matches!(name, "crate" | "self" | "super")
        {
            return Some(ReferenceTarget::Deferred);
        }
        let namespace = if usage == ReferenceUse::Type || path.segments.len() > 1 {
            Namespace::Type
        } else {
            Namespace::Value
        };
        let generated = self.generated_lookup(span, namespace, name);
        let original = self.original_lookup(&site, namespace, name);
        if site.origin == ReferenceOrigin::Source {
            return self.source_target(original, generated, name, &site, span);
        }
        match generated {
            Lookup::Found(id) => {
                let declaration = &self.generated_index.declarations[id.0];
                if declaration.kind == DeclarationKind::Parameter
                    && declaration.scope == self.wrapper
                {
                    return self.input(name, span);
                }
                if let Some(original) = self.original_declaration(id) {
                    return Some(ReferenceTarget::SnippetLocal(original));
                }
                if declaration.range.start.line < self.snippet_start
                    || declaration.range.start.line > self.snippet_end
                {
                    return Some(ReferenceTarget::GeneratedLocal(id));
                }
                // A bare constant pattern from the source may look like a new
                // binding in the incomplete shell. Trust original declaration
                // identity, not that synthetic binding.
            }
            Lookup::Import(id) => {
                return Some(self.original_import(id).map_or(
                    ReferenceTarget::GeneratedImport(id),
                    ReferenceTarget::Import,
                ));
            }
            Lookup::ForbiddenCapture(_) => {
                self.fail(
                    span,
                    &format!("`{name}` would capture an enclosing generated binding"),
                );
                return None;
            }
            Lookup::Missing => {}
        }
        if self.fallback_is_captured(span, original) {
            self.fail(
                span,
                &format!(
                    "`{name}` would capture original context across an output item/const boundary"
                ),
            );
            return None;
        }
        self.source_target(original, generated, name, &site, span)
    }

    fn record(&mut self, path: &syn::Path, usage: ReferenceUse) {
        if self.error.is_some() {
            return;
        }
        if let Some(target) = self.target(path, usage) {
            if path.segments.len() > 1 {
                self.defer(
                    path.span(),
                    "qualified path requires final-target resolution beyond its lexical root"
                        .into(),
                );
            }
            self.result.occurrences.push(Reference {
                path: path.to_token_stream().to_string(),
                root: path
                    .segments
                    .first()
                    .unwrap()
                    .ident
                    .to_string()
                    .trim_start_matches("r#")
                    .into(),
                usage,
                target,
                site: self.site(path.span()),
            });
        }
    }

    fn defer(&mut self, span: Span, description: String) {
        self.result.deferred_operations.push(DeferredOperation {
            description,
            site: self.site(span),
        });
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node.qself.is_some() {
            self.defer(
                node.span(),
                "qualified associated type requires trait/type resolution".into(),
            );
        }
        self.record(&node.path, ReferenceUse::Type);
        visit::visit_type_path(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if node.qself.is_some() {
            self.defer(
                node.span(),
                "qualified associated value requires trait/type resolution".into(),
            );
        }
        self.record(&node.path, ReferenceUse::Value);
        visit::visit_expr_path(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        self.record(&node.path, ReferenceUse::Type);
        visit::visit_expr_struct(self, node);
    }

    fn visit_pat_struct(&mut self, node: &'ast syn::PatStruct) {
        self.record(&node.path, ReferenceUse::Type);
        visit::visit_pat_struct(self, node);
    }

    fn visit_pat_tuple_struct(&mut self, node: &'ast syn::PatTupleStruct) {
        self.record(&node.path, ReferenceUse::Value);
        visit::visit_pat_tuple_struct(self, node);
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        let name = node.ident.to_string();
        let name = name.trim_start_matches("r#");
        let site = self.site(node.ident.span());
        let original = self.original_lookup(&site, Namespace::Value, name);
        if site.origin == ReferenceOrigin::Source {
            let is_binding = self
                .index
                .declarations
                .iter()
                .any(|decl| decl.range.start == site.range.start && decl.name == name);
            if !is_binding {
                let path = syn::Path::from(node.ident.clone());
                self.record(&path, ReferenceUse::Value);
            }
        } else if site.origin == ReferenceOrigin::Output {
            let generated = self.generated_lookup(node.ident.span(), Namespace::Value, name);
            if let Lookup::Found(id) = generated
                && matches!(
                    self.generated_index.declarations[id.0].kind,
                    DeclarationKind::Constant | DeclarationKind::UnitStruct
                )
            {
                self.record(&syn::Path::from(node.ident.clone()), ReferenceUse::Value);
                visit::visit_pat_ident(self, node);
                return;
            }
            if let Lookup::Found(id) = original
                && matches!(
                    self.index.declarations[id.0].kind,
                    DeclarationKind::Constant
                        | DeclarationKind::UnitStruct
                        | DeclarationKind::TupleStruct
                        | DeclarationKind::Static
                )
            {
                self.fail(node.span(), &format!("output pattern `{name}` conflicts with a source constant/constructor; use a qualified pattern or a distinct binding name"));
            } else if matches!(original, Lookup::Import(_)) {
                self.fail(
                    node.span(),
                    &format!("output pattern `{name}` has an unresolved imported target kind"),
                );
            }
        }
        visit::visit_pat_ident(self, node);
    }

    fn visit_trait_bound(&mut self, node: &'ast syn::TraitBound) {
        self.record(&node.path, ReferenceUse::Type);
        visit::visit_trait_bound(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.defer(
            node.method.span(),
            format!("method `{}` requires method/trait resolution", node.method),
        );
        visit::visit_expr_method_call(self, node);
    }

    fn visit_generic_argument(&mut self, node: &'ast syn::GenericArgument) {
        if let syn::GenericArgument::Type(syn::Type::Path(path)) = node
            && path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
            && matches!(path.path.segments[0].arguments, syn::PathArguments::None)
        {
            // syn cannot know whether Foo<N> names a type or a const parameter.
            self.record(&path.path, ReferenceUse::TypeOrConst);
        } else {
            visit::visit_generic_argument(self, node);
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.fail(node.span(), "module declarations inside selected code/output require independent module context and are not supported");
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.fail(
            node.span(),
            "opaque macro tokens require expansion before reference collection",
        );
    }

    fn visit_type(&mut self, node: &'ast syn::Type) {
        if matches!(node, syn::Type::Verbatim(_)) {
            self.fail(
                node.span(),
                "opaque type syntax cannot be analyzed for references",
            );
        } else {
            visit::visit_type(self, node);
        }
    }

    fn visit_item_use(&mut self, _node: &'ast syn::ItemUse) {
        // Imports are declarations, not uses of each segment as a free variable.
        // Their provenance is retained by the source/generated import index.
    }
}
