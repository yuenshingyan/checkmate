//! Whole-file syntax and exact, line-based selection for future automatic mode.
//!
//! Parse the original text, not proc-macro input tokens: proc_macro2's fallback
//! parser then supplies source locations even on stable Rust. Locations use
//! one-based lines and zero-based Unicode character columns, not byte offsets.
//! Span ends are used as exclusive positions. We never use columns to slice UTF-8.

use std::path::PathBuf;

use proc_macro2::{LineColumn, Span};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceRange {
    pub start: LineColumn,
    pub end: LineColumn,
}

impl SourceRange {
    pub fn of(node: &impl Spanned) -> Self {
        Self::from_span(node.span())
    }

    pub fn from_span(span: Span) -> Self {
        Self {
            start: span.start(),
            end: span.end(),
        }
    }

    pub fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EnclosingKind {
    File,
    Module(String),
    Function(String),
    Closure,
    Block,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EnclosingScope {
    pub kind: EnclosingKind,
    pub range: SourceRange,
}

pub(crate) struct ParsedSource {
    pub path: PathBuf,
    pub text: String,
    pub file: syn::File,
}

pub(crate) struct Selection<'a> {
    pub block: &'a syn::Block,
    pub statements: &'a [syn::Stmt],
    pub range: SourceRange,
    pub enclosing: Vec<EnclosingScope>,
}

impl ParsedSource {
    pub fn parse(path: impl Into<PathBuf>, text: impl Into<String>) -> syn::Result<Self> {
        let path = path.into();
        let text = text.into();
        let file = syn::parse_file(&text).map_err(|error| {
            let at = error.span().start();
            syn::Error::new(
                error.span(),
                format!(
                    "{}:{}:{}: automatic context requires a complete Rust source file: {error}",
                    path.display(),
                    at.line,
                    at.column + 1,
                ),
            )
        })?;
        Ok(Self { path, text, file })
    }

    pub fn select(&self, start: usize, end: usize) -> syn::Result<Selection<'_>> {
        let fail = |message: &str| {
            syn::Error::new(
                Span::call_site(),
                format!("{}:{start}-{end}: {message}", self.path.display(),),
            )
        };
        if start == 0 || end < start || end > self.text.lines().count() {
            return Err(fail("invalid inclusive line range"));
        }
        let range = SourceRange {
            start: LineColumn {
                line: start,
                column: 0,
            },
            end: LineColumn {
                line: end + 1,
                column: 0,
            },
        };
        let mut collector = Blocks {
            scopes: vec![EnclosingScope {
                kind: EnclosingKind::File,
                range: SourceRange {
                    start: LineColumn { line: 1, column: 0 },
                    end: LineColumn {
                        line: self.text.lines().count() + 1,
                        column: 0,
                    },
                },
            }],
            blocks: Vec::new(),
        };
        collector.visit_file(&self.file);

        for candidate in collector.blocks {
            // A selected whole nested block may be a statement of its parent;
            // its delimiters must never be mistaken for part of its own body.
            let interior = SourceRange {
                start: candidate.block.brace_token.span.open().end(),
                end: candidate.block.brace_token.span.close().start(),
            };
            if !interior.contains(range) {
                continue;
            }
            let mut first = None;
            let mut last = 0;
            let mut partial = false;
            for (index, statement) in candidate.block.stmts.iter().enumerate() {
                let statement_range = SourceRange::of(statement);
                if range.overlaps(statement_range) {
                    if !range.contains(statement_range) {
                        partial = true;
                        break;
                    }
                    first.get_or_insert(index);
                    last = index;
                }
            }
            if !partial && let Some(first) = first {
                return Ok(Selection {
                    block: candidate.block,
                    statements: &candidate.block.stmts[first..=last],
                    range,
                    enclosing: candidate.enclosing,
                });
            }
        }
        Err(fail(
            "select complete statements or a tail expression in one block; do not include enclosing headers/delimiters, partial statements, or only comments/blank lines",
        ))
    }
}

struct BlockCandidate<'a> {
    block: &'a syn::Block,
    enclosing: Vec<EnclosingScope>,
}

struct Blocks<'a> {
    scopes: Vec<EnclosingScope>,
    blocks: Vec<BlockCandidate<'a>>,
}

impl Blocks<'_> {
    fn enter(&mut self, kind: EnclosingKind, node: &impl Spanned) {
        self.scopes.push(EnclosingScope {
            kind,
            range: SourceRange::of(node),
        });
    }
}

impl<'ast> Visit<'ast> for Blocks<'ast> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.enter(EnclosingKind::Module(node.ident.to_string()), node);
        visit::visit_item_mod(self, node);
        self.scopes.pop();
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.enter(EnclosingKind::Function(node.sig.ident.to_string()), node);
        visit::visit_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.enter(EnclosingKind::Function(node.sig.ident.to_string()), node);
        visit::visit_impl_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.enter(EnclosingKind::Function(node.sig.ident.to_string()), node);
        visit::visit_trait_item_fn(self, node);
        self.scopes.pop();
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.enter(EnclosingKind::Closure, node);
        visit::visit_expr_closure(self, node);
        self.scopes.pop();
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.enter(EnclosingKind::Block, node);
        self.blocks.push(BlockCandidate {
            block: node,
            enclosing: self.scopes.clone(),
        });
        visit::visit_block(self, node);
        self.scopes.pop();
    }
}

#[cfg(test)]
mod tests;
