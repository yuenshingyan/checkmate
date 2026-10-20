use super::*;
use crate::scope::ScopeIndex;
use crate::source::ast::ParsedSource;

#[test]
fn uncalled_tuple_constructors_never_prove_nominal_receivers() {
    for (declaration, constructor, body) in [
        ("struct User(u8);", "User", "self.0 + 10"),
        (
            "enum User { Value(u8) }",
            "User::Value",
            "match self { Self::Value(n) => n + 10 }",
        ),
    ] {
        let source = format!(
            "pub fn source() {{\n{declaration}\nimpl User {{ fn clone(self) -> u8 {{ {body} }} }}\nlet user = {constructor};\nlet value = user.clone();\n}}\n"
        );
        let f = Fixture::new(&source);
        assert!(f.compile_original().status.success());
        let err = plan(
            &f,
            "let value",
            "let value",
            "fn extract(user: User) -> u8",
            Some("value"),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(err.contains("original method target"), "{err}");
        assert!(plan(&f, "let user", "let value", "fn extract()", None).is_err());
        let source = format!(
            "pub fn source() {{\n{declaration}\nimpl User {{ fn clone(self) -> u8 {{ {body} }} fn unused() {{ let factory = {constructor}; let cloned = factory.clone(); }} }}\nlet user = {constructor}(1);\n}}\n"
        );
        let f = Fixture::new(&source);
        assert!(f.compile_original().status.success());
        assert!(
            plan(
                &f,
                "let user =",
                "let user =",
                "fn extract() -> User",
                Some("user")
            )
            .is_err()
        );
    }
}

#[test]
fn actual_unit_tuple_and_named_construction_prove_owned_receivers() {
    for (declaration, expression, body) in [
        ("struct User;", "User", "3"),
        ("struct User(u8);", "User(3)", "self.0"),
        ("struct User(u8);", "User { 0: 3 }", "self.0"),
        (
            "enum User { Value(u8) }",
            "User::Value(3)",
            "match self { Self::Value(n) => n }",
        ),
        ("enum User { Value }", "User::Value", "3"),
    ] {
        let source = format!(
            "pub fn source() -> u8 {{\n{declaration}\nimpl User {{ fn take(self) -> u8 {{ {body} }} }}\nlet user = {expression};\nlet value = user.take();\nvalue\n}}\n"
        );
        let f = Fixture::new(&source);
        let p = plan(
            &f,
            "let user",
            "let value",
            "fn build() -> u8",
            Some("value"),
        )
        .unwrap();
        success(&f, &[&p], "assert_eq!(build(), 3);");
    }
}

#[test]
fn copied_method_struct_patterns_preserve_owner_identity_and_field_bindings() {
    for (pattern, result) in [
        ("Self { n }", "n"),
        ("User { n: renamed }", "renamed"),
        ("Alias { n }", "n"),
    ] {
        let source = format!(
            "pub fn source(n: u8) -> u8 {{\nstruct User {{ n: u8 }}\ntype Alias = User;\nimpl User {{ fn take(self) -> u8 {{ let {pattern} = self; {result} }} }}\nlet user = User {{ n }};\nlet value = user.take();\nvalue\n}}\n"
        );
        let f = Fixture::new(&source);
        let p = plan(
            &f,
            "let user",
            "let value",
            "fn build(n: u8) -> u8",
            Some("value"),
        )
        .unwrap();
        assert_eq!(p.copied_nominal_types.len(), 1);
        success(
            &f,
            &[&p],
            "for n in [0, 7, 255] { assert_eq!(build(n), n); assert_eq!(build(n), source_model::source(n)); }",
        );
    }
}

#[test]
fn shared_example_source_compiles_into_a_working_generated_function() {
    let f = Fixture::new(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/inherent-methods/src/lib.rs"
    )));
    let p = plan(
        &f,
        "let counter",
        "let counter",
        "fn finish_counter(n: u8) -> u8",
        Some("counter.finish()"),
    )
    .unwrap();
    success(
        &f,
        &[&p],
        "for n in [0, 7, 254] { assert_eq!(finish_counter(n), source_model::original(n)); }",
    );
}

#[test]
fn generic_type_and_const_arguments_in_method_signatures_are_preserved() {
    let f = Fixture::new(
        "pub const WIDTH: usize = 2;\npub struct Wrapper<T, const N: usize> { pub inner: T, pub bytes: [u8; N] }\npub fn source(n: u8) -> u8 {\nstruct User { n: u8 }\nimpl User { fn wrap(self: Self) -> Wrapper<Self, WIDTH> { Wrapper { inner: self, bytes: [0; WIDTH] } } }\nlet user = User { n };\nlet wrapped = User::wrap(user);\nwrapped.inner.n\n}\n",
    );
    let p = plan(
        &f,
        "let user",
        "let wrapped",
        "fn build(n: u8) -> Wrapper<User, WIDTH>",
        Some("wrapped"),
    )
    .unwrap();
    success(
        &f,
        &[&p],
        "let result = build(4); assert_eq!(result.inner.n, 4); assert_eq!(result.bytes, [0, 0]);",
    );
}

#[test]
fn constructors_borrowed_methods_and_owned_methods_compile_and_run() {
    let f = Fixture::new(
        "pub fn source(n: u8) -> u8 {\nstruct User { n: u8 }\nimpl User {\nfn new(n: u8) -> Self { Self { n } }\nfn read(&self) -> u8 { self.n }\nfn into_value(self) -> u8 { Self::read(&self) }\n}\nlet user = User::new(n);\nlet value = user.into_value();\nvalue\n}\n",
    );
    let p = plan(
        &f,
        "let user",
        "let value",
        "fn build(n: u8) -> u8",
        Some("value"),
    )
    .unwrap();
    assert_eq!(
        p.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Impl(_)))
            .count(),
        1
    );
    success(
        &f,
        &[&p],
        "assert_eq!(build(7), 7); assert_eq!(build(7), source_model::source(7));",
    );
}

#[test]
fn self_and_transitive_method_dependencies_expand_without_layout_cycles() {
    let f = Fixture::new(
        "pub const OFFSET: u8 = 2;\npub fn plus(n: u8) -> u8 { n + OFFSET }\npub fn source(n: u8) -> u8 {\nstruct First { n: u8 }\nstruct Second { n: u8 }\ntype Alias = First;\nimpl First { fn next(self) -> Second { Second { n: plus(self.n) } } }\nimpl Second { fn back(self) -> Alias { First { n: self.n + OFFSET } } }\nlet first = First { n };\nlet second = first.next();\nlet result = Second::back(second);\nresult.n\n}\n",
    );
    let p = plan(
        &f,
        "let first",
        "let result",
        "fn build(n: u8) -> First",
        Some("result"),
    )
    .unwrap();
    assert_eq!(p.copied_nominal_types.len(), 2);
    assert_eq!(
        p.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Impl(_)))
            .count(),
        2
    );
    success(
        &f,
        &[&p],
        "assert_eq!(build(5).n, 9); assert_eq!(build(5).n, source_model::source(5));",
    );
}

#[test]
fn multiple_impl_blocks_raw_names_and_mutable_borrowed_ufcs_are_preserved() {
    let f = Fixture::new(
        "pub fn source(n: u8) -> u8 {\nstruct User { n: u8 }\nimpl User { fn new(n: u8) -> Self { Self { n } } }\nimpl User { fn r#type(&mut self, n: u8) { self.n = n; } fn take(self) -> u8 { self.n } }\nlet mut user = User::new(n);\nUser::r#type(&mut user, n + 1);\nlet result = user.take();\nresult\n}\n",
    );
    let p = plan(
        &f,
        "let mut user",
        "let result",
        "fn build(n: u8) -> u8",
        Some("result"),
    )
    .unwrap();
    success(&f, &[&p], "assert_eq!(build(4), 5);");
}

#[test]
fn shadowed_nominal_owners_and_separate_extractions_do_not_share_methods() {
    let f = Fixture::new(
        "pub fn source(n: u8) {\nstruct User { n: u8 }\nimpl User { fn take(self) -> u8 { self.n + 1 } }\nlet outer = User { n };\n{\nstruct User { n: u8 }\nimpl User { fn take(self) -> u8 { self.n + 2 } }\nlet inner = User { n };\n}\n}\n",
    );
    let a = plan(
        &f,
        "let outer",
        "let outer",
        "fn first(n: u8) -> u8",
        Some("outer.take()"),
    )
    .unwrap();
    let b = plan(
        &f,
        "let inner",
        "let inner",
        "fn second(n: u8) -> u8",
        Some("inner.take()"),
    )
    .unwrap();
    success(
        &f,
        &[&a, &b],
        "assert_eq!(first(3), 4); assert_eq!(second(3), 5);",
    );
}

#[test]
fn local_enum_variants_and_self_pattern_references_are_rewritten() {
    let f = Fixture::new(
        "pub fn source(n: u8) -> u8 {\nenum Choice { Value(u8), Empty }\nimpl Choice { fn new(n: u8) -> Self { Self::Value(n) } fn take(self) -> u8 { match self { Self::Value(n) => n, Self::Empty => 0 } } }\nlet choice = Choice::new(n);\nlet value = choice.take();\nvalue\n}\n",
    );
    let p = plan(
        &f,
        "let choice",
        "let value",
        "fn build(n: u8) -> u8",
        Some("value"),
    )
    .unwrap();
    success(&f, &[&p], "assert_eq!(build(8), 8);");
}

#[test]
fn direct_constructor_receivers_and_owned_self_dot_calls_are_proven() {
    let f = Fixture::new(
        "pub fn source(n: u8) -> u8 {\nstruct User { n: u8 }\nimpl User { fn new(n: u8) -> Self { Self { n } } fn take(self) -> u8 { self.n } fn forward(self) -> u8 { self.take() } }\nlet value = User::new(n).forward();\nvalue\n}\n",
    );
    let mut p = plan(
        &f,
        "let value",
        "let value",
        "fn build(n: u8) -> u8",
        Some("value"),
    )
    .unwrap();
    // A caller's blanket trait must not steal either proven by-value dispatch.
    p.items.push(syn::parse_quote!(
        trait Steal {
            fn forward(self) -> u8;
            fn take(self) -> u8;
        }
    ));
    p.items.push(syn::parse_quote!(
        impl<T> Steal for T {
            fn forward(self) -> u8 {
                99
            }
            fn take(self) -> u8 {
                99
            }
        }
    ));
    success(&f, &[&p], "assert_eq!(build(6), 6);");
}

#[test]
fn typed_receivers_and_input_identity_are_checked_in_both_contexts() {
    let f = Fixture::new(
        "pub fn source(n: u8) -> u8 {\nstruct User { n: u8 }\nimpl User { fn take(self) -> u8 { self.n } }\nlet user: User = User { n };\nlet value = user.take();\nvalue\n}\n",
    );
    let p = plan(
        &f,
        "let value",
        "let value",
        "fn extract(user: User) -> u8",
        Some("value"),
    )
    .unwrap();
    let owner = p.copied_nominal_types.iter().next().unwrap();
    let ty = &p.names[&NodeId::Source(*owner)];
    success(
        &f,
        &[&p],
        &format!("assert_eq!(extract({ty} {{ n: 8 }}), 8);"),
    );
}

#[test]
fn supplied_signature_must_not_retarget_original_method_dispatch() {
    let f = Fixture::new(
        "pub fn source() {\nstruct First;\nstruct Second;\nimpl First { fn take(self) -> u8 { 1 } }\nimpl Second { fn take(self) -> u8 { 2 } }\nlet value: First = First;\nlet result = value.take();\n}\n",
    );
    let err = plan(
        &f,
        "let result",
        "let result",
        "fn extract(value: Second) -> u8",
        Some("result"),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(err.contains("original method target"), "{err}");
}

#[test]
fn autoref_deref_and_unknown_receivers_are_not_guessed() {
    for (receiver, method) in [
        ("user", "fn read(&self) -> u8 { self.n }"),
        ("user", "fn read(&mut self) -> u8 { self.n }"),
        ("(&user)", "fn read(self) -> u8 { self.n }"),
    ] {
        let source = format!(
            "pub fn source(n: u8) {{\nstruct User {{ n: u8 }}\nimpl User {{ {method} }}\nlet mut user = User {{ n }};\nlet value = {receiver}.read();\n}}\n"
        );
        let f = Fixture::new(&source);
        let err = plan(
            &f,
            "let mut user",
            "let value",
            "fn build(n: u8) -> u8",
            Some("value"),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(err.contains("method/trait resolution"), "{err}");
    }
}

#[test]
fn all_methods_are_checked_even_when_only_a_literal_is_selected() {
    for body in [
        "external::helper()",
        "String::new()",
        "self.n.count_ones()",
        "hidden()",
    ] {
        let source = format!(
            "fn hidden() -> u8 {{ 1 }}\npub fn source(n: u8) {{\nstruct User {{ n: u8 }}\nimpl User {{ fn unsupported(self) -> u8 {{ {body} }} }}\nlet user = User {{ n }};\n}}\n"
        );
        let f = Fixture::new(&source);
        assert!(
            plan(
                &f,
                "let user",
                "let user",
                "fn build(n: u8) -> User",
                Some("user")
            )
            .is_err(),
            "{body}"
        );
    }
}

#[test]
fn method_captures_nested_items_and_semantic_attributes_fail() {
    for method in [
        "fn bad(self) -> u8 { n }",
        "fn bad(self) -> u8 { fn inner() -> u8 { 1 } inner() }",
        "#[allow(dead_code)] fn bad(self) -> u8 { self.n }",
        "#[doc = include_str!(\"missing\")] fn bad(self) -> u8 { self.n }",
    ] {
        let source = format!(
            "pub fn source(n: u8) {{\nstruct User {{ n: u8 }}\nimpl User {{ {method} }}\nlet user = User {{ n }};\n}}\n"
        );
        let f = Fixture::new(&source);
        assert!(
            plan(
                &f,
                "let user",
                "let user",
                "fn build(n: u8) -> User",
                Some("user")
            )
            .is_err(),
            "{method}"
        );
    }
}

#[test]
fn trait_derive_generic_and_macro_context_fail_before_promotion() {
    for implementation in [
        "impl Clone for User { fn clone(&self) -> Self { Self { n: self.n } } }",
        "impl User { fn generic<T>(&self, value: T) {} }",
        "impl User { fn macro_body(self) { panic!(); } }",
        "impl User { const VALUE: u8 = 1; }",
        "impl User { type Other = u8; }",
    ] {
        let source = ParsedSource::parse(
            "model.rs",
            format!("fn source() {{ struct User {{ n: u8 }} {implementation} }}"),
        )
        .unwrap();
        assert!(ScopeIndex::build(&source).is_err(), "{implementation}");
    }
    let source =
        ParsedSource::parse("model.rs", "fn source() { #[derive(Clone)] struct User; }").unwrap();
    assert!(ScopeIndex::build(&source).is_err());
}

#[test]
fn original_types_retain_implementation_and_private_dependencies_without_copying() {
    let f = Fixture::new(
        "pub struct User { pub n: u8 }\nfn hidden(n: u8) -> u8 { n + 1 }\nimpl User { pub fn new(n: u8) -> Self { Self { n: hidden(n) } } }\npub fn source(n: u8) {\nlet user = User::new(n);\n}\n",
    );
    let p = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    assert!(
        !p.items
            .iter()
            .any(|item| matches!(item, syn::Item::Impl(_)))
    );
    success(
        &f,
        &[&p],
        "let user: source_model::User = build(3); assert_eq!(user.n, 4);",
    );
}

#[test]
fn original_private_methods_remain_private() {
    let f = Fixture::new(
        "pub struct User { pub n: u8 }\nimpl User { fn secret(n: u8) -> Self { Self { n } } }\npub fn source(n: u8) {\nlet user = User::secret(n);\n}\n",
    );
    let p = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    let output = compile(&f, &[&p], "let _ = build(3);", true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private"));
}

#[test]
fn selected_implementations_fail_instead_of_duplicating_or_dropping_behavior() {
    let f = Fixture::new(
        "pub fn source() {\nstruct User;\nimpl User { fn take(self) -> u8 { 1 } }\nlet user = User;\n}\n",
    );
    let err = plan(
        &f,
        "impl User",
        "let user",
        "fn build() -> User",
        Some("user"),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(err.contains("separate lifting"), "{err}");
}

#[test]
fn implementations_and_types_disappear_without_cfg_test() {
    let f = Fixture::new(
        "pub fn source(n: u8) {\nstruct User { n: u8 }\nimpl User { fn new(n: u8) -> Self { Self { n } } }\nlet user = User::new(n);\n}\n",
    );
    let p = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    assert!(compile(&f, &[&p], "", false).status.success());
    assert!(
        !compile(&f, &[&p], "let _ = build(3);", false)
            .status
            .success()
    );
}
