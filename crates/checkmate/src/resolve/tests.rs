use super::*;
use crate::diagnostics::Result;
use crate::scope::DeclarationKind;

struct Analysis {
    source: ParsedSource,
    index: ScopeIndex,
    references: References,
}

fn source(prelude: &str, parameters: &str, before: &str, body: &str) -> String {
    format!("{prelude}\nfn original({parameters}) {{\n{before}\n// start\n{body}\n// end\n}}\n")
}

fn analyze(text: &str, signature: &str, output: Option<&str>) -> Result<Analysis> {
    let source = ParsedSource::parse("src/example.rs", text)?;
    let index = ScopeIndex::build(&source)?;
    let start = text.lines().position(|line| line == "// start").unwrap() + 2;
    let end = text.lines().position(|line| line == "// end").unwrap();
    let selection = source.select(start, end)?;
    let signature = syn::parse_str(signature)?;
    let output = output.map(syn::parse_str::<Expr>).transpose()?;
    let references = collect_references(&source, &index, &selection, &signature, output.as_ref())?;
    Ok(Analysis {
        source,
        index,
        references,
    })
}

fn roots<'a>(analysis: &'a Analysis, name: &str) -> Vec<&'a Reference> {
    analysis
        .references
        .occurrences
        .iter()
        .filter(|reference| reference.root == name)
        .collect()
}

fn inputs(analysis: &Analysis) -> Vec<&str> {
    analysis
        .references
        .required_inputs
        .iter()
        .map(String::as_str)
        .collect()
}

fn error(text: &str, signature: &str, output: Option<&str>) -> String {
    analyze(text, signature, output)
        .err()
        .expect("expected collection error")
        .to_string()
}

#[test]
fn struct_construction_collects_type_and_shorthand_input_not_field_labels() {
    let text = source(
        "struct User { name: String, active: bool }",
        "",
        "let name = side_effect();",
        "let user = User { name, active: true };",
    );
    let result = analyze(&text, "fn build_user(name: String) -> User", Some("user")).unwrap();
    assert_eq!(inputs(&result), ["name"]);
    assert_eq!(roots(&result, "User").len(), 2); // signature and construction
    assert!(
        roots(&result, "User")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::Declaration(_)))
    );
    assert!(roots(&result, "active").is_empty());
    assert!(roots(&result, "side_effect").is_empty());
    assert!(!result.references.generated.text.contains("side_effect"));
    assert_eq!(result.source.text, text);
    let user = roots(&result, "user")[0];
    let ReferenceTarget::SnippetLocal(id) = user.target else {
        panic!()
    };
    assert_eq!(result.index.declarations[id.0].kind, DeclarationKind::Local);
    assert_eq!(user.site.origin, ReferenceOrigin::Output);
}

#[test]
fn source_local_shadowing_overrides_parameters_in_body_and_output() {
    let text = source(
        "",
        "x: i32",
        "",
        "let y = x;\n{ let x = 2; let z = x; }\nlet x = y;",
    );
    let result = analyze(&text, "fn extracted(x: i32) -> i32", Some("x")).unwrap();
    assert_eq!(inputs(&result), ["x"]);
    assert_eq!(
        roots(&result, "x")
            .iter()
            .filter(|reference| matches!(reference.target, ReferenceTarget::Input(_)))
            .count(),
        1
    );
    assert!(
        roots(&result, "x")
            .iter()
            .filter(|reference| reference.site.origin == ReferenceOrigin::Output)
            .all(|reference| matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
    let text = source("", "", "", "let x = 1; let y = x + 2;");
    assert!(inputs(&analyze(&text, "fn extracted() -> i32", Some("y")).unwrap()).is_empty());
}

#[test]
fn missing_outer_input_reports_original_source_location_without_copying_initializer() {
    let text = source("", "", "let name = side_effect();", "let result = name;");
    let error = error(&text, "fn extracted() -> String", Some("result"));
    assert!(error.contains("src/example.rs:5-5"), "{error}");
    assert!(error.contains("source 5:"));
    assert!(error.contains("required input `name`"));
    assert!(error.contains("typed parameter"));
}

#[test]
fn signature_and_output_only_declarations_are_included() {
    let text = source(
        "struct SignatureOnly; struct OutputOnly; const EXTRA: i32 = 4;",
        "",
        "",
        "let x = 1;",
    );
    let result = analyze(
        &text,
        "fn extracted(unused: SignatureOnly) -> (i32, OutputOnly)",
        Some("(x + EXTRA, OutputOnly)"),
    )
    .unwrap();
    assert!(inputs(&result).is_empty());
    let signature = roots(&result, "SignatureOnly")[0];
    assert_eq!(signature.site.origin, ReferenceOrigin::Signature);
    assert!(matches!(signature.target, ReferenceTarget::Declaration(_)));
    let extra = roots(&result, "EXTRA")[0];
    assert_eq!(extra.site.origin, ReferenceOrigin::Output);
    assert!(matches!(extra.target, ReferenceTarget::Declaration(_)));
}

#[test]
fn output_only_inputs_can_be_original_or_explicitly_new_parameters() {
    let text = source("", "expected: i32", "", "let x = 1;");
    let result = analyze(
        &text,
        "fn extracted(expected: i32, bonus: i32) -> i32",
        Some("x + expected + bonus"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["bonus", "expected"]);
    let error = error(&text, "fn extracted() -> i32", Some("x + expected"));
    assert!(error.contains("output expression"));
    assert!(error.contains("required input `expected`"));
}

#[test]
fn output_local_bindings_and_closures_do_not_become_inputs() {
    let text = source("", "", "", "let x = 1;");
    let result = analyze(
        &text,
        "fn extracted() -> i32",
        Some("{ let x = 9; (|| x)() }"),
    )
    .unwrap();
    assert!(inputs(&result).is_empty());
    let ReferenceTarget::GeneratedLocal(id) = roots(&result, "x")[0].target else {
        panic!()
    };
    assert_eq!(
        result.references.generated_index.declarations[id.0].name,
        "x"
    );
    assert_eq!(roots(&result, "x")[0].site.origin, ReferenceOrigin::Output);
}

#[test]
fn supplied_input_patterns_are_compared_by_bound_names() {
    let text = source(
        "",
        "left: i32, right: i32",
        "",
        "let result = left + right;",
    );
    let result = analyze(
        &text,
        "fn extracted((left, right): (i32, i32)) -> i32",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["left", "right"]);
    let text = source(
        "struct User { name: String }",
        "alias: String",
        "",
        "let result = alias;",
    );
    let result = analyze(
        &text,
        "fn extracted(User { name: alias }: User) -> String",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["alias"]);
    assert!(roots(&result, "name").is_empty());
}

#[test]
fn closure_and_conditional_bindings_obey_original_scopes() {
    let text = source(
        "",
        "x: i32",
        "",
        "let closure = |x: i32| x + 1;\nlet result = closure(0) + x;",
    );
    let result = analyze(&text, "fn extracted(x: i32) -> i32", Some("result")).unwrap();
    assert_eq!(inputs(&result), ["x"]);
    assert!(
        roots(&result, "x")
            .iter()
            .any(|reference| matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
    let text = source(
        "",
        "input: Option<i32>, fallback: i32",
        "",
        "let result = if let Some(x) = input { x } else { fallback };",
    );
    let result = analyze(
        &text,
        "fn extracted(input: Option<i32>, fallback: i32) -> i32",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["fallback", "input"]);
    assert!(
        roots(&result, "x")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
}

#[test]
fn loop_and_match_pattern_bindings_are_not_outer_inputs() {
    let text = source(
        "",
        "values: Vec<i32>",
        "",
        "let mut total = 0;\nfor value in values { total += value; }",
    );
    let result = analyze(
        &text,
        "fn extracted(values: Vec<i32>) -> i32",
        Some("total"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["values"]);
    let text = source(
        "",
        "input: Option<i32>, fallback: i32",
        "",
        "let result = match input { Some(value) if value > 0 => value, _ => fallback };",
    );
    let result = analyze(
        &text,
        "fn extracted(input: Option<i32>, fallback: i32) -> i32",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["fallback", "input"]);
    assert!(
        roots(&result, "value")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
}

#[test]
fn original_constant_patterns_are_dependencies_not_synthetic_bindings() {
    let text = source(
        "const LIMIT: i32 = 1;",
        "input: i32",
        "",
        "let result = match input { LIMIT => LIMIT, other => other };",
    );
    let result = analyze(&text, "fn extracted(input: i32) -> i32", Some("result")).unwrap();
    assert_eq!(inputs(&result), ["input"]);
    assert_eq!(roots(&result, "LIMIT").len(), 2);
    assert!(
        roots(&result, "LIMIT")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::Declaration(_)))
    );
}

#[test]
fn signature_parameters_must_not_silently_replace_original_items_or_imports() {
    let text = source("const FACTOR: i32 = 2;", "", "", "let x = FACTOR;");
    assert!(
        error(&text, "fn extracted(FACTOR: i32) -> i32", Some("x"))
            .contains("shadow original declaration `FACTOR`")
    );
    let text = source("use crate::model::Unit;", "", "", "let x = Unit;");
    assert!(
        error(&text, "fn extracted(Unit: i32) -> i32", Some("x"))
            .contains("shadow original import `Unit`")
    );
    let text = source("", "", "", "let x = original();");
    assert!(
        error(&text, "fn original() -> i32", Some("x")).contains("shadow original declaration")
    );
}

#[test]
fn nested_item_capture_errors_are_not_fixed_by_new_function_parameters() {
    let text = source(
        "",
        "x: i32",
        "",
        "fn inner() -> i32 { x }\nlet result = inner();",
    );
    assert!(
        error(&text, "fn extracted(x: i32) -> i32", Some("result"))
            .contains("original item/const capture boundary")
    );
    let text = source("", "", "", "let x = 1;");
    assert!(
        error(
            &text,
            "fn extracted() -> i32",
            Some("{ fn inner() -> i32 { x } inner() }")
        )
        .contains("capture an enclosing generated binding")
    );
    let text = source("", "x: i32", "", "let y = 1;");
    assert!(
        error(
            &text,
            "fn extracted() -> i32",
            Some("{ fn inner() -> i32 { x } inner() }")
        )
        .contains("output item/const boundary")
    );
}

#[test]
fn explicit_import_references_keep_original_or_generated_index_ownership() {
    let text = source(
        "use crate::model::User;",
        "name: String",
        "",
        "let user = User { name };",
    );
    let result = analyze(&text, "fn extracted(name: String) -> User", Some("user")).unwrap();
    for reference in roots(&result, "User") {
        let ReferenceTarget::Import(id) = reference.target else {
            panic!()
        };
        assert_eq!(result.index.imports[id.0].alias, "User");
    }
    let text = source(
        "struct User { name: String }",
        "name: String",
        "",
        "use crate::User as Alias;\nlet user = Alias { name };",
    );
    let result = analyze(&text, "fn extracted(name: String) -> User", Some("user")).unwrap();
    assert!(matches!(
        roots(&result, "Alias")[0].target,
        ReferenceTarget::Import(_)
    ));
    let text = source("", "", "", "let x = 1;");
    let result = analyze(
        &text,
        "fn extracted() -> i32",
        Some("{ use crate::VALUE as Alias; x + Alias }"),
    )
    .unwrap();
    let ReferenceTarget::GeneratedImport(id) = roots(&result, "Alias")[0].target else {
        panic!()
    };
    assert_eq!(
        result.references.generated_index.imports[id.0].alias,
        "Alias"
    );
}

#[test]
fn method_and_field_labels_are_not_free_variables() {
    let text = source(
        "struct Pair { field: usize }",
        "values: Vec<i32>, pair: Pair",
        "",
        "let length = values.len();\nlet result = pair.field + length;",
    );
    let result = analyze(
        &text,
        "fn extracted(values: Vec<i32>, pair: Pair) -> usize",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["pair", "values"]);
    assert!(roots(&result, "len").is_empty());
    assert!(roots(&result, "field").is_empty());
    assert_eq!(result.references.deferred_operations.len(), 1);
    assert!(
        result.references.deferred_operations[0]
            .description
            .contains("method `len`")
    );
}

#[test]
fn bare_generic_arguments_are_explicitly_deferred_not_guessed_as_types() {
    let text = source(
        "struct Holder<const N: usize>; const N: usize = 2;",
        "value: Holder<N>",
        "",
        "let result: Holder<N> = value;",
    );
    let result = analyze(
        &text,
        "fn extracted(value: Holder<N>) -> Holder<N>",
        Some("result"),
    )
    .unwrap();
    assert_eq!(inputs(&result), ["value"]);
    assert!(!roots(&result, "N").is_empty());
    assert!(
        roots(&result, "N")
            .iter()
            .all(|reference| reference.usage == ReferenceUse::TypeOrConst
                && reference.target == ReferenceTarget::Deferred)
    );
    let text = source(
        "const LENGTH: usize = 2;",
        "value: [u8; LENGTH]",
        "",
        "let result = value;",
    );
    let result = analyze(
        &text,
        "fn extracted(value: [u8; LENGTH]) -> [u8; LENGTH]",
        Some("result"),
    )
    .unwrap();
    assert!(
        roots(&result, "LENGTH")
            .iter()
            .all(|reference| reference.usage == ReferenceUse::Value
                && matches!(reference.target, ReferenceTarget::Declaration(_)))
    );
}

#[test]
fn unknown_and_explicitly_rooted_paths_stay_deferred() {
    let text = source(
        "",
        "external: i32",
        "",
        "let result = ::external::run() + crate::VALUE + unknown();",
    );
    let result = analyze(&text, "fn extracted(external: i32) -> i32", Some("result")).unwrap();
    assert!(inputs(&result).is_empty());
    for name in ["external", "crate", "unknown"] {
        assert!(
            roots(&result, name)
                .iter()
                .all(|reference| reference.target == ReferenceTarget::Deferred)
        );
    }
    assert!(roots(&result, "external")[0].path.starts_with("::"));
}

#[test]
fn qualified_trait_paths_are_recorded_as_pending_operations() {
    let text = source(
        "struct User; use external::Trait;",
        "",
        "",
        "let value = <User as Trait>::VALUE;",
    );
    let result = analyze(&text, "fn extracted() -> i32", Some("value")).unwrap();
    assert!(
        roots(&result, "User")
            .iter()
            .any(|reference| matches!(reference.target, ReferenceTarget::Declaration(_)))
    );
    assert!(
        roots(&result, "Trait")
            .iter()
            .any(|reference| matches!(reference.target, ReferenceTarget::Import(_)))
    );
    assert!(
        result
            .references
            .deferred_operations
            .iter()
            .any(|operation| operation.description.contains("trait/type resolution"))
    );
}

#[test]
fn output_pattern_context_and_selected_modules_fail_without_guessing() {
    let text = source("const LIMIT: i32 = 1;", "", "", "let x = 1;");
    assert!(
        error(
            &text,
            "fn extracted() -> i32",
            Some("{ let LIMIT = 2; LIMIT }")
        )
        .contains("output pattern `LIMIT`")
    );
    assert!(
        analyze(
            &text,
            "fn extracted() -> i32",
            Some("match x { crate::LIMIT => 1, _ => 0 }")
        )
        .is_ok()
    );
    let text = source("", "", "", "mod local { pub struct User; }");
    assert!(
        error(&text, "fn extracted()", None).contains("module declarations inside selected code")
    );
}

#[test]
fn local_type_in_signature_is_identified_for_later_promotion() {
    let text = source(
        "",
        "",
        "",
        "struct User { active: bool }\nlet user = User { active: true };",
    );
    let result = analyze(&text, "fn extracted() -> User", Some("user")).unwrap();
    let user = roots(&result, "User")
        .into_iter()
        .find(|reference| reference.site.origin == ReferenceOrigin::Signature)
        .unwrap();
    let ReferenceTarget::SnippetLocal(id) = user.target else {
        panic!()
    };
    assert_eq!(
        result.index.declarations[id.0].kind,
        DeclarationKind::Struct
    );
}

#[test]
fn original_unicode_crlf_locations_and_text_are_preserved() {
    let text = source("", "café: i32", "", "let result = café;").replace('\n', "\r\n");
    let result = analyze(&text, "fn extracted(café: i32) -> i32", Some("result")).unwrap();
    let reference = roots(&result, "café")[0];
    assert_eq!(reference.site.origin, ReferenceOrigin::Source);
    assert_eq!(reference.site.range.start.line, 5);
    assert_eq!(reference.site.range.start.column, 13);
    assert_eq!(result.source.text, text);
}

#[test]
fn constants_declared_in_snippet_or_output_can_be_used_as_patterns() {
    let text = source("", "", "", "const LIMIT: i32 = 1;\nlet x = 1;");
    let result = analyze(
        &text,
        "fn extracted() -> i32",
        Some("match x { LIMIT => 1, _ => 0 }"),
    )
    .unwrap();
    assert!(
        roots(&result, "LIMIT")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
    let text = source("", "", "", "let x = 1;");
    let result = analyze(
        &text,
        "fn extracted() -> i32",
        Some("{ const C: i32 = 1; match x { C => 1, _ => 0 } }"),
    )
    .unwrap();
    assert!(matches!(
        roots(&result, "C")[0].target,
        ReferenceTarget::GeneratedLocal(_)
    ));
    let text = source("const C: i32 = 1;", "", "", "let C = 1;");
    let result = analyze(&text, "fn extracted() -> i32", Some("C")).unwrap();
    assert!(
        roots(&result, "C")
            .iter()
            .all(|reference| matches!(reference.target, ReferenceTarget::Declaration(_)))
    );
}

#[test]
fn opaque_type_macros_are_not_silently_ignored() {
    let text = source("", "", "", "let value: type_macro!() = 1;");
    assert!(error(&text, "fn extracted() -> i32", Some("value")).contains("opaque macro tokens"));
    let text = source("", "", "", "let value = 1;");
    assert!(
        error(&text, "fn extracted() -> type_macro!()", Some("value"))
            .contains("opaque macro tokens")
    );
}

#[test]
fn implicit_tail_output_and_qualified_associated_names_remain_explicit() {
    let text = source("", "x: i32", "", "x + 1");
    let result = analyze(&text, "fn extracted(x: i32) -> i32", None).unwrap();
    assert_eq!(inputs(&result), ["x"]);
    assert!(
        result
            .references
            .occurrences
            .iter()
            .all(|reference| reference.site.origin != ReferenceOrigin::Output)
    );
    let text = source("struct User;", "", "", "let value = User::new();");
    let result = analyze(&text, "fn extracted() -> User", Some("value")).unwrap();
    assert!(roots(&result, "new").is_empty());
    assert!(
        result
            .references
            .deferred_operations
            .iter()
            .any(|operation| operation.description.contains("final-target resolution"))
    );
}

#[test]
fn constant_or_patterns_keep_original_classification_in_incomplete_shell() {
    let text = source(
        "const C: i32 = 1; const D: i32 = 2; const None: i32 = 3;",
        "input: i32",
        "",
        "let result = match input { C | D | None => 1, other => other };",
    );
    let result = analyze(&text, "fn extracted(input: i32) -> i32", Some("result")).unwrap();
    assert_eq!(inputs(&result), ["input"]);
    for name in ["C", "D", "None"] {
        assert!(matches!(
            roots(&result, name)[0].target,
            ReferenceTarget::Declaration(_)
        ));
        assert!(
            !result
                .references
                .generated_index
                .declarations
                .iter()
                .any(|decl| decl.name == name)
        );
    }
}

#[test]
fn proven_constant_patterns_do_not_allow_parameter_substitution() {
    let text = source(
        "const C: i32 = 1;",
        "input: i32",
        "",
        "let result = match input { C => 1, _ => 0 };",
    );
    assert!(
        error(
            &text,
            "fn extracted(input: i32, C: i32) -> i32",
            Some("result")
        )
        .contains("shadow original declaration `C`")
    );
}

#[test]
fn signature_values_cannot_capture_runtime_context() {
    let text = source("", "length: usize", "", "let x = 1;");
    assert!(
        error(&text, "fn extracted() -> [u8; length]", Some("[0; length]"))
            .contains("runtime value `length` cannot supply declaration context")
    );
}
