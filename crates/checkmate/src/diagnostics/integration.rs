//! Exercise real internal failure paths with configured snippet identity. This is
//! a test harness, not task 011's public pipeline or invocation-placement proof.
use super::*;
use crate::arguments::Invocation;
use crate::resolve::{
    collect_references,
    declarations::{Resolver, ReuseSite},
    promote::PromotionPlan,
};
use crate::scope::ScopeIndex;
use crate::source::{ast::ParsedSource, origin::SourceOrigin};
use std::{collections::BTreeSet, fs};

struct Case {
    root: tempfile::TempDir,
    source: String,
    start: usize,
    end: usize,
}
impl Case {
    fn new(source: &str, start: usize, end: usize, signature: &str, output: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"diagnostic-model\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(root.path().join("src/lib.rs"), source).unwrap();
        let config = format!(
            "[snippets.checked]\nfn_sig = {signature:?}\nmodule = \"src/lib.rs\"\nstart = {start}\nend = {end}\noutput = {output:?}\nsnapshot = \"reviewed.rs\"\n"
        );
        fs::write(root.path().join("checkmate.toml"), config).unwrap();
        fs::write(
            root.path().join("reviewed.rs"),
            "reviewed baseline must not be rewritten\n",
        )
        .unwrap();
        Self {
            root,
            source: source.into(),
            start,
            end,
        }
    }
    fn request(&self) -> Request<'_> {
        Request {
            snippet: "checkmate.toml [snippets.checked]",
            path: Path::new("src/lib.rs"),
            start: self.start,
            end: self.end,
        }
    }
    fn attempt(&self) -> Result<PromotionPlan> {
        let invocation: Invocation =
            syn::parse_str("config = \"checkmate.toml\", snippet = \"checked\"")?;
        let configuration = invocation.resolve(self.root.path())?;
        assert!(configuration.context.contains("[snippets.checked]"));
        let source = ParsedSource::parse(configuration.args.module.value(), self.source.clone())?;
        let selection = source.select(
            configuration.args.start.base10_parse()?,
            configuration.args.end.base10_parse()?,
        )?;
        let index = ScopeIndex::build(&source)?;
        let origin = SourceOrigin::discover(self.root.path(), &source.path)?;
        let signature = configuration.args.fn_sig.parse()?;
        let references = collect_references(
            &source,
            &index,
            &selection,
            &signature,
            configuration.args.output.as_ref(),
        )?;
        let resolver = Resolver::new(&source, &index, &origin, ReuseSite::PackageIntegrationTest)?;
        PromotionPlan::build(&resolver, &references, &selection, &BTreeSet::new())
    }
    fn failure(&self) -> Error {
        let paths = ["Cargo.toml", "src/lib.rs", "checkmate.toml", "reviewed.rs"];
        let before: Vec<_> = paths
            .iter()
            .map(|path| fs::read(self.root.path().join(path)).unwrap())
            .collect();
        let error = self.attempt().err().expect("expected a checked rejection");
        let after: Vec<_> = paths
            .iter()
            .map(|path| fs::read(self.root.path().join(path)).unwrap())
            .collect();
        assert_eq!(before, after, "analysis error paths must remain read-only");
        let report = error.render(&self.request(), &self.source, None);
        assert!(report.len() <= 8192);
        assert!(
            report.contains("[snippets.checked]") && report.contains("src/lib.rs"),
            "{report}"
        );
        assert!(
            report.contains(&format!(
                "source selection: lines {}..={}",
                self.start, self.end
            )),
            "{report}"
        );
        error
    }
}

#[test]
fn missing_input_retains_typed_parameter_identity_and_source_site() {
    let case = Case::new(
        "pub fn source(n: u8) {\nlet value = n + 1;\n}\n",
        2,
        2,
        "fn extract() -> u8",
        "value",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::MissingInput);
    assert_eq!(
        error.subject.as_ref().unwrap(),
        &Subject {
            name: "n".into(),
            kind: Some(DeclarationKind::Parameter)
        }
    );
    assert_eq!(error.site.unwrap().origin, Origin::Source);
    assert_eq!(error.site.unwrap().range.start.line, 2);
    let report = error.render(&case.request(), &case.source, None);
    assert!(
        report.contains("CM004") && report.contains("typed fn_sig parameter"),
        "{report}"
    );
}

#[test]
fn inaccessible_original_and_unknown_identifier_are_distinct_without_message_parsing() {
    let private = Case::new(
        "struct Hidden { n: u8 }\npub fn source(n: u8) {\nlet user = Hidden { n };\n}\n",
        3,
        3,
        "fn extract(n: u8) -> Hidden",
        "user",
    );
    let error = private.failure();
    assert_eq!(error.kind, Kind::InaccessibleItem);
    assert_eq!(
        error.subject.as_ref().unwrap().kind,
        Some(DeclarationKind::Struct)
    );
    assert_eq!(error.site.unwrap().origin, Origin::Signature);
    let unknown = Case::new(
        "pub fn source(n: u8) {\nlet user = Missing { n };\n}\n",
        2,
        2,
        "fn extract(n: u8)",
        "()",
    );
    let error = unknown.failure();
    assert_eq!(error.kind, Kind::UnknownItem);
    assert_eq!(error.subject.as_ref().unwrap().name, "Missing");
    // Structured categories survive arbitrary wording changes.
    assert_eq!(
        error.context("a different explanation").kind,
        Kind::UnknownItem
    );
}

#[test]
fn macro_forms_and_cfg_are_distinct_at_the_actual_source_site() {
    for source in [
        "pub fn source(n: u8) {\nlet value = make!(n);\n}\n",
        "pub fn source(n: u8) {\nmake!(n);\n}\n",
        "pub fn source(n: u8) {\nlet pattern!() = n;\n}\n",
        "make_items!();\npub fn source() {\nlet value = 1;\n}\n",
        "#[derive(Clone)] struct User;\npub fn source() {\nlet value = 1;\n}\n",
    ] {
        let line = if source.starts_with("pub") { 2 } else { 3 };
        let case = Case::new(source, line, line, "fn extract()", "()");
        let error = case.failure();
        assert_eq!(error.kind, Kind::Macro, "{error}");
        assert_eq!(error.site.unwrap().origin, Origin::Source);
    }
    let case = Case::new(
        "#[cfg(any())]\npub fn source(n: u8) {\nlet value = n;\n}\n",
        3,
        3,
        "fn extract(n: u8) -> u8",
        "value",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::Conditional);
    assert_eq!(error.site.unwrap().range.start.line, 1);
    assert!(
        error
            .render(&case.request(), &case.source, None)
            .contains("#[cfg(any())]")
    );
}

#[test]
fn unsafe_promotion_identifies_declaration_and_dependency_chain() {
    let case = Case::new(
        "pub fn source(n: u8) {\nstruct User<T> { n: T }\nlet user = User { n };\n}\n",
        3,
        3,
        "fn extract(n: u8) -> User<u8>",
        "user",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::UnsafePromotion);
    assert_eq!(error.subject.as_ref().unwrap().name, "User");
    assert_eq!(
        error.subject.as_ref().unwrap().kind,
        Some(DeclarationKind::Struct)
    );
    assert_eq!(error.chain[0].name, "User");
    assert_eq!(error.site.unwrap().range.start.line, 2);
}

#[test]
fn transitive_failure_retains_leaf_category_instead_of_reclassifying_as_generic_promotion() {
    let case = Case::new(
        "struct Hidden;\npub fn source() {\nstruct Leaf { value: Hidden }\ntype Root = Leaf;\nlet value: Option<Root> = None;\n}\n",
        5,
        5,
        "fn extract() -> Option<Root>",
        "value",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::InaccessibleItem);
    assert_eq!(error.subject.as_ref().unwrap().name, "Hidden");
    assert_eq!(
        error
            .chain
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        ["Root", "Leaf"]
    );
    assert_eq!(error.site.unwrap().range.start.line, 3);
    let case = Case::new(
        "pub struct Wrapper<T> { pub value: T }\npub fn source() {\nstruct User { value: Wrapper<Missing> }\nlet value: Option<User> = None;\n}\n",
        4,
        4,
        "fn extract() -> Option<User>",
        "value",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::UnknownItem);
    assert_eq!(error.subject.as_ref().unwrap().name, "Missing");
    assert_eq!(error.chain[0].name, "User");
}

#[test]
fn long_unicode_dependency_chain_is_bounded_and_keeps_both_ends() {
    let mut source = "pub fn source() {\n".to_owned();
    for i in 0..500 {
        source.push_str(&format!("type 类型{i} = 类型{};\n", i + 1));
    }
    source.push_str("type 类型500 = 未知;\nlet value: Option<类型0> = None;\n}\n");
    let case = Case::new(&source, 503, 503, "fn extract() -> Option<类型0>", "value");
    let error = case.failure();
    assert_eq!(error.kind, Kind::UnknownItem);
    assert_eq!(error.subject.as_ref().unwrap().name, "未知");
    assert_eq!(error.chain.first().unwrap().name, "类型0");
    assert_eq!(error.chain.last().unwrap().name, "类型500");
    assert_eq!(error.chain.len() + error.omitted_chain, 501);
    let report = error.render(&case.request(), &source, None);
    assert!(
        report.contains("类型0")
            && report.contains("类型500")
            && report.contains("未知")
            && report.contains("omitted"),
        "{report}"
    );
    assert_eq!(report, error.render(&case.request(), &source, None));
    assert!(std::str::from_utf8(report.as_bytes()).is_ok());
}

#[test]
fn signature_and_output_locations_are_not_presented_as_original_source() {
    let source = "pub fn source(n: u8) {\nlet value = n;\n}\n";
    for (signature, output, expected) in [
        ("fn extract(n: Unknown)", "()", Origin::Signature),
        ("fn extract(n: u8)", "Unknown", Origin::Output),
        ("fn extract(n: u8)", "{ unknown!(); }", Origin::Output),
    ] {
        let case = Case::new(source, 2, 2, signature, output);
        let error = case.failure();
        assert_eq!(error.site.unwrap().origin, expected, "{error}");
        let report = error.render(&case.request(), source, None);
        assert!(report.contains("preview unavailable"), "{report}");
        assert!(!report.contains("| let value"), "{report}");
    }
}

#[test]
fn promotion_policy_and_method_failures_keep_precise_locations() {
    let case = Case::new(
        "pub fn source(n: u8) {\nstruct User { pub(crate) n: u8 }\nlet user = User { n };\n}\n",
        3,
        3,
        "fn extract(n: u8) -> User",
        "user",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::UnsafePromotion);
    assert_eq!(error.subject.as_ref().unwrap().name, "User");
    assert_eq!(error.site.unwrap().range.start.line, 2);
    let case = Case::new(
        "pub fn source(n: u8) {\nstruct User { n: u8 }\nimpl User { fn unsupported(self) -> u32 { self.n.count_ones() } }\nlet user = User { n };\n}\n",
        4,
        4,
        "fn extract(n: u8) -> User",
        "user",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::MethodResolution);
    assert_eq!(error.subject.as_ref().unwrap().name, "count_ones");
    assert_eq!(error.site.unwrap().range.start.line, 3);
    assert_eq!(error.chain[0].name, "User");
}

#[test]
fn invalid_selection_syntax_and_origin_have_their_own_categories() {
    let case = Case::new(
        "pub fn source() {\nlet value = 1;\n}\n",
        0,
        10,
        "fn extract()",
        "()",
    );
    assert_eq!(case.failure().kind, Kind::Selection);
    let case = Case::new("not a complete Rust file", 1, 1, "fn extract()", "()");
    assert_eq!(case.failure().kind, Kind::Syntax);
    let case = Case::new(
        "pub fn source() {\nlet value = 1;\n}\n",
        2,
        2,
        "fn extract()",
        "()",
    );
    fs::write(
        case.root.path().join("Cargo.toml"),
        "[package]\nname = \"unsupported\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    assert_eq!(case.failure().kind, Kind::SourceIdentity);
}

#[test]
fn post_graph_promotion_failures_keep_the_dependency_path() {
    let case = Case::new(
        "pub fn source() {\nstruct Leaf { pub(crate) n: u8 }\ntype Root = Leaf;\nlet value: Option<Root> = None;\n}\n",
        4,
        4,
        "fn extract() -> Option<Root>",
        "value",
    );
    let error = case.failure();
    assert_eq!(error.kind, Kind::UnsafePromotion);
    assert_eq!(error.subject.as_ref().unwrap().name, "Leaf");
    assert_eq!(error.site.unwrap().range.start.line, 2);
    assert_eq!(
        error
            .chain
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        ["Root", "Leaf"]
    );
    // The same policy can fail while validating a selected declaration in the
    // generated shell, before individual graph nodes are emitted.
    let selected = Case::new(&case.source, 2, 4, "fn extract() -> Option<Root>", "value");
    let error = selected.failure();
    assert_eq!(error.subject.as_ref().unwrap().name, "Leaf");
    assert_eq!(error.chain.last().unwrap().name, "Leaf");
    assert_eq!(error.site.unwrap().range.start.line, 2);
}

#[test]
fn import_origin_identity_failures_have_identity_guidance() {
    use crate::scope::imports::ImportId;
    let case = Case::new(
        "pub struct Thing;\nuse crate::Thing as Alias;\npub fn source() {\nlet value = 1;\n}\n",
        4,
        4,
        "fn extract()",
        "()",
    );
    let origin = SourceOrigin::discover(case.root.path(), Path::new("src/lib.rs")).unwrap();
    fs::write(case.root.path().join("src/other.rs"), &case.source).unwrap();
    let paths = [
        "Cargo.toml",
        "checkmate.toml",
        "src/lib.rs",
        "src/other.rs",
        "reviewed.rs",
    ];
    let before: Vec<_> = paths
        .iter()
        .map(|path| fs::read(case.root.path().join(path)).unwrap())
        .collect();
    for path in ["src/other.rs", "src/missing.rs"] {
        let parsed = ParsedSource::parse(path, case.source.clone()).unwrap();
        let index = ScopeIndex::build(&parsed).unwrap();
        let error = index.import_origin(ImportId(0), &origin).unwrap_err();
        assert_eq!(error.kind, Kind::SourceIdentity);
        let report = error.render(&case.request(), &case.source, None);
        assert!(
            report.contains("CM003") && report.contains("Verify source/caller paths"),
            "{report}"
        );
    }
    let after: Vec<_> = paths
        .iter()
        .map(|path| fs::read(case.root.path().join(path)).unwrap())
        .collect();
    assert_eq!(before, after);
    assert!(!case.root.path().join("src/missing.rs").exists());
}

#[test]
fn automatic_mode_requires_compiler_metadata_instead_of_guessing_placement() {
    let invocation = syn::parse_str::<Invocation>(
        "fn_sig = \"fn test()\", module = \"src/lib.rs\", start = 1, end = 1, context = \"auto\"",
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let error = crate::generate::expand(invocation, root.path())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("CM003") && error.contains("compiler invocation metadata"),
        "{error}"
    );
}
