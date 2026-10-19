use crate::diagnostics::Result;
mod implementations;

use std::{collections::BTreeSet, fs, process::Command};

use super::*;
use crate::resolve::collect_references;
use crate::resolve::declarations::{ReuseSite, tests::Fixture};

fn plan(
    f: &Fixture,
    start: &str,
    end: &str,
    signature: &str,
    output: Option<&str>,
) -> Result<PromotionPlan> {
    plan_reserved(f, start, end, signature, output, &BTreeSet::new())
}

fn plan_reserved(
    f: &Fixture,
    start: &str,
    end: &str,
    signature: &str,
    output: Option<&str>,
    reserved: &BTreeSet<String>,
) -> Result<PromotionPlan> {
    let selection = f
        .source
        .select(f.position(start).line, f.position(end).line)?;
    let signature = syn::parse_str(signature)?;
    let output = output.map(syn::parse_str).transpose()?;
    let references =
        collect_references(&f.source, &f.index, &selection, &signature, output.as_ref())?;
    PromotionPlan::build(
        &f.resolver(ReuseSite::PackageIntegrationTest),
        &references,
        &selection,
        reserved,
    )
}

fn compile(f: &Fixture, plans: &[&PromotionPlan], body: &str, test: bool) -> std::process::Output {
    let original = f.compile_original();
    assert!(
        original.status.success(),
        "{}",
        String::from_utf8_lossy(&original.stderr)
    );
    let code = plans
        .iter()
        .map(|plan| plan.tokens().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let file = f.temp.path().join("promoted.rs");
    fs::write(&file, format!("{code}\nfn main() {{ {body} }}")).unwrap();
    let executable = f
        .temp
        .path()
        .join(format!("promoted{}", std::env::consts::EXE_SUFFIX));
    let mut rustc = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    rustc
        .arg("--edition=2024")
        .arg(&file)
        .arg("--extern")
        .arg(format!(
            "source_model={}",
            f.temp.path().join("libsource_model.rlib").display()
        ))
        .arg("-o")
        .arg(&executable);
    if test {
        rustc.args(["--cfg", "test"]);
    }
    let output = rustc.output().unwrap();
    if output.status.success() {
        let run = Command::new(&executable).output().unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    assert_eq!(
        fs::read_to_string(f.temp.path().join("src/lib.rs")).unwrap(),
        f.source.text
    );
    output
}

fn success(f: &Fixture, plans: &[&PromotionPlan], body: &str) {
    let output = compile(f, plans, body, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn caller_can_inspect_returned_local_struct_and_identity_change_is_explicit() {
    let f = Fixture::new(
        "pub fn original(name: String) -> impl std::any::Any {\nstruct User { name: String }\nlet user = User { name };\nuser\n}\n",
    );
    let plan = plan(
        &f,
        "let user",
        "let user",
        "fn build(name: String) -> User",
        Some("user"),
    )
    .unwrap();
    assert_eq!(plan.copied_nominal_types.len(), 1);
    success(
        &f,
        &[&plan],
        "let copy = build(String::from(\"Bob\")); assert_eq!(copy.name, \"Bob\"); let original = source_model::original(String::from(\"Bob\")); assert_ne!(std::any::Any::type_id(&copy), std::any::Any::type_id(&original));",
    );
}

#[test]
fn separate_extractions_of_the_same_local_type_do_not_collide() {
    let f =
        Fixture::new("pub fn source(n: u8) {\nstruct User { n: u8 }\nlet user = User { n };\n}\n");
    let first = plan(
        &f,
        "let user",
        "let user",
        "fn first(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    let second = plan(
        &f,
        "let user",
        "let user",
        "fn second(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    assert!(
        first
            .names
            .values()
            .all(|name| !second.names.values().any(|other| other == name))
    );
    success(
        &f,
        &[&first, &second],
        "assert_eq!(first(1).n, 1); assert_eq!(second(2).n, 2);",
    );
}

#[test]
fn nested_types_aliases_and_generic_arguments_are_rewritten_by_identity() {
    let f = Fixture::new(
        "pub const N: usize = 3;\npub struct Wrapper<T, const M: usize> { pub value: T, pub bytes: [u8; M] }\npub fn source(name: String) {\nstruct User { name: String }\ntype Account = User;\nlet wrapped = Wrapper::<Account, N> { value: Account { name }, bytes: [0; N] };\n}\n",
    );
    let plan = plan(
        &f,
        "let wrapped",
        "let wrapped",
        "fn build(name: String) -> Wrapper<Account, N>",
        Some("wrapped"),
    )
    .unwrap();
    assert_eq!(plan.copied_nominal_types.len(), 1); // Alias is transparent, not a second nominal type.
    success(
        &f,
        &[&plan],
        "fn original_wrapper<T>(_: source_model::Wrapper<T, 3>) {} let w = build(String::from(\"Bob\")); assert_eq!(w.value.name, \"Bob\"); assert_eq!(w.bytes, [0; 3]); original_wrapper(w);",
    );
}

#[test]
fn reused_original_structs_remain_original_in_a_generated_function() {
    let f = Fixture::new(
        "pub struct User { pub n: u8 }\npub fn source(n: u8) {\nlet user = User { n };\n}\n",
    );
    let plan = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> User",
        Some("user"),
    )
    .unwrap();
    assert!(plan.copied_nominal_types.is_empty());
    success(
        &f,
        &[&plan],
        "let user: source_model::User = build(7); assert_eq!(user.n, 7);",
    );
}

#[test]
fn a_copied_transparent_alias_can_still_refer_to_an_original_type() {
    let f = Fixture::new(
        "pub struct User { pub n: u8 }\npub fn source(n: u8) {\ntype Account = User;\nlet user = Account { n };\n}\n",
    );
    let plan = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> Account",
        Some("user"),
    )
    .unwrap();
    assert!(plan.copied_nominal_types.is_empty());
    success(
        &f,
        &[&plan],
        "let user: source_model::User = build(7); assert_eq!(user.n, 7);",
    );
}

#[test]
fn recursive_local_types_and_self_references_emit_once() {
    let f = Fixture::new(
        "pub fn source() {\nstruct Node { next: Option<Box<Self>> }\nlet node = Node { next: None };\n}\n",
    );
    let plan = plan(
        &f,
        "let node",
        "let node",
        "fn build() -> Node",
        Some("node"),
    )
    .unwrap();
    assert_eq!(
        plan.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Struct(_)))
            .count(),
        1
    );
    success(&f, &[&plan], "assert!(build().next.is_none());");
}

#[test]
fn selected_type_declarations_are_lifted_only_in_the_generated_copy() {
    let f = Fixture::new(
        "pub fn source() {\nstruct Node { next: Option<Box<Self>> }\nlet node = Node { next: None };\n}\n",
    );
    let plan = plan(
        &f,
        "struct Node",
        "let node",
        "fn build() -> Node",
        Some("node"),
    )
    .unwrap();
    let syn::Item::Fn(function) = plan.items.last().unwrap() else {
        panic!()
    };
    assert!(
        function
            .block
            .stmts
            .iter()
            .all(|stmt| !matches!(stmt, syn::Stmt::Item(syn::Item::Struct(_))))
    );
    success(&f, &[&plan], "assert!(build().next.is_none());");
}

#[test]
fn nested_shadowed_types_and_signature_output_scopes_stay_distinct() {
    let f = Fixture::new(
        "pub fn source() {\nstruct User { outer: u8 }\n{\nstruct User { inner: bool }\nlet inner = User { inner: true };\nlet _ = inner;\n}\n}\n",
    );
    // The selected statement is the entire nested block, not its inner scope.
    let selection = f.source.select(3, 7).unwrap();
    let references = collect_references(
        &f.source,
        &f.index,
        &selection,
        &syn::parse_str("fn build() -> User").unwrap(),
        Some(&syn::parse_str("User { outer: 9 }").unwrap()),
    )
    .unwrap();
    let plan = PromotionPlan::build(
        &f.resolver(ReuseSite::PackageIntegrationTest),
        &references,
        &selection,
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(plan.copied_nominal_types.len(), 2);
    success(&f, &[&plan], "assert_eq!(build().outer, 9);");
}

#[test]
fn labels_literals_and_unrelated_value_bindings_are_not_renamed() {
    let f = Fixture::new(
        "pub fn source() {\nstruct User { User: u8 }\nlet User = 7;\nlet user = User { User };\nlet label = \"User\";\n}\n",
    );
    let plan = plan(
        &f,
        "let User",
        "let label",
        "fn build() -> (User, u8, &'static str)",
        Some("(user, User, label)"),
    )
    .unwrap();
    let text = plan.tokens().to_string();
    assert!(text.contains("let User = 7"));
    assert!(text.contains("\"User\""));
    success(
        &f,
        &[&plan],
        "let (user, value, label) = build(); assert_eq!(user.User, 7); assert_eq!(value, 7); assert_eq!(label, \"User\");",
    );
}

#[test]
fn shorthand_constant_fields_and_patterns_keep_their_original_labels() {
    let f = Fixture::new(
        "pub const VALUE: u8 = 4;\npub fn source() {\nstruct Record { VALUE: u8 }\nlet record = Record { VALUE };\nlet matched = match record { Record { VALUE } => true, _ => false };\n}\n",
    );
    let plan = plan(
        &f,
        "let record",
        "let matched",
        "fn build() -> bool",
        Some("matched"),
    )
    .unwrap();
    success(&f, &[&plan], "assert!(build());");
    let text = plan.tokens().to_string();
    assert!(text.contains("VALUE : Checkmate"), "{text}");
}

#[test]
fn enums_tuple_structs_and_unit_patterns_use_the_promoted_identity() {
    let f = Fixture::new(
        "pub fn source(n: u8) {\nstruct Unit;\nstruct Pair(u8, u8);\nenum Choice { Empty, Value(Pair), Named { n: u8 } }\nlet unit = Unit;\nlet Unit = unit;\nlet choice = Choice::Value(Pair(n, 2));\nlet result = match choice { Choice::Value(Pair(a, b)) => a + b, Choice::Named { n } => n, Choice::Empty => 0 };\n}\n",
    );
    let plan = plan(
        &f,
        "let unit",
        "let result",
        "fn build(n: u8) -> u8",
        Some("result"),
    )
    .unwrap();
    assert_eq!(plan.copied_nominal_types.len(), 3);
    success(&f, &[&plan], "assert_eq!(build(4), 6);");
}

#[test]
fn generated_items_are_absent_without_cfg_test() {
    let f = Fixture::new("pub fn source() {\nstruct User;\nlet user = User;\n}\n");
    let plan = plan(
        &f,
        "let user",
        "let user",
        "fn build() -> User",
        Some("user"),
    )
    .unwrap();
    let output = compile(&f, &[&plan], "", false);
    assert!(output.status.success());
    let output = compile(&f, &[&plan], "let _ = build();", false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot find function"));
}

#[test]
fn names_are_stable_and_known_collisions_fail_instead_of_shadowing() {
    let source = "pub fn source() {\nstruct User;\nlet user = User;\n}\n";
    let f = Fixture::new(source);
    let other = Fixture::new(source);
    let a = plan(
        &f,
        "let user",
        "let user",
        "fn r#type() -> User",
        Some("user"),
    )
    .unwrap();
    let b = plan(
        &other,
        "let user",
        "let user",
        "fn r#type() -> User",
        Some("user"),
    )
    .unwrap();
    assert_eq!(a.tokens().to_string(), b.tokens().to_string());
    let name = a.names.values().next().unwrap().to_string();
    let error = plan_reserved(
        &f,
        "let user",
        "let user",
        "fn r#type() -> User",
        Some("user"),
        &BTreeSet::from([name]),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("generated name") && error.contains("conflicts"));
    assert!(
        plan_reserved(
            &f,
            "let user",
            "let user",
            "fn r#type() -> User",
            Some("user"),
            &BTreeSet::from(["type".into()])
        )
        .is_err()
    );
    success(&f, &[&a], "let _ = r#type();");
}

#[test]
fn literal_documentation_is_preserved_but_doc_macros_and_visibility_relocation_fail() {
    let f = Fixture::new(
        "pub fn source() {\n/// Original documentation for User.
struct User { n: u8 }\nlet user = User { n: 1 };\n}\n",
    );
    let plan = plan(
        &f,
        "let user",
        "let user",
        "fn build() -> User",
        Some("user"),
    )
    .unwrap();
    assert!(
        plan.tokens()
            .to_string()
            .contains("Original documentation for User.")
    );
    success(&f, &[&plan], "assert_eq!(build().n, 1);");
    for item in [
        "#[doc = include_str!(\"note.txt\")] struct User { n: u8 }",
        "struct User { pub(crate) n: u8 }",
    ] {
        let f = Fixture::new(&format!(
            "pub fn source() {{\n{item}\nlet user = User {{ n: 1 }};\n}}\n"
        ));
        let error = self::plan(
            &f,
            "let user",
            "let user",
            "fn build() -> User",
            Some("user"),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains("doc macros") || error.contains("restricted visibility"),
            "{error}"
        );
    }
}

#[test]
fn unsafe_dependencies_and_original_local_helper_interoperation_are_rejected() {
    let f = Fixture::new(
        "pub fn source(n: u8) {\nstruct User { n: u8 }\nfn accept(user: User) -> User { user }\nlet user = accept(User { n });\n}\n",
    );
    let error = plan(
        &f,
        "let user",
        "let user",
        "fn build(n: u8) -> User",
        Some("user"),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("function-local non-type"), "{error}");
    let f = Fixture::new("pub fn source<T>() {\nstruct User { data: T }\nlet user = ();\n}\n");
    assert!(plan(&f, "let user", "let user", "fn build() -> User", None).is_err());
}

#[test]
fn private_original_fields_are_not_made_public_by_promotion() {
    let f = Fixture::new(
        "pub struct Original { n: u8 }\npub fn source() {\nlet original = Original { n: 1 };\n}\n",
    );
    let plan = plan(
        &f,
        "let original",
        "let original",
        "fn build() -> Original",
        Some("original"),
    )
    .unwrap();
    let result = compile(&f, &[&plan], "let _ = build();", true);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("private"));
}

#[test]
fn output_construction_and_shadowing_keep_distinct_bindings() {
    let f = Fixture::new("pub fn source(n: u8) {\nstruct User { n: u8 }\nlet seen = n;\n}\n");
    let plan = plan(
        &f,
        "let seen",
        "let seen",
        "fn build(n: u8) -> User",
        Some("{ let n = 9; User { n } }"),
    )
    .unwrap();
    success(&f, &[&plan], "assert_eq!(build(1).n, 9);");
}

#[test]
fn original_enum_variant_generics_survive_both_path_and_import_alias_forms() {
    for constructor in ["Choice::<User>::Value", "Build::<User>"] {
        let f = Fixture::new(&format!(
            "pub enum Choice<T> {{ Value(T), Empty }}\nuse crate::Choice::Value as Build;\npub fn source(n: u8) {{\nstruct User {{ n: u8 }}\nlet choice = {constructor}(User {{ n }});\n}}\n"
        ));
        let plan = plan(
            &f,
            "let choice",
            "let choice",
            "fn build(n: u8) -> Choice<User>",
            Some("choice"),
        )
        .unwrap();
        success(
            &f,
            &[&plan],
            "match build(7) { source_model::Choice::Value(user) => assert_eq!(user.n, 7), _ => panic!() }",
        );
    }
}

#[test]
fn unicode_and_raw_names_do_not_require_textual_replacement() {
    let f = Fixture::new(
        "pub fn source(名: u8) {\nstruct r#type { 名: u8 }\nlet 用户 = r#type { 名 };\n}\n",
    );
    let plan = plan(
        &f,
        "let 用户",
        "let 用户",
        "fn 构建(名: u8) -> r#type",
        Some("用户"),
    )
    .unwrap();
    success(&f, &[&plan], "assert_eq!(构建(5).名, 5);");
}

#[test]
fn signature_parameters_cannot_capture_generated_constant_imports() {
    let f = Fixture::new(
        "pub const VALUE: u8 = 4;\npub fn source() {\nstruct User { n: u8 }\nlet user = User { n: VALUE };\n}\n",
    );
    let first = plan(
        &f,
        "let user",
        "let user",
        "fn build() -> User",
        Some("user"),
    )
    .unwrap();
    let id = DeclarationId(
        f.index
            .declarations
            .iter()
            .position(|item| item.name == "VALUE")
            .unwrap(),
    );
    let name = &first.names[&NodeId::Source(id)];
    let signature = format!("fn build({name}: u8) -> User");
    let error = plan(&f, "let user", "let user", &signature, Some("user"))
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("conflicts"), "{error}");
}

#[test]
fn selected_and_output_use_declarations_are_not_moved_with_wrong_crate_roots() {
    let f = Fixture::new(
        "pub struct User;\npub fn source() {\nuse crate::User as Account;\nlet user = Account;\n}\n",
    );
    let error = plan(
        &f,
        "use crate",
        "let user",
        "fn build() -> User",
        Some("user"),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("import-relocation"), "{error}");
    let error = plan(
        &f,
        "let user",
        "let user",
        "fn build() -> User",
        Some("{ use crate::User as Unused; user }"),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("import-relocation"), "{error}");
}

#[test]
fn generic_self_is_rejected_before_any_bare_name_rewrite() {
    for parameter in ["T", "T = u8"] {
        let f = Fixture::new(&format!(
            "pub fn source() {{\nstruct Node<{parameter}> {{ value: T, next: Option<Box<Self>> }}\nlet node = Node {{ value: 1u8, next: None }};\n}}\n"
        ));
        let error = plan(
            &f,
            "let node",
            "let node",
            "fn build() -> Node<u8>",
            Some("node"),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains("generic local declarations"), "{error}");
    }
    let f = Fixture::new(
        "pub struct Holder<T = u8> { value: T, bytes: [u8; {\nlet _owner: Option<Self> = None;\n0\n}] }\n",
    );
    let error = plan(&f, "let _owner", "let _owner", "fn inspect()", None)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("implicit generic Self"), "{error}");
}

#[test]
fn unmatched_rewrite_locations_never_produce_a_partial_plan() {
    let names = BTreeMap::new();
    let mut rewriter = rewrite::Rewriter::new(&names);
    rewriter
        .add(
            SourceRange::of(&syn::parse_str::<syn::Path>("Missing").unwrap()),
            super::super::declarations::Resolved::Binding,
        )
        .unwrap();
    assert!(
        rewriter
            .finish()
            .unwrap_err()
            .to_string()
            .contains("not every resolved reference")
    );
}
