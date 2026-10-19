use super::*;

fn indexed(text: &str) -> (ParsedSource, ScopeIndex) {
    let source = ParsedSource::parse("scope.rs", text).unwrap();
    let index = ScopeIndex::build(&source).unwrap();
    (source, index)
}

fn position(source: &ParsedSource, marker: &str) -> LineColumn {
    let byte = source.text.find(marker).unwrap() + marker.len();
    let prefix = &source.text[..byte];
    LineColumn {
        line: prefix.bytes().filter(|&byte| byte == b'\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap().chars().count(),
    }
}

fn lookup(
    source: &ParsedSource,
    index: &ScopeIndex,
    marker: &str,
    namespace: Namespace,
    name: &str,
) -> Lookup {
    let at = position(source, marker);
    index.lookup(index.scope_at(at).unwrap(), namespace, name, at)
}

fn found(
    source: &ParsedSource,
    index: &ScopeIndex,
    marker: &str,
    namespace: Namespace,
    name: &str,
) -> DeclarationId {
    match lookup(source, index, marker, namespace, name) {
        Lookup::Found(id) => id,
        other => panic!("{name} at {marker}: {other:?}"),
    }
}

fn value(source: &ParsedSource, index: &ScopeIndex, marker: &str, name: &str) -> DeclarationId {
    found(source, index, marker, Namespace::Value, name)
}

#[test]
fn locals_shadow_only_after_initializer_and_do_not_leak_from_blocks() {
    let (source, index) = indexed(
        r#"
fn f(x: i32) {
    let x = /* init */ x + 1;
    { let x = /* inner_init */ x; /* inner */ x; }
    /* after */ x;
}
"#,
    );
    let param = value(&source, &index, "/* init */", "x");
    let outer = value(&source, &index, "/* inner_init */", "x");
    let inner = value(&source, &index, "/* inner */", "x");
    assert_eq!(index.declarations[param.0].kind, DeclarationKind::Parameter);
    assert_eq!(index.declarations[outer.0].kind, DeclarationKind::Local);
    assert_ne!(inner, outer);
    assert_ne!(param, outer);
    assert_eq!(value(&source, &index, "/* after */", "x"), outer);
}

#[test]
fn let_else_sees_old_binding_and_new_binding_starts_after_else() {
    let (source, index) = indexed(
        r#"
fn f(x: i32, input: Option<i32>) {
    let Some(x) = /* initializer */ input else { /* failure */ x; return; };
    /* success */ x;
}
"#,
    );
    let before = value(&source, &index, "/* initializer */", "x");
    assert_eq!(value(&source, &index, "/* failure */", "x"), before);
    assert_ne!(value(&source, &index, "/* success */", "x"), before);
}

#[test]
fn named_items_are_hoisted_and_namespaces_are_separate() {
    let (source, index) = indexed(
        r#"
fn f() {
    let result = /* before */ make();
    let User = 10;
    let make = 20;
    /* after */ make;
    struct User { field: i32 }
    fn make() -> User { User { field: 1 } }
    type Alias = User;
    enum Choice { A, B }
    const LIMIT: i32 = 3;
    static FLAG: bool = false;
}
"#,
    );
    let before = value(&source, &index, "/* before */", "make");
    assert_eq!(index.declarations[before.0].kind, DeclarationKind::Function);
    assert_eq!(
        index.declarations[value(&source, &index, "/* after */", "make").0].kind,
        DeclarationKind::Local
    );
    let type_id = found(&source, &index, "/* after */", Namespace::Type, "User");
    assert_eq!(index.declarations[type_id.0].kind, DeclarationKind::Struct);
    assert_ne!(type_id, value(&source, &index, "/* after */", "User"));
    for (name, kind, namespace) in [
        ("Alias", DeclarationKind::TypeAlias, Namespace::Type),
        ("Choice", DeclarationKind::Enum, Namespace::Type),
        ("LIMIT", DeclarationKind::Constant, Namespace::Value),
        ("FLAG", DeclarationKind::Static, Namespace::Value),
    ] {
        let id = found(&source, &index, "/* before */", namespace, name);
        assert_eq!(index.declarations[id.0].kind, kind);
    }
}

#[test]
fn struct_patterns_bind_values_not_field_labels() {
    let (source, index) = indexed(
        r#"
fn f(input: User, pair: (i32, i32)) {
    let User { name: alias, active, .. } = input;
    let (left, ref right) = pair;
    let [first, rest @ ..] = values;
    let &(borrowed, _) = reference;
    /* here */ alias;
}
"#,
    );
    for name in [
        "alias", "active", "left", "right", "first", "rest", "borrowed",
    ] {
        assert_eq!(
            index.declarations[value(&source, &index, "/* here */", name).0].kind,
            DeclarationKind::Local
        );
    }
    assert_eq!(
        lookup(&source, &index, "/* here */", Namespace::Value, "name"),
        Lookup::Missing
    );
}

#[test]
fn closures_capture_outer_values_and_shadow_parameters() {
    let (source, index) = indexed(
        r#"
fn f(x: i32, outer: i32) {
    let closure = |x: i32, (a, b): (i32, i32)| { /* body */ x + outer + a + b };
    /* after */ x;
}
"#,
    );
    assert_ne!(
        value(&source, &index, "/* body */", "x"),
        value(&source, &index, "/* after */", "x")
    );
    assert_eq!(
        value(&source, &index, "/* body */", "outer"),
        value(&source, &index, "/* after */", "outer")
    );
    for name in ["a", "b"] {
        value(&source, &index, "/* body */", name);
        assert_eq!(
            lookup(&source, &index, "/* after */", Namespace::Value, name),
            Lookup::Missing
        );
    }
}

#[test]
fn for_loop_binding_is_not_visible_in_iterator_or_after_loop() {
    let (source, index) = indexed(
        r#"
fn f(x: Vec<i32>) {
    for x in /* iterator */ x { /* body */ x; }
    /* after */ x;
    for x in { let x = data; /* inner_iterator */ x } { /* second_body */ x; }
}
"#,
    );
    let outer = value(&source, &index, "/* iterator */", "x");
    assert_eq!(value(&source, &index, "/* after */", "x"), outer);
    let inner = value(&source, &index, "/* body */", "x");
    assert_ne!(inner, outer);
    assert_eq!(index.declarations[inner.0].kind, DeclarationKind::Pattern);
    let iterator = value(&source, &index, "/* inner_iterator */", "x");
    assert_ne!(iterator, outer);
    assert_ne!(value(&source, &index, "/* second_body */", "x"), iterator);
}

#[test]
fn match_bindings_cover_guards_and_body_but_not_other_arms() {
    let (source, index) = indexed(
        r#"
fn f(x: i32, input: Option<i32>) {
    match input {
        Some(x) | Other(x) if /* guard */ x > 0 => /* body */ x,
        _ => /* fallback */ x,
    };
    /* after */ x;
}
"#,
    );
    let arm = value(&source, &index, "/* guard */", "x");
    assert_eq!(value(&source, &index, "/* body */", "x"), arm);
    let outer = value(&source, &index, "/* fallback */", "x");
    assert_eq!(value(&source, &index, "/* after */", "x"), outer);
    assert_ne!(arm, outer);
}

#[test]
fn if_let_chain_bindings_are_sequential_and_excluded_from_else() {
    let (source, index) = indexed(
        r#"
fn f(x: Option<i32>) {
    if let Some(x) = /* initializer */ x && /* chain */ x > 0 {
        /* yes */ x;
    } else { /* no */ x; }
    /* after */ x;
}
"#,
    );
    let outer = value(&source, &index, "/* initializer */", "x");
    let inner = value(&source, &index, "/* chain */", "x");
    assert_ne!(outer, inner);
    assert_eq!(value(&source, &index, "/* yes */", "x"), inner);
    assert_eq!(value(&source, &index, "/* no */", "x"), outer);
    assert_eq!(value(&source, &index, "/* after */", "x"), outer);
}

#[test]
fn while_let_binding_does_not_leak() {
    let (source, index) = indexed(
        r#"
fn f(x: Option<i32>) {
    while let Some(x) = /* initializer */ x { /* body */ x; }
    /* after */ x;
}
"#,
    );
    let outer = value(&source, &index, "/* initializer */", "x");
    assert_eq!(value(&source, &index, "/* after */", "x"), outer);
    assert_ne!(value(&source, &index, "/* body */", "x"), outer);
}

#[test]
fn functions_cannot_capture_runtime_or_generic_bindings_but_can_see_items() {
    let (source, index) = indexed(
        r#"
fn outer<T>(input: i32) {
    struct User { value: i32 }
    fn inner() { /* runtime */ input; let _: /* generic */ T; let _: /* item */ User; }
    let closure = || /* closure */ input;
}
"#,
    );
    assert!(matches!(
        lookup(&source, &index, "/* runtime */", Namespace::Value, "input"),
        Lookup::ForbiddenCapture(_)
    ));
    assert!(matches!(
        lookup(&source, &index, "/* generic */", Namespace::Type, "T"),
        Lookup::ForbiddenCapture(_)
    ));
    found(&source, &index, "/* item */", Namespace::Type, "User");
    value(&source, &index, "/* closure */", "input");
}

#[test]
fn module_boundaries_do_not_inherit_values_or_types_but_macros_are_textual() {
    let (source, index) = indexed(
        r#"
struct User { value: i32 }
fn before() { /* before_macro */ 0; }
macro_rules! helper { () => {} }
mod child { fn f() { let _: /* child_type */ User; /* child_macro */ 0; } }
fn outside() { let _: /* outside */ User; }
"#,
    );
    assert_eq!(
        lookup(&source, &index, "/* child_type */", Namespace::Type, "User"),
        Lookup::Missing
    );
    found(&source, &index, "/* outside */", Namespace::Type, "User");
    let macro_id = found(
        &source,
        &index,
        "/* child_macro */",
        Namespace::Macro,
        "helper",
    );
    assert_eq!(index.declarations[macro_id.0].kind, DeclarationKind::Macro);
    assert_eq!(
        lookup(
            &source,
            &index,
            "/* before_macro */",
            Namespace::Macro,
            "helper"
        ),
        Lookup::Missing
    );
    let module = found(&source, &index, "/* outside */", Namespace::Type, "child");
    assert_eq!(index.declarations[module.0].kind, DeclarationKind::Module);
}

#[test]
fn constants_and_unit_struct_patterns_are_not_new_bindings() {
    let (source, index) = indexed(
        r#"
const LIMIT: i32 = 1;
struct Unit;
struct Pair(i32, i32);
fn f(x: i32) {
    match x { LIMIT => /* constant */ 1, variable => /* variable */ variable };
    let Unit = input;
    /* here */ 0;
}
"#,
    );
    let constant = value(&source, &index, "/* constant */", "LIMIT");
    assert_eq!(
        index.declarations[constant.0].kind,
        DeclarationKind::Constant
    );
    assert_eq!(
        index
            .declarations
            .iter()
            .filter(|decl| decl.name == "LIMIT")
            .count(),
        1
    );
    let unit = value(&source, &index, "/* here */", "Unit");
    assert_eq!(index.declarations[unit.0].kind, DeclarationKind::UnitStruct);
    assert_eq!(
        found(&source, &index, "/* here */", Namespace::Type, "Unit"),
        unit
    );
    let tuple = value(&source, &index, "/* here */", "Pair");
    assert_eq!(
        index.declarations[tuple.0].kind,
        DeclarationKind::TupleStruct
    );
    value(&source, &index, "/* variable */", "variable");
}

#[test]
fn generics_are_indexed_without_type_inference_or_leaking() {
    let (source, index) = indexed(
        r#"
struct Container<T> { value: /* struct_generic */ T }
fn f<T, const N: usize>(input: T) { let _: /* fn_generic */ T; let _: [u8; /* size */ N]; }
fn other() { /* outside */ 0; }
"#,
    );
    let struct_param = found(
        &source,
        &index,
        "/* struct_generic */",
        Namespace::Type,
        "T",
    );
    let fn_param = found(&source, &index, "/* fn_generic */", Namespace::Type, "T");
    assert_ne!(struct_param, fn_param);
    assert_eq!(
        index.declarations[fn_param.0].kind,
        DeclarationKind::TypeParameter
    );
    assert_eq!(
        index.declarations[value(&source, &index, "/* size */", "N").0].kind,
        DeclarationKind::ConstParameter
    );
    assert_eq!(
        lookup(&source, &index, "/* outside */", Namespace::Type, "T"),
        Lookup::Missing
    );
}

#[test]
fn const_context_does_not_capture_runtime_values() {
    let (source, index) = indexed(
        r#"
fn f(value: i32) {
    const C: i32 = /* constant */ value;
    let c = const { /* const_block */ value };
    /* normal */ value;
}
"#,
    );
    for marker in ["/* constant */", "/* const_block */"] {
        assert!(matches!(
            lookup(&source, &index, marker, Namespace::Value, "value"),
            Lookup::ForbiddenCapture(_)
        ));
    }
    value(&source, &index, "/* normal */", "value");
}

#[test]
fn raw_names_unicode_positions_and_ids_are_stable() {
    let text = "fn f() {\n let r#value = 1; let café = 2; /* here */ value + café;\n}\n";
    let (source, index) = indexed(text);
    assert_eq!(index, ScopeIndex::build(&source).unwrap());
    let id = value(&source, &index, "/* here */", "value");
    assert_eq!(value(&source, &index, "/* here */", "r#value"), id);
    let name = value(&source, &index, "/* here */", "café");
    assert_eq!(index.declarations[name.0].range.start.line, 2);
    let selection = source.select(2, 2).unwrap();
    let block = index
        .scope_at(selection.statements[0].span().start())
        .unwrap();
    assert_eq!(index.scopes[block.0].kind, ScopeKind::Block);
    assert_eq!(
        index.scopes[block.0].range,
        SourceRange::of(selection.block)
    );
    assert_eq!(index.declarations[id.0].scope, block);
    assert_eq!(source.text, text);
}

#[test]
fn unsupported_or_inconsistent_patterns_return_errors_not_partial_indexes() {
    for (text, message) in [
        ("fn f() { let (x, x) = value; }", "duplicate binding"),
        (
            "fn f() { match value { Some(x) | Option::None => 1 } }",
            "same names",
        ),
        (
            "fn f() { match value { Some(ref x) | Some(x) => 1 } }",
            "same modes",
        ),
        ("fn f() { let pat!() = value; }", "unsupported pattern"),
        (
            "fn f() { match value { const { 1 } => 0, _ => 1 } }",
            "unsupported pattern",
        ),
        (
            "const C: i32 = 1; fn f() { let mut C = 2; }",
            "conflicts with",
        ),
        (
            "struct Pair(i32); fn f() { let Pair = value; }",
            "conflicts with",
        ),
    ] {
        let source = ParsedSource::parse("unsupported.rs", text).unwrap();
        let error = ScopeIndex::build(&source).unwrap_err().to_string();
        assert!(error.contains(message), "{text}: {error}");
        assert!(error.contains("unsupported.rs:1:"));
    }
}

#[test]
fn unsupported_context_is_an_explicit_error() {
    for (text, message) in [
        ("use other::*; fn f() {}", "glob imports"),
        ("fn f() { make_bindings!(); }", "macro statements"),
        ("fn f() { let x = value!(); }", "macro expressions"),
        (
            "#[cfg(feature = \"x\")] fn f() {}",
            "conditional compilation",
        ),
        ("mod external;", "out-of-line"),
        ("#[add_items] fn f() {}", "unsupported attribute"),
        ("#[derive(Debug)] struct S;", "unsupported attribute"),
        ("trait T {}", "unsupported item"),
        ("struct S; struct S;", "duplicate item"),
    ] {
        let source = ParsedSource::parse("unsupported.rs", text).unwrap();
        let error = ScopeIndex::build(&source).unwrap_err().to_string();
        assert!(error.contains(message), "{text}: {error}");
    }
}

#[test]
fn inline_const_can_use_generic_parameters_but_named_constants_cannot() {
    let (source, index) = indexed(
        r#"
fn f<T, const N: usize>() {
    let n = const { /* inline_value */ N };
    let t = const { let _: /* inline_type */ T; 0 };
    const C: usize = /* named */ N;
}
"#,
    );
    value(&source, &index, "/* inline_value */", "N");
    found(&source, &index, "/* inline_type */", Namespace::Type, "T");
    assert!(matches!(
        lookup(&source, &index, "/* named */", Namespace::Value, "N"),
        Lookup::ForbiddenCapture(_)
    ));
}

#[test]
fn duplicate_parameters_and_unknown_pattern_constructors_are_rejected() {
    for (text, message) in [
        ("fn f(x: i32, x: i32) {}", "duplicate parameter"),
        ("fn f() { let c = |x, x| x; }", "duplicate parameter"),
        (
            "fn f() { match x { None => 1, _ => 0 } }",
            "unresolved prelude constructor",
        ),
    ] {
        let source = ParsedSource::parse("unsupported.rs", text).unwrap();
        assert!(
            ScopeIndex::build(&source)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
    let (source, index) = indexed("fn f() { match x { Option::None => /* here */ 1, _ => 0 } }");
    assert_eq!(
        lookup(&source, &index, "/* here */", Namespace::Value, "None"),
        Lookup::Missing
    );
}

#[test]
fn repeated_let_chain_names_shadow_in_order_and_do_not_escape_to_else_if() {
    let (source, index) = indexed(
        r#"
fn f(x: Option<i32>) {
    if let Some(x) = x && let Some(x) = /* second_initializer */ wrap(x) {
        /* first_then */ x;
    } else if let Some(x) = /* else_if_initializer */ x {
        /* second_then */ x;
    }
    /* after */ x;
}
"#,
    );
    let outer = value(&source, &index, "/* after */", "x");
    let first = value(&source, &index, "/* second_initializer */", "x");
    let second = value(&source, &index, "/* first_then */", "x");
    assert_ne!(first, outer);
    assert_ne!(first, second);
    assert_eq!(
        value(&source, &index, "/* else_if_initializer */", "x"),
        outer
    );
    let alternative = value(&source, &index, "/* second_then */", "x");
    assert_ne!(alternative, outer);
    assert_ne!(alternative, second);
}

#[test]
fn declaration_visibility_on_same_line_is_position_sensitive() {
    let (source, index) =
        indexed("fn f(x: i32) { let x = /* first */ x; let x = /* second */ x; /* third */ x; }");
    let first = value(&source, &index, "/* first */", "x");
    let second = value(&source, &index, "/* second */", "x");
    let third = value(&source, &index, "/* third */", "x");
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(index.declarations[first.0].kind, DeclarationKind::Parameter);
}

#[test]
fn impl_receivers_and_parameters_have_body_only_identity_and_capture_barriers() {
    let (source, index) = indexed(
        r#"
fn outer<T>(outer: i32) {
    struct S;
    impl /* header */ S {
        fn method(&self, x: /* signature */ i32) {
            /* body */ self;
            let closure = || { /* closure */ self; x; };
            fn nested() { /* nested */ self; x; }
        }
        fn other(self) { /* other */ self; }
        fn associated() { /* associated */ 0; }
    }
    /* outside */ outer;
}
"#,
    );
    let receiver = value(&source, &index, "/* body */", "self");
    let param = value(&source, &index, "/* body */", "x");
    for (name, id) in [("self", receiver), ("x", param)] {
        assert_eq!(index.declarations[id.0].kind, DeclarationKind::Parameter);
        assert_eq!(value(&source, &index, "/* closure */", name), id);
        assert_eq!(
            lookup(&source, &index, "/* nested */", Namespace::Value, name),
            Lookup::ForbiddenCapture(id)
        );
        for marker in ["/* signature */", "/* outside */", "/* associated */"] {
            assert_eq!(
                lookup(&source, &index, marker, Namespace::Value, name),
                Lookup::Missing
            );
        }
    }
    assert_ne!(receiver, value(&source, &index, "/* other */", "self"));
    let method_scope = index.declarations[receiver.0].scope;
    assert_eq!(index.scopes[method_scope.0].kind, ScopeKind::Function);
    assert_eq!(
        index.scopes[index.scopes[method_scope.0].parent.unwrap().0].kind,
        ScopeKind::Item
    );
    let outer = value(&source, &index, "/* outside */", "outer");
    for marker in ["/* header */", "/* body */"] {
        assert_eq!(
            lookup(&source, &index, marker, Namespace::Value, "outer"),
            Lookup::ForbiddenCapture(outer)
        );
        assert!(matches!(
            lookup(&source, &index, marker, Namespace::Type, "T"),
            Lookup::ForbiddenCapture(_)
        ));
    }
    assert!(
        !index
            .declarations
            .iter()
            .any(|decl| ["method", "other", "associated"].contains(&decl.name.as_str()))
    );
    assert_eq!(
        lookup(&source, &index, "/* body */", Namespace::Value, "method"),
        Lookup::Missing
    );
}

#[test]
fn impls_resolve_shadowed_local_type_names_by_identity() {
    let (source, index) = indexed(
        r#"
fn f() {
    struct S;
    fn method() {}
    impl S { fn method(&mut self) { /* outer */ 0; } }
    {
        struct S;
        impl S { fn method(self: Box<Self>) { /* inner */ 0; } }
    }
    /* after */ 0;
}
"#,
    );
    let outer = found(&source, &index, "/* outer */", Namespace::Type, "S");
    let inner = found(&source, &index, "/* inner */", Namespace::Type, "S");
    assert_ne!(outer, inner);
    assert_eq!(
        found(&source, &index, "/* after */", Namespace::Type, "S"),
        outer
    );
    let lexical = value(&source, &index, "/* after */", "method");
    assert_eq!(value(&source, &index, "/* inner */", "method"), lexical);
    assert_eq!(value(&source, &index, "/* outer */", "method"), lexical);
}

#[test]
fn unsupported_impls_methods_attributes_and_macros_are_rejected() {
    for implementation in [
        "impl Trait for S {}",
        "unsafe impl S {}",
        "default impl S {}",
        "impl<T> S {}",
        "impl S where S: Trait {}",
        "impl S<u8> {}",
        "impl &S {}",
        "impl (S,) {}",
        "impl <S as Trait>::Assoc {}",
        "impl S { const C: i32 = 1; }",
        "impl S { type A = S; }",
        "impl S { items!(); }",
        "impl S { fn f<T>() {} }",
        "impl S { fn f() where S: Trait {} }",
        "impl S { async fn f() {} }",
        "impl S { unsafe fn f() {} }",
        "impl S { const fn f() {} }",
        "impl S { extern \"C\" fn f() {} }",
        "impl S { default fn f() {} }",
        "#[cfg(test)] impl S {}",
        "#[generate] impl S {}",
        "impl S { #[cfg(test)] fn f() {} }",
        "impl S { #[generate] fn f() {} }",
        "impl S { fn f(#[generate] &self) {} }",
        "impl S { fn f(#[generate] x: i32) {} }",
        "impl S { fn f() { bindings!(); } }",
        "impl S { fn f() { let x = expression!(); } }",
    ] {
        let text = format!("struct S; {implementation}");
        let source = ParsedSource::parse("unsupported.rs", &text).unwrap();
        assert!(ScopeIndex::build(&source).is_err(), "{implementation}");
    }
    indexed("struct S; #[doc = \"impl\"] impl S { #[allow(dead_code)] fn f(&self) {} }");
    indexed("mod m { struct S; } impl crate::m::S { fn f() {} }");
}

use syn::spanned::Spanned;
