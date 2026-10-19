//! Versioned, read-only model of the approved dependency graph. This is source
//! review data, not generated Rust, a compiler fingerprint, or a semantic proof.
use super::identity::StableNames;
use crate::{
    diagnostics::{Error, Kind, Result},
    resolve::{
        References,
        declarations::{QualifiedPath, Resolved, Resolver},
        graph::{Graph, LocalKind, NodeId, NodeKind},
    },
    source::{ast::Selection, origin::SourceOrigin},
};
use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Component};

const FORMAT: &str = "checkmate-context";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContextSnapshot {
    format: String,
    version: u32,
    extraction: String,
    required_inputs: Vec<String>,
    source: Source,
    references: Vec<Mapping>,
    declarations: BTreeMap<String, Declaration>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Source {
    package: String,
    library: String,
    path: String,
    owner: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Declaration {
    role: String,
    syntax: Option<String>,
    reuse_route: Option<String>,
    implementations: Vec<String>,
    references: Vec<Dependency>,
    edges: Vec<Edge>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Target {
    declaration: Option<String>,
    route: Option<String>,
    member: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Mapping {
    origin: String,
    path: String,
    usage: String,
    target: Target,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Dependency {
    path: String,
    usage: String,
    target: Target,
    indirect: bool,
    layout: bool,
    expansion: bool,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Edge {
    target: String,
    unguarded_layout: bool,
    alias_expansion: bool,
}

pub(crate) fn error(message: impl Into<String>) -> Error {
    Error::new(Span::call_site(), message).with_kind(Kind::ContextSnapshot)
}
fn path(path: &QualifiedPath) -> Result<String> {
    Ok(path.syntax()?.to_token_stream().to_string())
}
fn key(id: &NodeId, names: &StableNames) -> Result<String> {
    match id {
        NodeId::Source(id) => Ok(format!(
            "source:{}",
            names
                .declaration(*id)
                .map_err(|err| err.with_kind(Kind::ContextSnapshot))?
        )),
        NodeId::External(value) => Ok(format!("external:{}", path(value)?)),
    }
}
fn target(value: &Resolved, names: &StableNames) -> Result<Target> {
    let (declaration, route, member) = match value {
        Resolved::Binding => (None, None, None),
        Resolved::Original {
            declaration,
            base,
            member,
        } => (
            Some(key(&NodeId::Source(*declaration), names)?),
            Some(path(base)?),
            member.clone(),
        ),
        Resolved::LocalType {
            declaration,
            member,
        } => (
            Some(key(&NodeId::Source(*declaration), names)?),
            None,
            member.clone(),
        ),
        Resolved::External(value) => (
            Some(key(&NodeId::External(value.clone()), names)?),
            Some(path(value)?),
            None,
        ),
    };
    Ok(Target {
        declaration,
        route,
        member,
    })
}

impl ContextSnapshot {
    pub fn build(
        origin: &SourceOrigin,
        resolver: &Resolver<'_>,
        references: &References,
        selection: &Selection<'_>,
        graph: &Graph,
        visibility: &syn::Visibility,
    ) -> Result<Self> {
        let names = StableNames::build(resolver, selection)
            .map_err(|err| err.with_kind(Kind::ContextSnapshot))?;
        let relative = origin.source.strip_prefix(&origin.package_root).map_err(|_| error("context snapshots require a source under the package root; no absolute checkout path is used as an identity"))?;
        let components = relative
            .components()
            .map(|part| match part {
                Component::Normal(part) => part
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| error("context snapshot source identity must be UTF-8")),
                _ => Err(error(
                    "context snapshot source identity must be package-relative",
                )),
            })
            .collect::<Result<Vec<_>>>()?;
        let [syn::Item::Fn(function)] = references.generated.file.items.as_slice() else {
            return Err(error(
                "context snapshot requires a single analyzed function",
            ));
        };
        let mut function = function.clone();
        function.vis = visibility.clone();
        let mut declarations = BTreeMap::new();
        for (id, node) in &graph.nodes {
            let (role, syntax, reuse_route) = match &node.kind {
                NodeKind::Original(route) => (
                    "original",
                    Some(item(resolver, id)?.to_token_stream().to_string()),
                    Some(path(route)?),
                ),
                NodeKind::External => ("external", None, None),
                NodeKind::Local(kind) => (
                    match kind {
                        LocalKind::Struct => "copied_struct",
                        LocalKind::Enum => "copied_enum",
                        LocalKind::Alias => "copied_alias",
                    },
                    Some(item(resolver, id)?.to_token_stream().to_string()),
                    None,
                ),
            };
            let mut implementations = match id {
                NodeId::Source(id) => resolver
                    .implementations
                    .get(id)
                    .into_iter()
                    .flatten()
                    .map(|item| item.to_token_stream().to_string())
                    .collect::<Vec<_>>(),
                NodeId::External(_) => Vec::new(),
            };
            implementations.sort();
            let mut dependencies = node
                .references
                .iter()
                .map(|reference| {
                    Ok(Dependency {
                        path: reference.path.clone(),
                        usage: usage(reference.usage).into(),
                        target: target(&reference.target, &names)?,
                        indirect: reference.indirect,
                        layout: reference.layout,
                        expansion: reference.expansion,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            dependencies.sort();
            let mut edges = node
                .edges
                .iter()
                .map(|(id, edge)| {
                    Ok(Edge {
                        target: key(id, &names)?,
                        unguarded_layout: edge.unguarded_layout,
                        alias_expansion: edge.alias_expansion,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            edges.sort();
            if declarations
                .insert(
                    key(id, &names)?,
                    Declaration {
                        role: role.into(),
                        syntax,
                        reuse_route,
                        implementations,
                        references: dependencies,
                        edges,
                    },
                )
                .is_some()
            {
                return Err(error(
                    "ambiguous stable declaration identity; no context baseline can be approved",
                ));
            }
        }
        if references.occurrences.len() != graph.root_references.len() {
            return Err(error("context snapshot reference/graph length mismatch"));
        }
        let mappings = references
            .occurrences
            .iter()
            .zip(&graph.root_references)
            .map(|(reference, resolved)| {
                Ok(Mapping {
                    origin: match reference.site.origin {
                        crate::diagnostics::Origin::Source => "source",
                        crate::diagnostics::Origin::Signature => "signature",
                        crate::diagnostics::Origin::Output => "output",
                    }
                    .into(),
                    path: reference.path.clone(),
                    usage: usage(reference.usage).into(),
                    target: target(resolved, &names)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut snapshot = Self {
            format: FORMAT.into(),
            version: VERSION,
            extraction: function.to_token_stream().to_string(),
            required_inputs: references.required_inputs.iter().cloned().collect(),
            source: Source {
                package: origin.package_name.clone(),
                library: origin.target_name.clone(),
                path: components.join("/"),
                owner: names.selection().into(),
            },
            references: mappings,
            declarations,
        };
        snapshot.normalize()?;
        Ok(snapshot)
    }

    /// Plain TOML review data. The caller decides how to display/copy it; never IO.
    pub fn render(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|err| error(format!("cannot serialize context review data: {err}")))
    }

    pub fn check(&self, text: &str) -> Result<()> {
        let value: toml::Value = toml::from_str(text)
            .map_err(|err| error(format!("invalid context snapshot TOML: {err}")))?;
        if value.get("format").and_then(toml::Value::as_str) != Some(FORMAT) {
            return Err(error(
                "expected format = \"checkmate-context\"; a snippet snapshot is not a context baseline",
            ));
        }
        if value.get("version").and_then(toml::Value::as_integer) != Some(i64::from(VERSION)) {
            return Err(error(
                "unsupported context snapshot version; expected version = 1; versions are never migrated automatically",
            ));
        }
        let mut before: Self = value
            .try_into()
            .map_err(|err| error(format!("invalid context snapshot schema: {err}")))?;
        before.normalize()?;
        if before == *self {
            return Ok(());
        }
        Err(error(difference(&before, self)))
    }

    fn normalize(&mut self) -> Result<()> {
        self.extraction = syn::parse_str::<syn::ItemFn>(&self.extraction.replace("\r\n", "\n"))
            .map_err(|err| {
                error(format!(
                    "invalid extraction syntax in context snapshot: {err}"
                ))
            })?
            .to_token_stream()
            .to_string();
        for mapping in &mut self.references {
            mapping.path = tokens(&mapping.path)?;
            normalize_target(&mut mapping.target)?;
        }
        self.required_inputs.sort();
        for declaration in self.declarations.values_mut() {
            if let Some(syntax) = &mut declaration.syntax {
                *syntax = syn::parse_str::<syn::Item>(&syntax.replace("\r\n", "\n"))
                    .map_err(|err| {
                        error(format!(
                            "invalid declaration syntax in context snapshot: {err}"
                        ))
                    })?
                    .to_token_stream()
                    .to_string();
            }
            if let Some(route) = &mut declaration.reuse_route {
                *route = tokens(route)?;
            }
            for implementation in &mut declaration.implementations {
                *implementation =
                    syn::parse_str::<syn::ItemImpl>(&implementation.replace("\r\n", "\n"))
                        .map_err(|err| {
                            error(format!(
                                "invalid implementation syntax in context snapshot: {err}"
                            ))
                        })?
                        .to_token_stream()
                        .to_string();
            }
            declaration.implementations.sort();
            for reference in &mut declaration.references {
                reference.path = tokens(&reference.path)?;
                normalize_target(&mut reference.target)?;
            }
            declaration.references.sort();
            declaration.edges.sort();
        }
        Ok(())
    }
}
fn item<'a>(resolver: &Resolver<'a>, id: &NodeId) -> Result<&'a syn::Item> {
    let NodeId::Source(id) = id else {
        return Err(error("external graph node has no source declaration"));
    };
    resolver.catalog.item(*id)
}
fn tokens(text: &str) -> Result<String> {
    text.replace("\r\n", "\n")
        .parse::<TokenStream>()
        .map(|tokens| tokens.to_string())
        .map_err(|err| error(format!("invalid tokens in context snapshot: {err}")))
}
fn usage(usage: crate::resolve::ReferenceUse) -> &'static str {
    match usage {
        crate::resolve::ReferenceUse::Type => "type",
        crate::resolve::ReferenceUse::Constructor => "constructor",
        crate::resolve::ReferenceUse::Value => "value",
        crate::resolve::ReferenceUse::TypeOrConst => "type_or_const",
    }
}
fn normalize_target(target: &mut Target) -> Result<()> {
    if let Some(route) = &mut target.route {
        *route = tokens(route)?;
    }
    Ok(())
}
fn short(text: &str) -> String {
    format!("{:?}", crate::source::bounded_line(text))
}
fn difference(before: &ContextSnapshot, after: &ContextSnapshot) -> String {
    let mut message = String::from("context snapshot differs; ");
    let added: Vec<_> = after
        .declarations
        .keys()
        .filter(|key| !before.declarations.contains_key(*key))
        .collect();
    let removed: Vec<_> = before
        .declarations
        .keys()
        .filter(|key| !after.declarations.contains_key(*key))
        .collect();
    let changed: Vec<_> = after
        .declarations
        .iter()
        .filter(|(key, value)| {
            before
                .declarations
                .get(*key)
                .is_some_and(|old| old != *value)
        })
        .map(|(key, _)| key)
        .collect();
    for (label, keys) in [
        ("added declarations", &added),
        ("removed declarations", &removed),
        ("changed declarations", &changed),
    ] {
        if !keys.is_empty() {
            message.push_str(&format!(
                "{label} ({}): {}; ",
                keys.len(),
                keys.iter()
                    .take(3)
                    .map(|key| short(key))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if before.source != after.source {
        message.push_str(&format!(
            "source identity/owner changed: expected {}, actual {}; ",
            short(&format!("{:?}", before.source)),
            short(&format!("{:?}", after.source))
        ));
    }
    if before.extraction != after.extraction {
        message.push_str(&format!(
            "extraction syntax changed: expected {}, actual {}; ",
            short(&before.extraction),
            short(&after.extraction)
        ));
    }
    if before.required_inputs != after.required_inputs {
        message.push_str("required inputs changed; ");
    }
    if before.references != after.references {
        message.push_str("resolved reference mapping changed; ");
    }
    if let Some(key) = changed.first() {
        let old = &before.declarations[*key];
        let new = &after.declarations[*key];
        if old.syntax != new.syntax {
            message.push_str(&format!(
                "declaration content changed: expected {}, actual {}; ",
                short(old.syntax.as_deref().unwrap_or("<none>")),
                short(new.syntax.as_deref().unwrap_or("<none>"))
            ));
        }
        if old.implementations != new.implementations {
            message.push_str("inherent implementation content changed; ");
        }
        if old.references != new.references
            || old.edges != new.edges
            || old.reuse_route != new.reuse_route
        {
            message.push_str("declaration dependency mapping changed; ");
        }
    }
    message.push_str("review a fresh context_report before manually replacing the baseline. No files were changed.");
    message
}

#[cfg(test)]
mod tests;
