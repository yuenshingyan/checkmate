//! Native public automatic-workflow value and identity regressions.
mod support;
use support::Consumer;

// Select a unique, complete statement, never a marker or a statement fragment.
fn extract(source: &str, statement: &str, signature: &str, output: &str) -> String {
    let lines: Vec<_> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == statement)
        .collect();
    assert_eq!(lines.len(), 1, "selection must be unique: {statement}");
    let line = lines[0].0 + 1;
    format!(
        "checkmate::extract_fn! {{ snapshot = false, fn_sig = {signature:?}, module = \"src/lib.rs\", start = {line}, end = {line}, output = {output}, context = \"auto\" }}\n"
    )
}

#[test]
fn original_enums_aliases_and_reexports_keep_concrete_identity() {
    let c = Consumer::new();
    let source = r#"mod model {
    pub enum Choice { Empty, Number(u32), Pair { left: u32, right: u32 } }
    pub type Alias = Choice;
}
pub use model::{Choice as PublicChoice, Alias as PublicAlias};
use PublicChoice as Imported;
pub fn original(n: u32) -> Imported {
let value = if n == 0 { Imported::Empty } else if n == 1 { Imported::Number(n) } else { Imported::Pair { left: n, right: n + 1 } };
value
}
pub fn score(value: PublicChoice) -> u32 {
match value { PublicChoice::Empty => 0, PublicChoice::Number(n) => n, PublicChoice::Pair { left, right } => left + right }
}
pub fn aliased(n: u32) -> PublicAlias {
type First = PublicAlias;
type Second = First;
let aliased: Second = Imported::Number(n);
aliased
}
pub mod alpha { pub struct Packet { pub value: u32 } }
pub mod beta { pub struct Packet { pub value: (u32, u32) } }
use alpha::Packet;
pub fn scoped(n: u32) -> beta::Packet {
use beta::Packet;
let scoped = Packet { value: (n, n + 1) };
scoped
}
"#;
    c.write("src/lib.rs", source);
    let calls = format!(
        "{}{}{}",
        extract(
            source,
            "let scoped = Packet { value: (n, n + 1) };",
            "fn scoped(n: u32) -> Packet",
            "scoped"
        ),
        extract(
            source,
            "let value = if n == 0 { Imported::Empty } else if n == 1 { Imported::Number(n) } else { Imported::Pair { left: n, right: n + 1 } };",
            "fn build(n: u32) -> Imported",
            "value"
        ),
        extract(
            source,
            "let aliased: Second = Imported::Number(n);",
            "fn alias(n: u32) -> Second",
            "aliased"
        )
    );
    c.write("tests/probe.rs", &format!(r#"
struct Imported;
struct Packet;
struct First;
struct Second;
{calls}
#[test]
fn variants_and_interoperability() {{
    for n in [0, 1, 8, 100] {{
        let nested: regression_consumer::beta::Packet = scoped(n);
        assert_eq!(nested.value, (n, n + 1));
        assert_eq!(nested.value, regression_consumer::scoped(n).value);
        let value: regression_consumer::PublicChoice = build(n);
        assert_eq!(regression_consumer::score(value), regression_consumer::score(regression_consumer::original(n)));
        match (n, build(n)) {{
            (0, regression_consumer::PublicChoice::Empty) => (),
            (1, regression_consumer::PublicChoice::Number(value)) => assert_eq!(value, 1),
            (_, regression_consumer::PublicChoice::Pair {{ left, right }}) => assert_eq!((left, right), (n, n + 1)),
            _ => panic!("wrong variant"),
        }}
        let value: regression_consumer::PublicAlias = alias(n);
        assert_eq!(std::any::Any::type_id(&value), std::any::TypeId::of::<regression_consumer::PublicChoice>());
        assert_eq!(regression_consumer::score(value), n);
    }}
}}
"#));
    c.test(&[]);
}

#[test]
fn copied_enum_aliases_diamond_and_recursive_dependencies() {
    let c = Consumer::new();
    let source = r#"pub fn original(n: u32) -> (std::any::TypeId, std::any::TypeId) {
struct Leaf { value: u32 }
struct Left { leaf: Leaf }
enum Right { Item(Leaf), Empty }
impl Right { fn score(self) -> u32 { match self { Self::Item(leaf) => leaf.value, Self::Empty => 99 } } }
struct Diamond { left: Left, right: Right }
struct Node { value: u32, next: Option<Box<Node>> }
type First = Diamond;
type Second = First;
let diamond = Second { left: Left { leaf: Leaf { value: n } }, right: if n == 0 { Right::Empty } else { Right::Item(Leaf { value: n + 1 }) } };
let node = Node { value: n, next: None };
(std::any::TypeId::of::<Diamond>(), std::any::TypeId::of::<Right>())
}
"#;
    c.write("src/lib.rs", source);
    let statement = "let diamond = Second { left: Left { leaf: Leaf { value: n } }, right: if n == 0 { Right::Empty } else { Right::Item(Leaf { value: n + 1 }) } };";
    // Output matches are in the original lexical context too; callers need not
    // know or guess the generated nominal names to observe enum variants.
    let output = "{ let same: Diamond = diamond; let alias: First = same; let diamond: Second = alias; let _shared: &Leaf = match &diamond.right { Right::Item(leaf) => leaf, Right::Empty => &diamond.left.leaf }; let observed = match &diamond.right { Right::Item(leaf) => leaf.value, Right::Empty => 99 }; (diamond, observed) }";
    let calls = format!(
        "{}{}{}{}",
        extract(
            source,
            statement,
            "fn variant(n: u32) -> Right",
            "diamond.right"
        ),
        extract(
            source,
            statement,
            "fn first(n: u32) -> (Second, u32)",
            output
        ),
        extract(
            source,
            statement,
            "fn second(n: u32) -> (First, u32)",
            output
        ),
        extract(
            source,
            "let node = Node { value: n, next: None };",
            "fn recursive(n: u32) -> Node",
            "node"
        )
    );
    c.write("tests/probe.rs", &format!(r#"
struct Leaf;
struct Diamond;
struct Node;
struct Second;
{calls}
#[test]
fn values_and_nominals() {{
    for n in [0, 7, 99] {{
        let (a, observed) = first(n);
        let (b, other) = second(n);
        let expected = if n == 0 {{ 99 }} else {{ n + 1 }};
        assert_eq!((a.left.leaf.value, b.left.leaf.value, observed, other), (n, n, expected, expected));
        assert_ne!(std::any::Any::type_id(&a), std::any::Any::type_id(&b));
        assert_ne!(std::any::Any::type_id(&a), regression_consumer::original(n).0);
        let right = variant(n);
        assert_ne!(std::any::Any::type_id(&right), regression_consumer::original(n).1);
        assert_eq!(right.score(), expected);
        let mut node = recursive(n);
        assert_eq!(node.value, n);
        assert!(node.next.is_none());
        // Allocation is caller code: associated Box::new discovery is outside
        // the supported subset, but the copied recursive edge must be usable.
        node.next = Some(Box::new(recursive(n + 1)));
        let next = node.next.unwrap();
        assert_eq!(next.value, n + 1);
        assert!(next.next.is_none());
    }}
}}
"#));
    c.test(&[]);

    // Exercise the direct-nominal signature too. Typed moves in each output
    // verify alias transparency within that build; TypeIds are not compared
    // across builds (their stability is not part of the contract).
    let caller = std::fs::read_to_string(c.path("tests/probe.rs")).unwrap();
    c.write(
        "tests/probe.rs",
        &caller.replace(
            "fn first(n: u32) -> (Second, u32)",
            "fn first(n: u32) -> (Diamond, u32)",
        ),
    );
    c.test(&[]);
}

#[test]
fn nested_shadowing_shorthand_and_explicit_inputs_skip_initializers() {
    let c = Consumer::new();
    let source = r#"use std::sync::atomic::{AtomicUsize, Ordering};
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn initialize() -> u32 { CALLS.fetch_add(1, Ordering::SeqCst); 91 }
pub fn calls() -> usize { CALLS.load(Ordering::SeqCst) }
pub struct Value { pub value: u32 }
pub fn original() -> u32 {
let value = initialize();
struct Value { value: u32 }
{
struct Value { value: (u32, u32) }
type Alias = Value;
let result = { let value = (value, value + 1); Alias { value } };
result.value.0 + result.value.1
}
}
"#;
    c.write("src/lib.rs", source);
    let calls = extract(
        source,
        "let result = { let value = (value, value + 1); Alias { value } };",
        "fn supplied(value: u32) -> Alias",
        "result",
    );
    c.write(
        "tests/probe.rs",
        &format!(
            r#"
struct Value {{ wrong: bool }}
type Alias = bool;
{calls}
#[test]
fn explicit_values_not_original_initializers() {{
    assert_eq!(regression_consumer::calls(), 0);
    for value in [0, 12, 500] {{
        let result = supplied(value);
        assert_eq!(result.value, (value, value + 1));
        assert_eq!(regression_consumer::calls(), 0);
    }}
    assert_eq!(regression_consumer::original(), 183);
    assert_eq!(regression_consumer::calls(), 1);
    assert_eq!(supplied(4).value, (4, 5));
    assert_eq!(regression_consumer::calls(), 1);
}}
"#
        ),
    );
    c.test(&[]);
}
