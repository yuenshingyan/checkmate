use super::*;
use crate::resolve::collect_references;
use crate::resolve::declarations::ReuseSite;
use crate::resolve::declarations::tests::Fixture;

fn fixture(items: &str) -> Fixture {
    Fixture::new(&format!(
        "pub fn source() {{\n{items}\nlet selected = ();\n}}\n"
    ))
}

fn graph(fixture: &Fixture, names: &[&str]) -> syn::Result<Graph> {
    let resolver = fixture.resolver(ReuseSite::SameTarget);
    let at = fixture.position("let selected");
    let roots = names
        .iter()
        .map(|name| {
            resolver.path(
                fixture.index.scope_at(at).unwrap(),
                &syn::parse_str(name).unwrap(),
                ReferenceUse::Type,
                at,
            )
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Graph::from_roots(
        &resolver,
        roots,
        fixture.source.select(at.line, at.line).unwrap().range,
    )
}

fn id(fixture: &Fixture, name: &str) -> NodeId {
    NodeId::Source(DeclarationId(
        fixture
            .index
            .declarations
            .iter()
            .position(|declaration| declaration.name == name)
            .unwrap(),
    ))
}

#[test]
fn fields_aliases_and_shared_transitive_dependencies_are_discovered_once() {
    let fixture = fixture(
        "struct Address { zip: u32 }\ntype Label = String;\nstruct User { name: Label, other: Label, address: Address }\nstruct Envelope { first: User, second: User }",
    );
    let graph = graph(&fixture, &["Envelope"]).unwrap();
    assert_eq!(graph.nodes.len(), 6);
    assert_eq!(graph.nodes[&id(&fixture, "Envelope")].edges.len(), 1);
    let user = &graph.nodes[&id(&fixture, "User")];
    assert_eq!(user.edges.len(), 2);
    assert_eq!(
        user.references
            .iter()
            .filter(|dependency| dependency.path == "Label")
            .count(),
        2
    );
    assert_eq!(graph.emission_order.len(), graph.nodes.len());
}

#[test]
fn deterministic_discovery_is_independent_of_root_order_and_checkout_location() {
    let text = "struct Shared { n: u8 }\nstruct A { data: Shared }\nstruct B { data: Shared }";
    let first = fixture(text);
    let second = fixture(text);
    let a = graph(&first, &["A", "B"]).unwrap();
    let b = graph(&first, &["B", "A", "A"]).unwrap();
    assert_eq!(a.nodes, b.nodes);
    assert_eq!(a.emission_order, b.emission_order);
    assert_eq!(a.roots, b.roots);
    assert_eq!(a, graph(&second, &["A", "B"]).unwrap());
    assert!(!format!("{a:?}").contains(&first.temp.path().display().to_string()));
}

#[test]
fn recursive_types_terminate_and_preserve_guarded_edges() {
    let fixture = fixture("struct Node { next: Option<Box<Node>> }");
    let graph = graph(&fixture, &["Node"]).unwrap();
    assert_eq!(graph.nodes.len(), 3);
    let node = id(&fixture, "Node");
    assert_eq!(
        graph.nodes[&node]
            .edges
            .get(&node)
            .map(|edge| edge.unguarded_layout),
        Some(false)
    );
    assert_eq!(
        graph
            .emission_order
            .iter()
            .filter(|id| **id == node)
            .count(),
        1
    );
    println!(
        "Node -> Option, Box, Node (recursive Node edge is indirect); 3 unique nodes, no copies emitted."
    );
}

#[test]
fn references_pointers_function_pointers_and_self_are_indirection() {
    let fixture =
        fixture("struct Node { parent: &'static Node, raw: *const Self, factory: fn() -> Node }");
    let graph = graph(&fixture, &["Node"]).unwrap();
    assert_eq!(graph.nodes.len(), 1);
    let node = id(&fixture, "Node");
    assert_eq!(
        graph.nodes[&node]
            .edges
            .get(&node)
            .map(|edge| edge.unguarded_layout),
        Some(false)
    );
}

#[test]
fn mutually_recursive_nominal_types_and_aliases_need_not_form_a_dag() {
    let fixture = fixture("type Link = Option<Box<Node>>;\nstruct Node { next: Link }");
    let graph = graph(&fixture, &["Node"]).unwrap();
    let link = id(&fixture, "Link");
    let node = id(&fixture, "Node");
    assert_eq!(
        graph.nodes[&link]
            .edges
            .get(&node)
            .map(|edge| edge.unguarded_layout),
        Some(false)
    );
    assert_eq!(
        graph.nodes[&node]
            .edges
            .get(&link)
            .map(|edge| edge.unguarded_layout),
        Some(true)
    );
    assert_eq!(
        graph.emission_order.iter().collect::<BTreeSet<_>>().len(),
        4
    );
}

#[test]
fn alias_only_cycles_are_illegal_even_behind_indirection() {
    for items in ["type A = Box<A>;", "type A = Vec<B>;\ntype B = A;"] {
        let fixture = fixture(items);
        let error = graph(&fixture, &["A"]).unwrap_err().to_string();
        assert!(error.contains("cyclic type alias expansion"), "{error}");
        assert!(error.contains("dependency chain A"), "{error}");
    }
}

#[test]
fn direct_and_guarded_occurrences_do_not_hide_an_invalid_layout_cycle() {
    let fixture = fixture("struct Node { good: Box<Node>, bad: Node }");
    let error = graph(&fixture, &["Node"]).unwrap_err().to_string();
    assert!(
        error.contains("without proven indirection: Node -> Node"),
        "{error}"
    );
}

#[test]
fn direct_cycles_are_detected_even_when_discovery_first_takes_a_guarded_route() {
    let fixture = fixture(
        "struct A { guarded: Box<B>, direct: C }\nstruct B { guarded: Box<C> }\nstruct C { direct: A }",
    );
    let error = graph(&fixture, &["A"]).unwrap_err().to_string();
    assert!(
        error.contains("without proven indirection: A -> C -> A"),
        "{error}"
    );
}

#[test]
fn allocator_arguments_are_not_mistaken_for_box_indirection() {
    let fixture = fixture("struct Node { value: Box<u8, Node> }");
    let error = graph(&fixture, &["Node"]).unwrap_err().to_string();
    assert!(error.contains("without proven indirection"), "{error}");
}

#[test]
fn container_shadowing_and_opaque_original_layouts_are_not_guessed() {
    let fixture = Fixture::new(
        "pub struct Wrapper<T> { pub pointer: *const T }\npub fn source() {\nstruct Node { data: Wrapper<Node> }\nlet selected = ();\n}\n",
    );
    assert!(
        graph(&fixture, &["Node"])
            .unwrap_err()
            .to_string()
            .contains("without proven indirection")
    );
    let fixture = Fixture::new(
        "pub struct Box<T> { pub value: T }\npub fn source() {\nstruct Node { data: Box<Node> }\nlet selected = ();\n}\n",
    );
    assert!(
        graph(&fixture, &["Node"])
            .unwrap_err()
            .to_string()
            .contains("without proven indirection")
    );
}

#[test]
fn a_box_import_alias_retains_proven_indirection() {
    let fixture = Fixture::new(
        "use std::boxed::Box as Heap;\npub fn source() {\nstruct Node { data: Option<Heap<Node>> }\nlet selected = ();\n}\n",
    );
    assert!(graph(&fixture, &["Node"]).is_ok());
}

#[test]
fn reused_originals_are_terminal_without_leaking_private_dependencies() {
    let fixture = Fixture::new(
        "struct Hidden;\npub struct Public { hidden: Hidden }\npub fn source() {\nlet selected = ();\n}\n",
    );
    let at = fixture.position("let selected");
    let resolver = fixture.resolver(ReuseSite::PackageIntegrationTest);
    let root = fixture
        .resolve(
            "Public",
            ReferenceUse::Type,
            "let selected",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    let graph = Graph::from_roots(
        &resolver,
        vec![root],
        fixture.source.select(at.line, at.line).unwrap().range,
    )
    .unwrap();
    assert_eq!(graph.nodes.len(), 1);
    let node = &graph.nodes[&id(&fixture, "Public")];
    assert!(matches!(node.kind, NodeKind::Original(_)));
    assert!(node.edges.is_empty());
    assert!(node.references.is_empty());
}

#[test]
fn unresolved_transitive_dependencies_report_the_chain_and_original_location() {
    let fixture = fixture("struct A { b: B }\nstruct B { missing: Missing }");
    let error = graph(&fixture, &["A"]).unwrap_err().to_string();
    assert!(
        error.contains("src/lib.rs:4-4") && error.contains("dependency chain A -> B"),
        "{error}"
    );
    assert!(
        error.contains("source 3:") && error.contains("Missing"),
        "{error}"
    );
    // Failure is transactional: another independent graph can still be built.
    let good = graph(&fixture, &["u8"]).unwrap();
    assert_eq!(good.nodes.len(), 1);
}

#[test]
fn const_paths_are_reused_but_helpers_and_local_constants_are_not_evaluated() {
    let fixture = Fixture::new(
        "pub const COUNT: usize = 2;\npub fn source() {\nstruct Bytes { data: [u8; COUNT + 1] }\nlet selected = ();\n}\n",
    );
    let good = graph(&fixture, &["Bytes"]).unwrap();
    assert!(matches!(
        good.nodes[&id(&fixture, "COUNT")].kind,
        NodeKind::Original(_)
    ));
    let fixture = Fixture::new(
        "pub const fn count() -> usize { 2 }\npub fn source() {\nstruct Bytes { data: [u8; count()] }\nlet selected = ();\n}\n",
    );
    assert!(
        graph(&fixture, &["Bytes"])
            .unwrap_err()
            .to_string()
            .contains("helper calls")
    );
    let fixture = Fixture::new(
        "pub fn source() {\nconst COUNT: usize = 2;\nstruct Bytes { data: [u8; COUNT] }\nlet selected = ();\n}\n",
    );
    assert!(
        graph(&fixture, &["Bytes"])
            .unwrap_err()
            .to_string()
            .contains("function-local non-type")
    );
}

#[test]
fn fields_discriminants_alias_targets_and_where_trait_bounds_are_visited() {
    let fixture = fixture(
        "struct Payload { n: i32 }\nstruct Envelope where Payload: Sized { payload: Payload, formatter: &'static dyn std::fmt::Debug }\nenum Choice { Empty = 1, Other = 2 + 3 }\ntype Pair = (Envelope, Choice);",
    );
    let graph = graph(&fixture, &["Pair"]).unwrap();
    for name in ["Pair", "Payload", "Envelope", "Choice"] {
        assert!(graph.nodes.contains_key(&id(&fixture, name)), "{name}");
    }
    assert!(graph.nodes.keys().any(
        |key| matches!(key, NodeId::External(path) if path.segments == ["core", "marker", "Sized"])
    ));
    assert!(graph.nodes.keys().any(
        |key| matches!(key, NodeId::External(path) if path.segments == ["std", "fmt", "Debug"])
    ));
}

#[test]
fn local_generics_and_captured_lifetimes_fail_while_higher_ranked_lifetimes_are_local() {
    let fixture = fixture("struct Generic<T> { value: T }");
    assert!(
        graph(&fixture, &["Generic"])
            .unwrap_err()
            .to_string()
            .contains("generic local declarations")
    );
    let fixture = Fixture::new(
        "pub fn source<'a>() {\nstruct Borrowed { value: &'a str }\nlet selected = ();\n}\n",
    );
    assert!(
        graph(&fixture, &["Borrowed"])
            .unwrap_err()
            .to_string()
            .contains("lifetime depends on")
    );
    let fixture = Fixture::new(
        "pub fn source() {\ntype Callback = for<'a> fn(&'a str) -> &'a str;\nlet selected = ();\n}\n",
    );
    assert!(graph(&fixture, &["Callback"]).is_ok());
}

#[test]
fn opaque_and_inferred_types_and_non_documentation_attributes_are_rejected() {
    for items in [
        "type A = unknown!();",
        "type A = _;",
        "type A = impl Sized;",
        "#[allow(dead_code)] struct A;",
    ] {
        let fixture = fixture(items);
        let error = graph(&fixture, &["A"]).unwrap_err().to_string();
        assert!(error.contains("dependency chain A"), "{error}");
        assert!(
            error.contains("unsupported") || error.contains("self-contained"),
            "{error}"
        );
    }
}

#[test]
fn unresolved_root_operations_cannot_produce_a_partial_graph() {
    let fixture = Fixture::new("pub fn source(values: &[i32]) {\nlet n = values.len();\n}\n");
    let selected = fixture.source.select(2, 2).unwrap();
    let refs = collect_references(
        &fixture.source,
        &fixture.index,
        &selected,
        &syn::parse_str("fn count(values: &[i32]) -> usize").unwrap(),
        Some(&syn::parse_str("n").unwrap()),
    )
    .unwrap();
    let result = Graph::build(&fixture.resolver(ReuseSite::SameTarget), &refs, &selected);
    assert!(result.unwrap_err().to_string().contains("method `len`"));
}

#[test]
fn reference_collection_feeds_a_complete_deduplicated_graph() {
    let fixture = fixture("struct A { n: u8 }");
    let at = fixture.position("let selected");
    let selected = fixture.source.select(at.line, at.line).unwrap();
    let refs = collect_references(
        &fixture.source,
        &fixture.index,
        &selected,
        &syn::parse_str("fn consume(first: A, second: A)").unwrap(),
        None,
    )
    .unwrap();
    let graph = Graph::build(&fixture.resolver(ReuseSite::SameTarget), &refs, &selected).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.roots.len(), 1);
    assert_eq!(graph.root_references.len(), refs.occurrences.len());
}

#[test]
fn where_constraints_are_dependencies_not_layout_or_alias_expansion_cycles() {
    let fixture = fixture(
        "struct Node where Self: Sized { next: Option<Box<Self>> }\ntype A where B: Sized = u8;\ntype B where A: Sized = u16;",
    );
    let graph = graph(&fixture, &["Node", "A"]).unwrap();
    let a = id(&fixture, "A");
    let b = id(&fixture, "B");
    assert_eq!(
        graph.nodes[&a].edges[&b],
        Edge {
            unguarded_layout: false,
            alias_expansion: false
        }
    );
    let node = id(&fixture, "Node");
    assert!(!graph.nodes[&node].edges[&node].unguarded_layout);
}

#[test]
fn recursive_aliases_inside_trait_object_arguments_still_expand_recursively() {
    let fixture = fixture("type A = dyn std::iter::Iterator<Item = A>;");
    let err = graph(&fixture, &["A"]).unwrap_err().to_string();
    assert!(err.contains("cyclic type alias expansion"), "{err}");
}

#[test]
fn constant_expressions_depending_on_copied_type_values_are_rejected() {
    let fixture = fixture("enum A { One = B::One as isize }\nenum B { One = A::One as isize }");
    let err = graph(&fixture, &["A"]).unwrap_err().to_string();
    assert!(err.contains("copied type values"), "{err}");
}

#[test]
fn shadowed_names_are_deduplicated_by_declaration_id_not_spelling() {
    let fixture = Fixture::new(
        "pub fn source() {\nstruct Data { outer: u32 }\n{\nstruct Data { inner: bool }\nstruct Root { data: Data }\nlet selected = ();\n}\n}\n",
    );
    let graph = graph(&fixture, &["Root"]).unwrap();
    assert_eq!(graph.nodes.len(), 3);
    assert!(!graph.nodes.contains_key(&id(&fixture, "Data"))); // First Data is outside the selected block.
    assert!(graph.nodes.keys().any(
        |id| matches!(id, NodeId::External(path) if path.segments == ["core", "primitive", "bool"])
    ));
}

#[test]
fn compiler_checks_confirm_the_supported_recursive_relationships() {
    for items in [
        "struct Node { next: Option<Box<Node>> }",
        "type Link = Option<Box<Node>>; struct Node { next: Link }",
        "struct Node where Self: Sized { next: Option<Box<Self>> }",
        "struct Node { callback: for<'a> fn(&'a Node) -> &'a Node, marker: std::marker::PhantomData<Self> }",
        "type A where B: Sized = u8; type B where A: Sized = u16; struct Node { a: A }",
    ] {
        let fixture = fixture(items);
        graph(&fixture, &["Node"]).unwrap();
        let output = fixture.compile_original();
        assert!(
            output.status.success(),
            "{items}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(fixture.temp.path().join("src/lib.rs")).unwrap(),
            fixture.source.text
        );
    }
    for items in ["struct Node { bad: Node }", "type Node = Box<Node>;"] {
        let fixture = fixture(items);
        assert!(graph(&fixture, &["Node"]).is_err());
        assert!(!fixture.compile_original().status.success());
    }
}

#[test]
fn long_alias_chains_use_iterative_discovery_and_cycle_validation() {
    let mut items = String::new();
    for index in 0..500 {
        items.push_str(&format!("type N{index} = N{};\n", index + 1));
    }
    items.push_str("type N500 = u8;");
    let fixture = fixture(&items);
    let graph = graph(&fixture, &["N0"]).unwrap();
    assert_eq!(graph.nodes.len(), 502);
    assert_eq!(graph.emission_order.len(), 502);
}
