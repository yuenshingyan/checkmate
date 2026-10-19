//! Deterministic dependency discovery, not declaration promotion or code generation.
//! Reused originals are terminals: inspecting/copying their private implementation
//! dependencies would be unnecessary and could break their identity/privacy.

mod collect;

use std::collections::{BTreeMap, BTreeSet};

use super::catalog::error;
use super::declarations::{QualifiedPath, Resolved, Resolver};
use super::{ReferenceUse, References};
use crate::scope::DeclarationId;
use crate::source::ast::{Selection, SourceRange};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum NodeId {
    Source(DeclarationId),
    External(QualifiedPath),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalKind {
    Struct,
    Enum,
    Alias,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NodeKind {
    Original(QualifiedPath),
    Local(LocalKind),
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Dependency {
    pub path: String,
    pub range: SourceRange,
    pub usage: ReferenceUse,
    pub target: Resolved,
    // Proven pointer/container indirection, not a claim about arbitrary wrappers.
    pub indirect: bool,
    pub layout: bool,
    pub expansion: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Edge {
    // Parallel guarded/unguarded references must not hide a layout cycle.
    pub unguarded_layout: bool,
    // Alias RHS expansion differs from where-clause constraints and from layout.
    pub alias_expansion: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub kind: NodeKind,
    pub edges: BTreeMap<NodeId, Edge>,
    pub references: Vec<Dependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Graph {
    pub roots: BTreeSet<NodeId>,
    pub root_references: Vec<Resolved>,
    pub nodes: BTreeMap<NodeId, Node>,
    // Item names are hoisted in Rust. Stable ID order supports recursive groups;
    // this is deliberately NOT a claimed topological ordering of a DAG.
    pub emission_order: Vec<NodeId>,
}

impl Graph {
    pub fn build(
        resolver: &Resolver<'_>,
        references: &References,
        selection: &Selection<'_>,
    ) -> syn::Result<Self> {
        let roots = resolver.references(references, selection)?;
        Self::from_roots(resolver, roots, selection.range)
    }

    fn from_roots(
        resolver: &Resolver<'_>,
        roots: Vec<Resolved>,
        selection: SourceRange,
    ) -> syn::Result<Self> {
        let mut builder = Builder {
            resolver,
            selection,
            graph: Graph {
                roots: BTreeSet::new(),
                root_references: roots.clone(),
                nodes: BTreeMap::new(),
                emission_order: Vec::new(),
            },
            pending: BTreeSet::new(),
            parents: BTreeMap::new(),
        };
        for root in roots {
            if let Some(id) = builder.insert(&root)? {
                builder.graph.roots.insert(id.clone());
                builder.parents.insert(id, None);
            }
        }
        // Every node is expanded once. Inserting an existing node never queues it
        // again, so recursive discovery cannot overflow a recursive DFS stack.
        while let Some(id) = builder.pending.pop_first() {
            let NodeId::Source(declaration) = id else {
                unreachable!()
            };
            let mut references = collect::dependencies(resolver, declaration)
                .map_err(|err| builder.failure(&id, &err.to_string()))?;
            references.extend(
                super::implementations::dependencies(resolver, declaration)
                    .map_err(|err| builder.failure(&id, &err.to_string()))?,
            );
            for dependency in &references {
                if let Some(child) = builder
                    .insert(&dependency.target)
                    .map_err(|err| builder.failure(&id, &err.to_string()))?
                {
                    builder
                        .parents
                        .entry(child.clone())
                        .or_insert_with(|| Some(id.clone()));
                    builder
                        .graph
                        .nodes
                        .get_mut(&id)
                        .expect("queued node exists")
                        .edges
                        .entry(child)
                        .and_modify(|edge| {
                            edge.unguarded_layout |= dependency.layout && !dependency.indirect;
                            edge.alias_expansion |= dependency.expansion;
                        })
                        .or_insert(Edge {
                            unguarded_layout: dependency.layout && !dependency.indirect,
                            alias_expansion: dependency.expansion,
                        });
                }
            }
            builder
                .graph
                .nodes
                .get_mut(&id)
                .expect("queued node exists")
                .references = references;
        }
        // Validate on the COMPLETE graph. Checking only discovery back-edges can
        // miss a direct cycle first reached via an unrelated guarded route.
        if let Some(cycle) = cycle(&builder.graph.nodes, true) {
            return Err(builder.failure(
                &cycle[0],
                &format!("cyclic type alias expansion: {}", builder.labels(&cycle)),
            ));
        }
        if let Some(cycle) = cycle(&builder.graph.nodes, false) {
            return Err(builder.failure(
                &cycle[0],
                &format!(
                    "recursive type dependency without proven indirection: {}",
                    builder.labels(&cycle)
                ),
            ));
        }
        builder.graph.emission_order = builder.graph.nodes.keys().cloned().collect();
        Ok(builder.graph)
    }
}

struct Builder<'a, 'source> {
    resolver: &'a Resolver<'source>,
    selection: SourceRange,
    graph: Graph,
    pending: BTreeSet<NodeId>,
    parents: BTreeMap<NodeId, Option<NodeId>>,
}

impl Builder<'_, '_> {
    fn insert(&mut self, resolved: &Resolved) -> syn::Result<Option<NodeId>> {
        let (id, kind) = match resolved {
            Resolved::Binding => return Ok(None),
            Resolved::External(path) => (NodeId::External(path.clone()), NodeKind::External),
            Resolved::Original {
                declaration, base, ..
            } => (
                NodeId::Source(*declaration),
                NodeKind::Original(base.clone()),
            ),
            Resolved::LocalType { declaration, .. } => {
                let kind = match self.resolver.catalog.item(*declaration)? {
                    syn::Item::Struct(_) => LocalKind::Struct,
                    syn::Item::Enum(_) => LocalKind::Enum,
                    syn::Item::Type(_) => LocalKind::Alias,
                    _ => return Err(error("unsupported local declaration in dependency graph")),
                };
                (NodeId::Source(*declaration), NodeKind::Local(kind))
            }
        };
        if let Some(existing) = self.graph.nodes.get_mut(&id) {
            match (&mut existing.kind, kind) {
                (NodeKind::Original(old), NodeKind::Original(new)) => {
                    if new < *old {
                        *old = new;
                    }
                }
                (old, new) if *old == new => {}
                _ => {
                    return Err(error(
                        "one declaration identity has conflicting reuse/promotion classifications",
                    ));
                }
            }
        } else {
            if matches!(kind, NodeKind::Local(_)) {
                self.pending.insert(id.clone());
            }
            self.graph.nodes.insert(
                id.clone(),
                Node {
                    kind,
                    edges: BTreeMap::new(),
                    references: Vec::new(),
                },
            );
        }
        Ok(Some(id))
    }

    fn labels(&self, nodes: &[NodeId]) -> String {
        nodes
            .iter()
            .map(|id| match id {
                NodeId::Source(id) => self.resolver.catalog.index.declarations[id.0].name.clone(),
                NodeId::External(path) => path.segments.join("::"),
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn failure(&self, node: &NodeId, message: &str) -> syn::Error {
        let mut chain = vec![node.clone()];
        let mut next = node;
        while let Some(Some(parent)) = self.parents.get(next) {
            chain.push(parent.clone());
            next = parent;
        }
        chain.reverse();
        error(&format!(
            "{}:{}-{}: dependency chain {}: {message}",
            self.resolver.catalog.index.source_path.display(),
            self.selection.start.line,
            self.selection.end.line - 1,
            self.labels(&chain)
        ))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Visiting,
    Visited,
}

/// An alias-only cycle is illegal even behind Box. A general cycle is rejected
/// only in the subgraph of unguarded edges. The latter still rejects opaque
/// recursive containers conservatively; rustc checks all remaining layout rules.
fn cycle(nodes: &BTreeMap<NodeId, Node>, aliases: bool) -> Option<Vec<NodeId>> {
    let eligible =
        |id: &NodeId| !aliases || matches!(nodes[id].kind, NodeKind::Local(LocalKind::Alias));
    let children = |id: &NodeId| {
        nodes[id]
            .edges
            .iter()
            .filter(|(child, edge)| {
                eligible(child)
                    && if aliases {
                        edge.alias_expansion
                    } else {
                        edge.unguarded_layout
                    }
            })
            .map(|(child, _)| child.clone())
            .collect::<Vec<_>>()
    };
    let mut states = BTreeMap::new();
    for root in nodes.keys().filter(|id| eligible(id)) {
        if states.contains_key(root) {
            continue;
        }
        states.insert(root.clone(), State::Visiting);
        let mut stack = vec![(root.clone(), 0, children(root))];
        while let Some((id, offset, neighbors)) = stack.last_mut() {
            if *offset == neighbors.len() {
                states.insert(id.clone(), State::Visited);
                stack.pop();
                continue;
            }
            let child = neighbors[*offset].clone();
            *offset += 1;
            match states.get(&child) {
                Some(State::Visited) => {}
                Some(State::Visiting) => {
                    let start = stack
                        .iter()
                        .position(|(id, _, _)| *id == child)
                        .expect("visiting node is active");
                    let mut cycle: Vec<_> =
                        stack[start..].iter().map(|(id, _, _)| id.clone()).collect();
                    cycle.push(child);
                    return Some(cycle);
                }
                None => {
                    states.insert(child.clone(), State::Visiting);
                    let neighbors = children(&child);
                    stack.push((child, 0, neighbors));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod properties;
#[cfg(test)]
mod tests;
