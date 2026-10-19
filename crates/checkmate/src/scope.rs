//! Lexical declarations, namespaces, and binding visibility for automatic context.
//!
//! This is not whole-program name resolution. Imports and opaque binding-producing
//! syntax are explicit errors until their own tasks implement them. A missing name
//! means only "not in this lexical index", not "not provided by Rust's prelude".

mod build;
mod patterns;

use proc_macro2::LineColumn;

use crate::source::ast::{ParsedSource, SourceRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScopeId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeclarationId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Namespace {
    Type,
    Value,
    Macro,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScopeKind {
    File,
    Module,
    Function,
    Item,
    Block,
    Closure,
    Loop,
    Arm,
    Condition,
    Const,
    InlineConst,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationKind {
    Struct,
    TupleStruct,
    UnitStruct,
    Enum,
    TypeAlias,
    Function,
    Constant,
    Static,
    Module,
    Macro,
    Parameter,
    Local,
    Pattern,
    TypeParameter,
    ConstParameter,
}

impl DeclarationKind {
    fn is_runtime_binding(self) -> bool {
        matches!(self, Self::Parameter | Self::Local | Self::Pattern)
    }

    fn is_generic_binding(self) -> bool {
        matches!(self, Self::TypeParameter | Self::ConstParameter)
    }

    fn is_pattern_constant(self) -> bool {
        matches!(self, Self::Constant | Self::UnitStruct)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Declaration {
    pub name: String,
    pub kind: DeclarationKind,
    pub scope: ScopeId,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Binding {
    declaration: DeclarationId,
    namespace: Namespace,
    // None means a hoisted item/generic binding. Locals and macro_rules bindings
    // use a source position to express textual visibility.
    available_from: Option<LineColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Scope {
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub range: SourceRange,
    depth: usize,
    bindings: Vec<Binding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lookup {
    Found(DeclarationId),
    ForbiddenCapture(DeclarationId),
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScopeIndex {
    pub scopes: Vec<Scope>,
    pub declarations: Vec<Declaration>,
}

impl ScopeIndex {
    pub fn build(source: &ParsedSource) -> syn::Result<Self> {
        build::build(source)
    }

    pub fn scope_at(&self, at: LineColumn) -> Option<ScopeId> {
        self.scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.range.start <= at && at < scope.range.end)
            .max_by_key(|(_, scope)| scope.depth)
            .map(|(id, _)| ScopeId(id))
    }

    pub fn lookup(
        &self,
        mut scope: ScopeId,
        namespace: Namespace,
        name: &str,
        at: LineColumn,
    ) -> Lookup {
        let name = canonical(name);
        let mut blocks_runtime = false;
        let mut blocks_generics = false;
        loop {
            let current = &self.scopes[scope.0];
            for binding in current.bindings.iter().rev() {
                let declaration = &self.declarations[binding.declaration.0];
                if binding.namespace == namespace
                    && declaration.name == name
                    && binding.available_from.is_none_or(|from| from <= at)
                {
                    return if (blocks_runtime && declaration.kind.is_runtime_binding())
                        || (blocks_generics && declaration.kind.is_generic_binding())
                    {
                        Lookup::ForbiddenCapture(binding.declaration)
                    } else {
                        Lookup::Found(binding.declaration)
                    };
                }
            }
            // Ordinary unqualified names do not inherit a module's parent scope.
            // Textually scoped macro_rules names are the exception.
            if current.kind == ScopeKind::Module && namespace != Namespace::Macro {
                return Lookup::Missing;
            }
            let item_boundary = matches!(
                current.kind,
                ScopeKind::Function | ScopeKind::Item | ScopeKind::Const
            );
            blocks_runtime |= item_boundary || current.kind == ScopeKind::InlineConst;
            blocks_generics |= item_boundary;
            match current.parent {
                Some(parent) => scope = parent,
                None => return Lookup::Missing,
            }
        }
    }
}

fn canonical(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

#[cfg(test)]
mod tests;
