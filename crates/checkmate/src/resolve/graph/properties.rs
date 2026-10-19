//! Bounded, seed-free properties. No rustc subprocesses or property-test dependency.
//! Registered only for tests; these do not expose automatic context publicly.
use super::*;
use crate::diagnostics::Result;
use crate::resolve::declarations::ReuseSite;
use crate::resolve::declarations::tests::Fixture;

// Independent oracle: transitive closure, not the production DFS/back-edge algorithm.
// Starting with adjacency (not an identity matrix) makes a diagonal entry mean
// a nonempty closed walk. Constraints are deliberately absent from both matrices.
fn cyclic(mut reach: Vec<Vec<bool>>) -> bool {
    for k in 0..reach.len() {
        for i in 0..reach.len() {
            for j in 0..reach.len() {
                reach[i][j] |= reach[i][k] && reach[k][j];
            }
        }
    }
    (0..reach.len()).any(|i| reach[i][i])
}

fn synthetic_id(i: usize) -> NodeId {
    NodeId::Source(DeclarationId(i))
}

// States: absent, constraint-only, guarded expansion, layout-only,
// unguarded expansion. Layout-only also exercises independence of the flags.
fn check_model(n: usize, alias_mask: usize, states: &[usize]) {
    let mut nodes = BTreeMap::new();
    let mut expansion = vec![vec![false; n]; n];
    let mut layout = expansion.clone();
    for i in 0..n {
        let mut edges = BTreeMap::new();
        for j in 0..n {
            let state = states[i * n + j];
            layout[i][j] = state == 3 || state == 4;
            expansion[i][j] = (state == 2 || state == 4)
                && alias_mask & (1 << i) != 0
                && alias_mask & (1 << j) != 0;
            if state != 0 {
                edges.insert(
                    synthetic_id(j),
                    Edge {
                        unguarded_layout: state == 3 || state == 4,
                        alias_expansion: state == 2 || state == 4,
                    },
                );
            }
        }
        nodes.insert(
            synthetic_id(i),
            Node {
                kind: NodeKind::Local(if alias_mask & (1 << i) != 0 {
                    LocalKind::Alias
                } else {
                    LocalKind::Struct
                }),
                edges,
                references: Vec::new(),
            },
        );
    }
    for (aliases, adjacency) in [(true, expansion), (false, layout)] {
        let witness = cycle(&nodes, aliases);
        assert_eq!(
            witness.is_some(),
            cyclic(adjacency.clone()),
            "n={n}, aliases={alias_mask:b}, states={states:?}, mode={aliases}"
        );
        if let Some(path) = witness {
            assert!(path.len() >= 2 && path.len() <= n + 1);
            assert_eq!(path.first(), path.last());
            assert_eq!(
                path[..path.len() - 1].iter().collect::<BTreeSet<_>>().len(),
                path.len() - 1
            );
            for pair in path.windows(2) {
                let index = |id: &NodeId| match id {
                    NodeId::Source(id) => id.0,
                    _ => unreachable!(),
                };
                assert!(adjacency[index(&pair[0])][index(&pair[1])]);
            }
        }
    }
}

#[test]
fn bounded_cycles_match_transitive_closure() {
    // All 5^4 edge assignments and 2^2 kind assignments: 2,500 models.
    // Includes self loops, disconnected nodes and mixed constraint/real cycles.
    for code in 0..625 {
        let states: Vec<_> = (0..4).map(|i| code / 5usize.pow(i) % 5).collect();
        for kinds in 0..4 {
            check_model(2, kinds, &states);
        }
    }
    // All three-node directed topologies, including loops, with all alias
    // subsets and three edge palettes: 512 * 8 * 3 = 12,288 models.
    // Missing topology edges become constraints in one palette, so those
    // dependencies must never complete a cycle in either filtered graph.
    for topology in 0..512 {
        for kinds in 0..8 {
            for palette in 0..3 {
                let states: Vec<_> = (0..9)
                    .map(|bit| {
                        if topology & (1 << bit) != 0 {
                            if palette == 0 { 2 } else { 4 }
                        } else if palette == 2 {
                            1
                        } else {
                            0
                        }
                    })
                    .collect();
                check_model(3, kinds, &states);
            }
        }
    }
}

fn fixture(items: &str) -> Fixture {
    Fixture::new(&format!(
        "pub fn source() {{\n{items}\nlet selected = ();\n}}\n"
    ))
}

fn build(f: &Fixture, names: &[&str]) -> Result<Graph> {
    let resolver = f.resolver(ReuseSite::SameTarget);
    let at = f.position("let selected");
    let roots = names
        .iter()
        .map(|name| {
            resolver.path(
                f.index.scope_at(at).unwrap(),
                &syn::parse_str(name).unwrap(),
                ReferenceUse::Type,
                at,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Graph::from_roots(
        &resolver,
        roots,
        f.source.select(at.line, at.line).unwrap().range,
    )
}

fn named_id(f: &Fixture, name: &str) -> NodeId {
    synthetic_id(
        f.index
            .declarations
            .iter()
            .position(|d| d.name == name)
            .unwrap(),
    )
}

#[test]
fn generated_source_cycles_distinguish_expansion_layout_and_constraints() {
    // 4 kind assignments * 4^2 reference forms = 64 sources. Both roots
    // are requested so a constraint-only edge cannot hide an undiscovered node.
    for kinds in 0..4 {
        for code in 0..16 {
            let forms = [code % 4, code / 4];
            let mut items = String::new();
            for i in 0..2 {
                let name = ["A", "B"][i];
                let target = ["B", "A"][i];
                let (constraint, ty) = match forms[i] {
                    0 => (format!("where {target}: Sized"), "()".to_owned()),
                    1 => (String::new(), format!("*const {target}")),
                    2 => (String::new(), target.to_owned()),
                    _ => (String::new(), format!("(*const {target}, {target})")),
                };
                if kinds & (1 << i) != 0 {
                    items.push_str(&format!("type {name} {constraint} = {ty};\n"));
                } else {
                    items.push_str(&format!("struct {name} {constraint} {{ field: {ty} }}\n"));
                }
            }
            // A two-node ring has exactly one possible cycle; this oracle
            // derives its classification from the source recipe, not graph flags.
            let alias_cycle = kinds == 3 && forms.iter().all(|&f| f != 0);
            let layout_cycle = forms.iter().all(|&f| f >= 2);
            let f = fixture(&items);
            match build(&f, &["A", "B"]) {
                Err(err) => {
                    let message = err.to_string();
                    assert!(alias_cycle || layout_cycle, "{items}\n{message}");
                    assert!(
                        message.contains(if alias_cycle {
                            "cyclic type alias expansion"
                        } else {
                            "recursive type dependency without proven indirection"
                        }),
                        "{items}\n{message}"
                    );
                }
                Ok(graph) => {
                    assert!(!alias_cycle && !layout_cycle, "{items}");
                    for i in 0..2 {
                        let edge = &graph.nodes[&named_id(&f, ["A", "B"][i])].edges
                            [&named_id(&f, ["B", "A"][i])];
                        assert_eq!(edge.unguarded_layout, forms[i] >= 2, "{items}");
                        assert_eq!(edge.alias_expansion, forms[i] != 0, "{items}");
                    }
                }
            }
        }
    }
}

#[test]
fn generated_dags_are_ordered_and_deduplicated_across_roots_and_checkouts() {
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let names = ["R0", "R1", "R2"];
    // All eight DAGs on three ordered roots, plus a shared non-root leaf.
    // Parallel pointer/direct occurrences must merge to an unguarded edge.
    for mask in 0..8 {
        let mut items = String::from("struct Leaf;\n");
        let pairs = [(0, 1), (0, 2), (1, 2)];
        for i in 0..3 {
            items.push_str(&format!("struct R{i} {{ a: *const Leaf, b: Leaf,"));
            for (bit, &(from, to)) in pairs.iter().enumerate() {
                if from == i && mask & (1 << bit) != 0 {
                    items.push_str(&format!(" p{to}: *const R{to}, d{to}: R{to},"));
                }
            }
            items.push_str("}\n");
        }
        let first = fixture(&items);
        let second = fixture(&items);
        assert_ne!(first.temp.path(), second.temp.path());
        let baseline = build(&first, &names).unwrap();
        let mut expected_ids = vec![named_id(&first, "Leaf")];
        expected_ids.extend(names.iter().map(|name| named_id(&first, name)));
        expected_ids.sort();
        assert_eq!(baseline.emission_order, expected_ids);
        assert_eq!(baseline.nodes.len(), 4);
        assert_eq!(
            baseline.roots,
            names.iter().map(|name| named_id(&first, name)).collect()
        );
        assert!(baseline.nodes[&named_id(&first, "Leaf")].edges.is_empty());
        for i in 0..3 {
            let node = &baseline.nodes[&named_id(&first, names[i])];
            let mut targets = BTreeSet::from([named_id(&first, "Leaf")]);
            for (bit, &(from, to)) in pairs.iter().enumerate() {
                if from == i && mask & (1 << bit) != 0 {
                    targets.insert(named_id(&first, names[to]));
                }
            }
            assert_eq!(node.edges.keys().cloned().collect::<BTreeSet<_>>(), targets);
            assert_eq!(node.references.len(), targets.len() * 2);
            assert!(
                node.edges
                    .values()
                    .all(|edge| edge.unguarded_layout && edge.alias_expansion)
            );
        }
        for f in [&first, &second] {
            for p in permutations {
                let roots = [names[p[0]], names[p[1]], names[p[2]], names[p[0]]];
                let actual = build(f, &roots).unwrap();
                assert_eq!(actual.nodes, baseline.nodes);
                assert_eq!(actual.roots, baseline.roots);
                assert_eq!(actual.emission_order, expected_ids);
                // Occurrences intentionally retain order/multiplicity, unlike roots.
                assert_eq!(
                    actual.root_references,
                    [p[0], p[1], p[2], p[0]].map(|i| baseline.root_references[i].clone())
                );
                assert!(!format!("{actual:?}").contains(&f.temp.path().display().to_string()));
            }
        }
    }
}
