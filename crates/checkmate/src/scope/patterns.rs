//! Pattern binding collection, including or-pattern consistency and constants.

use std::collections::BTreeMap;

use proc_macro2::LineColumn;
use syn::spanned::Spanned;

use super::build::Builder;
use super::*;

type Names = BTreeMap<String, PatternName>;

struct PatternName {
    ident: syn::Ident,
    by_ref: bool,
    mutable: bool,
}

impl Builder {
    pub(super) fn bind_pattern(
        &mut self,
        pattern: &syn::Pat,
        kind: DeclarationKind,
        from: LineColumn,
    ) {
        match self.pattern_names(pattern) {
            Ok(names) => {
                // BTreeMap order gives stable IDs independent of hash iteration.
                for name in names.into_values() {
                    self.declare(&name.ident, kind, &[Namespace::Value], Some(from));
                }
            }
            Err(error) if self.error.is_none() => self.error = Some(error),
            Err(_) => {}
        }
    }

    fn pattern_names(&self, pattern: &syn::Pat) -> syn::Result<Names> {
        let mut result = Names::new();
        match pattern {
            syn::Pat::Ident(node) => {
                let ident = canonical(&node.ident.to_string()).to_owned();
                if node.by_ref.is_none()
                    && node.mutability.is_none()
                    && node.subpat.is_none()
                    && self
                        .pattern_constants
                        .contains(&(node.ident.span().start(), ident.clone()))
                {
                    return Ok(result);
                }
                let found = self.index.lookup(
                    self.current,
                    Namespace::Value,
                    &ident,
                    node.ident.span().start(),
                );
                let existing = match found {
                    Lookup::Found(id) | Lookup::ForbiddenCapture(id) => {
                        Some(self.index.declarations[id.0].kind)
                    }
                    Lookup::Import(_) => {
                        return Err(syn::Error::new(
                            node.span(),
                            format!(
                                "pattern `{ident}` may be an imported constant or constructor; target-kind resolution is required before inferring a binding"
                            ),
                        ));
                    }
                    Lookup::Missing => None,
                };
                if existing.is_none() && matches!(ident.as_str(), "None" | "Some" | "Ok" | "Err") {
                    return Err(syn::Error::new(
                        node.span(),
                        format!(
                            "unresolved prelude constructor `{ident}` in pattern; use a qualified path until import/prelude resolution is available"
                        ),
                    ));
                }
                // A bare constant/unit-struct name is a path-like pattern, not a
                // new variable. Imported target kinds remain a task 006 concern.
                if let Some(kind) = existing {
                    if kind.is_pattern_constant()
                        && node.by_ref.is_none()
                        && node.mutability.is_none()
                        && node.subpat.is_none()
                    {
                        return Ok(result);
                    }
                    if kind.is_pattern_constant()
                        || matches!(
                            kind,
                            DeclarationKind::Static
                                | DeclarationKind::TupleStruct
                                | DeclarationKind::ConstParameter
                        )
                    {
                        return Err(syn::Error::new(
                            node.span(),
                            format!(
                                "binding `{ident}` conflicts with a constant, static, or constructor"
                            ),
                        ));
                    }
                }
                result.insert(
                    ident,
                    PatternName {
                        ident: node.ident.clone(),
                        by_ref: node.by_ref.is_some(),
                        mutable: node.mutability.is_some(),
                    },
                );
                if let Some((_, subpattern)) = &node.subpat {
                    merge(&mut result, self.pattern_names(subpattern)?)?;
                }
            }
            syn::Pat::Or(node) => {
                let mut alternatives = node.cases.iter();
                if let Some(first) = alternatives.next() {
                    result = self.pattern_names(first)?;
                    for alternative in alternatives {
                        let other = self.pattern_names(alternative)?;
                        if result.len() != other.len()
                            || result.iter().any(|(name, pattern)| {
                                other.get(name).is_none_or(|other| {
                                    pattern.by_ref != other.by_ref
                                        || pattern.mutable != other.mutable
                                })
                            })
                        {
                            return Err(syn::Error::new(
                                node.span(),
                                "or-pattern alternatives must bind the same names in the same modes",
                            ));
                        }
                    }
                }
            }
            syn::Pat::Tuple(node) => {
                for element in &node.elems {
                    merge(&mut result, self.pattern_names(element)?)?;
                }
            }
            syn::Pat::Slice(node) => {
                for element in &node.elems {
                    merge(&mut result, self.pattern_names(element)?)?;
                }
            }
            syn::Pat::TupleStruct(node) => {
                for element in &node.elems {
                    merge(&mut result, self.pattern_names(element)?)?;
                }
            }
            syn::Pat::Struct(node) => {
                for field in &node.fields {
                    merge(&mut result, self.pattern_names(&field.pat)?)?;
                }
            }
            syn::Pat::Reference(node) => return self.pattern_names(&node.pat),
            syn::Pat::Type(node) => return self.pattern_names(&node.pat),
            syn::Pat::Paren(node) => return self.pattern_names(&node.pat),
            syn::Pat::Wild(_)
            | syn::Pat::Rest(_)
            | syn::Pat::Lit(_)
            | syn::Pat::Path(_)
            | syn::Pat::Range(_) => {}
            _ => {
                return Err(syn::Error::new(
                    pattern.span(),
                    "unsupported pattern form (macros, inline consts, or opaque syntax); no bindings were inferred",
                ));
            }
        }
        Ok(result)
    }
}

fn merge(into: &mut Names, other: Names) -> syn::Result<()> {
    for (name, binding) in other {
        if into.contains_key(&name) {
            return Err(syn::Error::new(
                binding.ident.span(),
                format!("duplicate binding `{name}` in pattern"),
            ));
        }
        into.insert(name, binding);
    }
    Ok(())
}
