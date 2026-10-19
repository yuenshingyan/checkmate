//! Logical source identities. Ranges and index IDs are joins only, never output.
use std::collections::BTreeMap;

use crate::diagnostics::{Error, Kind, Result};
use crate::resolve::catalog::{canonical, item_ident};
use crate::resolve::declarations::Resolver;
use crate::scope::DeclarationId;
use crate::source::ast::{Selection, SourceRange};
use syn::visit::{self, Visit};

pub(super) struct StableNames {
    declarations: BTreeMap<DeclarationId, Option<String>>,
    selection: String,
}

#[derive(Clone, Copy)]
enum Parent {
    Root,
    Node(usize),
    Owner(DeclarationId),
    Unsupported,
}

struct Node {
    parent: Parent,
    segment: String,
}

fn failure(message: &str) -> Error {
    Error::new(proc_macro2::Span::call_site(), message).with_kind(Kind::UnsupportedContext)
}

impl StableNames {
    pub fn build(resolver: &Resolver<'_>, selection: &Selection<'_>) -> Result<Self> {
        struct Collector<'a, 'r> {
            resolver: &'r Resolver<'a>,
            block: SourceRange,
            selected: Option<Parent>,
            parent: Parent,
            nodes: Vec<Node>,
            declarations: BTreeMap<DeclarationId, usize>,
            identities:
                BTreeMap<(proc_macro2::LineColumn, proc_macro2::LineColumn, String), DeclarationId>,
        }
        impl Collector<'_, '_> {
            fn enter(&mut self, kind: &str, ident: &syn::Ident) -> usize {
                let id = self.nodes.len();
                self.nodes.push(Node {
                    parent: self.parent,
                    segment: format!("{kind}:{}", canonical(&ident.to_string())),
                });
                self.parent = Parent::Node(id);
                id
            }
        }
        impl<'ast> Visit<'ast> for Collector<'_, '_> {
            fn visit_item(&mut self, item: &'ast syn::Item) {
                let saved = self.parent;
                if let syn::Item::Impl(implementation) = item {
                    // The resolver, not the self-type's spelling, establishes ownership.
                    self.parent = self
                        .resolver
                        .implementations
                        .iter()
                        .find_map(|(owner, items)| {
                            items
                                .iter()
                                .any(|candidate| {
                                    SourceRange::of(*candidate) == SourceRange::of(implementation)
                                })
                                .then_some(Parent::Owner(*owner))
                        })
                        .unwrap_or(Parent::Unsupported);
                } else if let Some(ident) = item_ident(item) {
                    let kind = match item {
                        syn::Item::Fn(_) => "fn",
                        syn::Item::Mod(_) => "mod",
                        syn::Item::Struct(_) => "struct",
                        syn::Item::Enum(_) => "enum",
                        syn::Item::Type(_) => "type",
                        syn::Item::Const(_) => "const",
                        syn::Item::Static(_) => "static",
                        syn::Item::Macro(_) => "macro",
                        _ => unreachable!(),
                    };
                    let node = self.enter(kind, ident);
                    let range = SourceRange::of(ident);
                    let identity = (
                        range.start,
                        range.end,
                        canonical(&ident.to_string()).to_owned(),
                    );
                    if let Some(&id) = self.identities.get(&identity) {
                        self.declarations.insert(id, node);
                    }
                } else if !matches!(item, syn::Item::Use(_) | syn::Item::ExternCrate(_)) {
                    self.parent = Parent::Unsupported;
                }
                visit::visit_item(self, item);
                self.parent = saved;
            }

            fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
                let saved = self.parent;
                self.enter("method", &method.sig.ident);
                visit::visit_impl_item_fn(self, method);
                self.parent = saved;
            }

            fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
                let saved = self.parent;
                self.enter("const", &item.ident);
                visit::visit_impl_item_const(self, item);
                self.parent = saved;
            }

            fn visit_block(&mut self, block: &'ast syn::Block) {
                if SourceRange::of(block) == self.block {
                    self.selected = Some(self.parent);
                }
                // Anonymous blocks/closures deliberately introduce no ordinal.
                visit::visit_block(self, block);
            }
        }
        let mut collector = Collector {
            resolver,
            block: SourceRange::of(selection.block),
            selected: None,
            parent: Parent::Root,
            nodes: Vec::new(),
            declarations: BTreeMap::new(),
            identities: resolver
                .catalog
                .index
                .declarations
                .iter()
                .enumerate()
                .map(|(id, declaration)| {
                    (
                        (
                            declaration.range.start,
                            declaration.range.end,
                            declaration.name.clone(),
                        ),
                        DeclarationId(id),
                    )
                })
                .collect(),
        };
        collector.visit_file(&resolver.catalog.source.file);
        let nodes = &collector.nodes;
        let declarations = &collector.declarations;
        fn target(parent: Parent, declarations: &BTreeMap<DeclarationId, usize>) -> Option<usize> {
            match parent {
                Parent::Node(id) => Some(id),
                Parent::Owner(id) => declarations.get(&id).copied(),
                _ => None,
            }
        }
        fn key(
            id: usize,
            nodes: &[Node],
            declarations: &BTreeMap<DeclarationId, usize>,
            active: &mut Vec<usize>,
            keys: &mut [Option<String>],
            done: &mut [bool],
        ) -> Result<Option<String>> {
            if done[id] {
                return Ok(keys[id].clone());
            }
            if active.contains(&id) {
                return Err(failure("cyclic logical source ownership"));
            }
            active.push(id);
            let parent = nodes[id].parent;
            let prefix = if let Some(parent) = target(parent, declarations) {
                key(parent, nodes, declarations, active, keys, done)?
            } else if matches!(parent, Parent::Root) {
                Some("crate".to_owned())
            } else {
                None
            };
            keys[id] = prefix.map(|prefix| format!("{prefix}/{}", nodes[id].segment));
            done[id] = true;
            active.pop();
            Ok(keys[id].clone())
        }
        let mut keys = vec![None; nodes.len()];
        let mut done = vec![false; nodes.len()];
        for id in 0..nodes.len() {
            key(
                id,
                nodes,
                declarations,
                &mut Vec::new(),
                &mut keys,
                &mut done,
            )?;
        }
        let mut counts = BTreeMap::new();
        for key in keys.iter().flatten() {
            *counts.entry(key.as_str()).or_insert(0usize) += 1;
        }
        let unique = |mut id: usize| {
            loop {
                if keys[id]
                    .as_ref()
                    .is_none_or(|key| counts[key.as_str()] != 1)
                {
                    return false;
                }
                match target(nodes[id].parent, declarations) {
                    Some(parent) => id = parent,
                    None => return matches!(nodes[id].parent, Parent::Root),
                }
            }
        };
        let selected = collector
            .selected
            .ok_or_else(|| failure("selection has no source identity"))?;
        let selection = match target(selected, declarations) {
            Some(id) if unique(id) => keys[id].clone().unwrap(),
            _ => {
                return Err(failure(
                    "selection has an ambiguous or unsupported logical source anchor",
                ));
            }
        };
        let declarations = declarations
            .iter()
            .map(|(&id, &node)| (id, unique(node).then(|| keys[node].clone()).flatten()))
            .collect();
        Ok(Self {
            declarations,
            selection,
        })
    }

    pub fn declaration(&self, id: DeclarationId) -> Result<&str> {
        self.declarations
            .get(&id)
            .and_then(|key| key.as_deref())
            .ok_or_else(|| {
                failure("declaration has an ambiguous or unsupported logical source anchor")
            })
    }

    pub fn selection(&self) -> &str {
        &self.selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::declarations::ReuseSite;
    use crate::resolve::{collect_references, graph::Graph};
    use crate::scope::ScopeIndex;
    use crate::source::{ast::ParsedSource, origin::SourceOrigin};

    struct Fixture {
        _temp: tempfile::TempDir,
        source: ParsedSource,
        index: ScopeIndex,
        origin: SourceOrigin,
    }
    impl Fixture {
        fn new(text: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            std::fs::create_dir(temp.path().join("src")).unwrap();
            std::fs::write(
                temp.path().join("Cargo.toml"),
                "[package]\nname = \"identity-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .unwrap();
            std::fs::write(temp.path().join("src/lib.rs"), text).unwrap();
            let source = ParsedSource::parse("src/lib.rs", text).unwrap();
            let index = ScopeIndex::build(&source).unwrap();
            let origin = SourceOrigin::discover(temp.path(), &source.path).unwrap();
            Self {
                _temp: temp,
                source,
                index,
                origin,
            }
        }
        fn names(&self) -> Result<StableNames> {
            let line = self
                .source
                .text
                .lines()
                .position(|line| line.contains("let selected"))
                .unwrap()
                + 1;
            let selection = self.source.select(line, line).unwrap();
            let resolver = Resolver::new(
                &self.source,
                &self.index,
                &self.origin,
                ReuseSite::SameTarget,
            )
            .unwrap();
            StableNames::build(&resolver, &selection)
        }
        fn ids(&self, name: &str) -> Vec<DeclarationId> {
            self.index
                .declarations
                .iter()
                .enumerate()
                .filter(|(_, d)| d.name == name)
                .map(|(id, _)| DeclarationId(id))
                .collect()
        }
    }

    #[test]
    fn independent_checkouts_and_insertions_preserve_graph_anchors() {
        let original =
            "mod m { fn outer() { fn inner() {\nstruct Local;\nlet selected = ();\n} } }\n";
        let changed = "fn unrelated() {}\nmod before {}\nmod m { fn outer() { {} fn inner() {\n{}\nstruct Local;\nlet selected = ();\n} } }\n";
        let mut snapshots = Vec::new();
        for text in [original, original, changed] {
            let fixture = Fixture::new(text);
            let names = fixture.names().unwrap();
            let line = text
                .lines()
                .position(|s| s.contains("let selected"))
                .unwrap()
                + 1;
            let selection = fixture.source.select(line, line).unwrap();
            let resolver = Resolver::new(
                &fixture.source,
                &fixture.index,
                &fixture.origin,
                ReuseSite::SameTarget,
            )
            .unwrap();
            let references = collect_references(
                &fixture.source,
                &fixture.index,
                &selection,
                &syn::parse_str("fn test(value: Local)").unwrap(),
                None,
            )
            .unwrap();
            let graph = Graph::build(&resolver, &references, &selection).unwrap();
            assert!(!graph.nodes.is_empty());
            snapshots.push((
                names.selection().to_owned(),
                names
                    .declaration(fixture.ids("Local")[0])
                    .unwrap()
                    .to_owned(),
            ));
        }
        assert!(snapshots.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(snapshots[0].1, "crate/mod:m/fn:outer/fn:inner/struct:Local");
    }

    #[test]
    fn source_wide_duplicates_are_lazy_but_ancestors_are_checked() {
        let fixture = Fixture::new(
            "fn f() {\n{ struct D; }\n{ struct D; }\nstruct Unique;\nlet selected = ();\n}\n",
        );
        let names = fixture.names().unwrap();
        assert!(names.declaration(fixture.ids("Unique")[0]).is_ok());
        for id in fixture.ids("D") {
            assert!(names.declaration(id).is_err());
        }
        let fixture = Fixture::new(
            "fn f() {\n{ fn same() { struct Unique; } }\n{ fn same() {} }\nlet selected = ();\n}\n",
        );
        assert!(
            fixture
                .names()
                .unwrap()
                .declaration(fixture.ids("Unique")[0])
                .is_err()
        );
        let fixture =
            Fixture::new("fn f() {\n{ fn same() {\nlet selected = ();\n} }\n{ fn same() {} }\n}\n");
        assert!(fixture.names().is_err());
    }

    #[test]
    fn resolved_impl_owner_not_alias_or_impl_position_controls_identity() {
        fn fixture(header: &str, target: &str, extra: &str) -> Fixture {
            Fixture::new(&format!(
                "mod a {{ pub struct Owner; }}\nmod b {{ pub struct Owner; }}\nuse {target}::Owner as Alias;\n{extra}\nimpl {header} {{ fn r#run() {{\nstruct Local;\nlet selected = ();\n}} }}\n"
            ))
        }
        let a = fixture("Alias", "a", "");
        let moved = fixture("crate::a::Owner", "a", "impl a::Owner { fn other() {} }");
        let b = fixture("Alias", "b", "");
        let na = a.names().unwrap();
        let nm = moved.names().unwrap();
        let nb = b.names().unwrap();
        assert_eq!(na.selection(), "crate/mod:a/struct:Owner/method:run");
        assert_eq!(na.selection(), nm.selection());
        assert_ne!(na.selection(), nb.selection());
        assert_eq!(
            na.declaration(a.ids("Local")[0]).unwrap(),
            nm.declaration(moved.ids("Local")[0]).unwrap()
        );
        assert_ne!(
            na.declaration(a.ids("Local")[0]).unwrap(),
            nb.declaration(b.ids("Local")[0]).unwrap()
        );
    }
}
