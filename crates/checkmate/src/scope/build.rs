use proc_macro2::{LineColumn, Span};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::*;

pub(super) fn build(source: &ParsedSource) -> syn::Result<ScopeIndex> {
    let mut builder = Builder {
        index: ScopeIndex {
            scopes: vec![Scope {
                parent: None,
                kind: ScopeKind::File,
                range: SourceRange {
                    start: LineColumn { line: 1, column: 0 },
                    end: LineColumn {
                        line: source.text.lines().count() + 1,
                        column: 0,
                    },
                },
                depth: 0,
                bindings: Vec::new(),
            }],
            declarations: Vec::new(),
        },
        current: ScopeId(0),
        error: None,
    };
    builder.visit_file(&source.file);
    if let Some(error) = builder.error {
        let at = error.span().start();
        Err(syn::Error::new(
            error.span(),
            format!(
                "{}:{}:{}: cannot index lexical context: {error}",
                source.path.display(),
                at.line,
                at.column + 1,
            ),
        ))
    } else {
        Ok(builder.index)
    }
}

pub(super) struct Builder {
    pub index: ScopeIndex,
    pub current: ScopeId,
    pub error: Option<syn::Error>,
}

impl Builder {
    pub fn fail(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(syn::Error::new(span, message));
        }
    }

    fn within(&mut self, kind: ScopeKind, range: SourceRange, body: impl FnOnce(&mut Self)) {
        let parent = self.current;
        self.current = ScopeId(self.index.scopes.len());
        self.index.scopes.push(Scope {
            parent: Some(parent),
            kind,
            range,
            depth: self.index.scopes[parent.0].depth + 1,
            bindings: Vec::new(),
        });
        body(self);
        self.current = parent;
    }

    pub fn declare(
        &mut self,
        ident: &syn::Ident,
        kind: DeclarationKind,
        namespaces: &[Namespace],
        from: Option<LineColumn>,
    ) {
        let name = canonical(&ident.to_string()).to_owned();
        if kind == DeclarationKind::Parameter
            && self.index.scopes[self.current.0]
                .bindings
                .iter()
                .any(|binding| {
                    let declaration = &self.index.declarations[binding.declaration.0];
                    declaration.kind == DeclarationKind::Parameter && declaration.name == name
                })
        {
            self.fail(ident.span(), &format!("duplicate parameter `{name}`"));
            return;
        }
        // Duplicate hoisted names are not valid Rust. Do not guess which one wins.
        if from.is_none()
            && self.index.scopes[self.current.0]
                .bindings
                .iter()
                .any(|binding| {
                    binding.available_from.is_none()
                        && namespaces.contains(&binding.namespace)
                        && self.index.declarations[binding.declaration.0].name == name
                })
        {
            self.fail(
                ident.span(),
                &format!("duplicate item or parameter `{name}` in the same namespace"),
            );
            return;
        }
        let declaration = DeclarationId(self.index.declarations.len());
        self.index.declarations.push(Declaration {
            name,
            kind,
            scope: self.current,
            range: SourceRange::of(ident),
        });
        for &namespace in namespaces {
            self.index.scopes[self.current.0].bindings.push(Binding {
                declaration,
                namespace,
                available_from: from,
            });
        }
    }

    fn item_binding(&mut self, item: &syn::Item) {
        use DeclarationKind as D;
        use Namespace::{Macro, Type, Value};
        match item {
            syn::Item::Fn(item) => self.declare(&item.sig.ident, D::Function, &[Value], None),
            syn::Item::Struct(item) => {
                let (kind, namespaces): (_, &[_]) = match item.fields {
                    syn::Fields::Named(_) => (D::Struct, &[Type]),
                    syn::Fields::Unnamed(_) => (D::TupleStruct, &[Type, Value]),
                    syn::Fields::Unit => (D::UnitStruct, &[Type, Value]),
                };
                self.declare(&item.ident, kind, namespaces, None);
            }
            syn::Item::Enum(item) => self.declare(&item.ident, D::Enum, &[Type], None),
            syn::Item::Type(item) => self.declare(&item.ident, D::TypeAlias, &[Type], None),
            syn::Item::Const(item) => self.declare(&item.ident, D::Constant, &[Value], None),
            syn::Item::Static(item) => self.declare(&item.ident, D::Static, &[Value], None),
            syn::Item::Mod(item) => self.declare(&item.ident, D::Module, &[Type], None),
            syn::Item::Macro(item) if item.mac.path.is_ident("macro_rules") => {
                if let Some(ident) = &item.ident {
                    self.declare(ident, D::Macro, &[Macro], Some(item.span().end()));
                } else {
                    self.fail(item.span(), "unnamed macro definition");
                }
            }
            syn::Item::Use(item) => self.fail(item.span(), "import indexing is deferred to task 004"),
            _ => self.fail(item.span(), "unsupported item kind; imports, impls, traits, unions, extern items, and item-producing macros require further analysis"),
        }
    }

    fn generics(&mut self, generics: &syn::Generics) {
        for param in &generics.params {
            match param {
                syn::GenericParam::Type(param) => self.declare(
                    &param.ident,
                    DeclarationKind::TypeParameter,
                    &[Namespace::Type],
                    None,
                ),
                syn::GenericParam::Const(param) => self.declare(
                    &param.ident,
                    DeclarationKind::ConstParameter,
                    &[Namespace::Value],
                    None,
                ),
                syn::GenericParam::Lifetime(_) => {} // Lifetimes are not lexical type/value names.
            }
        }
    }

    fn condition(&mut self, expression: &syn::Expr) {
        match expression {
            syn::Expr::Let(node) => {
                for attr in &node.attrs {
                    self.visit_attribute(attr);
                }
                self.visit_expr(&node.expr);
                self.bind_pattern(&node.pat, DeclarationKind::Pattern, node.expr.span().end());
            }
            syn::Expr::Binary(node) if matches!(node.op, syn::BinOp::And(_)) => {
                for attr in &node.attrs {
                    self.visit_attribute(attr);
                }
                self.condition(&node.left);
                self.condition(&node.right);
            }
            syn::Expr::Paren(node) => {
                for attr in &node.attrs {
                    self.visit_attribute(attr);
                }
                self.condition(&node.expr);
            }
            syn::Expr::Group(node) => {
                for attr in &node.attrs {
                    self.visit_attribute(attr);
                }
                self.condition(&node.expr);
            }
            _ => self.visit_expr(expression),
        }
    }
}

impl<'ast> Visit<'ast> for Builder {
    fn visit_file(&mut self, file: &'ast syn::File) {
        for item in &file.items {
            self.item_binding(item);
        }
        visit::visit_file(self, file);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.within(ScopeKind::Block, SourceRange::of(block), |this| {
            for statement in &block.stmts {
                if let syn::Stmt::Item(item) = statement {
                    this.item_binding(item);
                }
            }
            visit::visit_block(this, block);
        });
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if self.error.is_some() {
            return;
        }
        match item {
            syn::Item::Fn(node) => {
                self.within(ScopeKind::Function, SourceRange::of(node), |this| {
                    this.generics(&node.sig.generics);
                    for argument in &node.sig.inputs {
                        match argument {
                            syn::FnArg::Typed(arg) => this.bind_pattern(
                                &arg.pat,
                                DeclarationKind::Parameter,
                                node.block.brace_token.span.open().end(),
                            ),
                            syn::FnArg::Receiver(arg) => this.fail(
                                arg.span(),
                                "receiver parameters require implementation analysis",
                            ),
                        }
                    }
                    visit::visit_item_fn(this, node);
                });
            }
            syn::Item::Mod(node) => {
                if let Some((_, items)) = &node.content {
                    self.within(ScopeKind::Module, SourceRange::of(node), |this| {
                        for item in items {
                            this.item_binding(item);
                        }
                        visit::visit_item_mod(this, node);
                    });
                } else {
                    self.fail(node.span(), "out-of-line module context is not indexed");
                }
            }
            syn::Item::Struct(node) => {
                self.within(ScopeKind::Item, SourceRange::of(node), |this| {
                    this.generics(&node.generics);
                    visit::visit_item_struct(this, node);
                })
            }
            syn::Item::Enum(node) => self.within(ScopeKind::Item, SourceRange::of(node), |this| {
                this.generics(&node.generics);
                visit::visit_item_enum(this, node);
            }),
            syn::Item::Type(node) => self.within(ScopeKind::Item, SourceRange::of(node), |this| {
                this.generics(&node.generics);
                visit::visit_item_type(this, node);
            }),
            syn::Item::Const(node) => {
                self.within(ScopeKind::Const, SourceRange::of(node), |this| {
                    this.generics(&node.generics);
                    visit::visit_item_const(this, node);
                })
            }
            syn::Item::Static(node) => {
                self.within(ScopeKind::Const, SourceRange::of(node), |this| {
                    visit::visit_item_static(this, node);
                })
            }
            // Definitions do not expand bindings; invocations are rejected below.
            syn::Item::Macro(node) if node.mac.path.is_ident("macro_rules") => {
                for attr in &node.attrs {
                    self.visit_attribute(attr);
                }
            }
            _ => self.fail(item.span(), "unsupported item in lexical context"),
        }
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        // Validate the pattern before visiting opaque pattern subexpressions.
        // Its availability position, not visitor order, keeps the new binding
        // out of its initializer and let-else failure branch.
        self.bind_pattern(&local.pat, DeclarationKind::Local, local.span().end());
        visit::visit_local(self, local);
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.within(ScopeKind::Closure, SourceRange::of(node), |this| {
            for input in &node.inputs {
                this.bind_pattern(input, DeclarationKind::Parameter, node.body.span().start());
            }
            visit::visit_expr_closure(this, node);
        });
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.visit_expr(&node.expr);
        // The iterator expression is outside the pattern's lexical scope.
        self.within(ScopeKind::Loop, SourceRange::of(&node.body), |this| {
            this.bind_pattern(
                &node.pat,
                DeclarationKind::Pattern,
                node.body.brace_token.span.open().end(),
            );
            this.visit_block(&node.body);
        });
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.within(ScopeKind::Arm, SourceRange::of(node), |this| {
            this.bind_pattern(&node.pat, DeclarationKind::Pattern, node.pat.span().end());
            visit::visit_arm(this, node);
        });
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        let range = SourceRange {
            start: node.cond.span().start(),
            end: node.then_branch.span().end(),
        };
        self.within(ScopeKind::Condition, range, |this| {
            this.condition(&node.cond);
            this.visit_block(&node.then_branch);
        });
        if let Some((_, otherwise)) = &node.else_branch {
            self.visit_expr(otherwise);
        }
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        let range = SourceRange {
            start: node.cond.span().start(),
            end: node.body.span().end(),
        };
        self.within(ScopeKind::Condition, range, |this| {
            this.condition(&node.cond);
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_const(&mut self, node: &'ast syn::ExprConst) {
        self.within(ScopeKind::InlineConst, SourceRange::of(node), |this| {
            visit::visit_expr_const(this, node)
        });
    }

    fn visit_expr_let(&mut self, node: &'ast syn::ExprLet) {
        self.fail(
            node.span(),
            "let expressions are supported only in if/while conditions and && chains",
        );
    }

    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        self.fail(node.span(), "macro expressions require expansion analysis");
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.fail(
            node.span(),
            "macro statements may introduce unknown bindings",
        );
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr") {
            self.fail(
                attr.span(),
                "conditional compilation cannot be resolved by lexical indexing",
            );
        } else if !["doc", "allow", "warn", "deny", "forbid", "expect"]
            .iter()
            .any(|name| attr.path().is_ident(name))
        {
            self.fail(
                attr.span(),
                "unsupported attribute may alter lexical declarations",
            );
        }
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        if matches!(node, syn::Expr::Verbatim(_)) {
            self.fail(node.span(), "opaque expression syntax cannot be indexed");
        } else {
            visit::visit_expr(self, node);
        }
    }
}
