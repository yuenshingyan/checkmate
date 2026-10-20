use super::*;
use crate::{
    resolve::{collect_references, declarations::ReuseSite, promote::PromotionPlan},
    scope::ScopeIndex,
    source::ast::ParsedSource,
};
use std::{collections::BTreeMap, fs, path::Path};

// Exercise caller-supplied data only; no framework invocation or baseline IO.
struct Fixture {
    temp: tempfile::TempDir,
    source: ParsedSource,
    index: ScopeIndex,
    origin: SourceOrigin,
}

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

impl Fixture {
    fn new(text: &str) -> Self {
        Self::at(text, "context-test", "context_lib", "src/lib.rs")
    }

    fn at(text: &str, package: &str, library: &str, path: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(path).parent().unwrap()).unwrap();
        fs::write(temp.path().join("Cargo.toml"), format!(
            "[package]\nname = {package:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = {library:?}\npath = {path:?}\n"
        )).unwrap();
        fs::write(temp.path().join(path), text).unwrap();
        let source = ParsedSource::parse(path, text).unwrap();
        let index = ScopeIndex::build(&source).unwrap();
        let origin = SourceOrigin::discover(temp.path(), &source.path).unwrap();
        Self {
            temp,
            source,
            index,
            origin,
        }
    }

    fn build(&self, signature: &str, output: Option<&str>) -> Result<ContextSnapshot> {
        let before = files(self.temp.path());
        let result = (|| {
            let line = self
                .source
                .text
                .lines()
                .position(|line| line.contains("let selected"))
                .unwrap()
                + 1;
            let selection = self.source.select(line, line)?;
            let resolver = Resolver::new(
                &self.source,
                &self.index,
                &self.origin,
                ReuseSite::SameTarget,
            )?;
            let output = output.map(|text| syn::parse_str::<syn::Expr>(text).unwrap());
            let references = collect_references(
                &self.source,
                &self.index,
                &selection,
                &syn::parse_str(signature).unwrap(),
                output.as_ref(),
            )?;
            let graph = Graph::build(&resolver, &references, &selection)?;
            let _plan = PromotionPlan::from_graph(
                &resolver,
                &references,
                &selection,
                &Default::default(),
                &graph,
            )?;
            ContextSnapshot::build(
                &self.origin,
                &resolver,
                &references,
                &selection,
                &graph,
                &syn::Visibility::Inherited,
            )
        })();
        assert_eq!(
            files(self.temp.path()),
            before,
            "analysis must be read-only even on failure"
        );
        result
    }

    fn snapshot(&self) -> ContextSnapshot {
        self.build("fn extracted()", None).unwrap()
    }
}

const LOCAL: &str = "fn owner() {\nstruct Local { field: u8 }\nimpl Local { fn unused(&self) -> u8 { 1 } }\nlet selected: Option<Local> = None;\n}\n";

fn mismatch(before: &ContextSnapshot, after: &ContextSnapshot) -> String {
    let baseline = before.render().unwrap();
    let rendered = after.render().unwrap();
    assert_ne!(baseline, rendered);
    let error = after.check(&baseline).unwrap_err();
    assert_eq!(error.kind, Kind::ContextSnapshot);
    // Checking caller-owned text does not mutate either model.
    assert_eq!(before.render().unwrap(), baseline);
    assert_eq!(after.render().unwrap(), rendered);
    error.to_string()
}

#[test]
fn roundtrip_is_read_only_and_uses_logical_package_relative_identities() {
    let fixture = Fixture::new(LOCAL);
    let before = files(fixture.temp.path());
    let snapshot = fixture.snapshot();
    let text = snapshot.render().unwrap();
    snapshot.check(&text).unwrap();
    let decoded: ContextSnapshot = toml::from_str(&text).unwrap();
    assert_eq!(decoded, snapshot);
    assert_eq!(decoded.render().unwrap(), text);
    assert_eq!(snapshot.source.path, "src/lib.rs");
    assert_eq!(snapshot.source.owner, "crate/fn:owner");
    assert_eq!(snapshot.source.package, "context-test");
    assert_eq!(snapshot.source.library, "context_lib");
    assert!(!text.contains(fixture.temp.path().to_str().unwrap()));
    assert!(!text.contains("DeclarationId"));
    assert!(!text.contains("CheckmateF"));
    assert!(
        snapshot
            .declarations
            .contains_key("source:crate/fn:owner/struct:Local")
    );
    for key in snapshot
        .declarations
        .keys()
        .filter(|key| key.starts_with("source:"))
    {
        assert!(key.starts_with("source:crate/"));
        assert!(!key.contains(".."));
    }
    assert_eq!(files(fixture.temp.path()), before);
}

#[test]
fn independent_checkouts_insertions_and_line_endings_render_identically() {
    let inserted = format!(
        "struct Unrelated;\nmod unrelated {{}}\n{}",
        LOCAL.replace("fn owner() {", "fn owner() {\n{}\n{ let unrelated = 0; }")
    );
    let spaced = LOCAL
        .replace("field: u8", "field :  u8")
        .replace("{ 1 }", "{\n  1\n}")
        .replace("let selected:", "    let selected :");
    let variants = [
        LOCAL.to_owned(),
        LOCAL.to_owned(),
        inserted,
        spaced,
        format!("\n\n{LOCAL}").replace('\n', "\r\n"),
    ];
    let baseline = Fixture::new(LOCAL).snapshot();
    for text in variants {
        let fixture = Fixture::new(&text);
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.render().unwrap(), baseline.render().unwrap());
        snapshot.check(&baseline.render().unwrap()).unwrap();
    }
}

#[test]
fn complete_impl_moves_and_rust_crlf_literal_normalization_are_stable() {
    let text = "fn owner() {\nstruct Local;\nimpl Local { fn first(self) -> u8 { 1 } }\nimpl Local { fn second(self) -> u8 { 2 } }\nlet selected: Option<Local> = None;\n}\n";
    let moved = text.replace(
        "impl Local { fn first(self) -> u8 { 1 } }\nimpl Local { fn second(self) -> u8 { 2 } }",
        "impl Local { fn second(self) -> u8 { 2 } }\n\nimpl Local { fn first(self) -> u8 { 1 } }",
    );
    assert_eq!(
        Fixture::new(text).snapshot().render().unwrap(),
        Fixture::new(&moved).snapshot().render().unwrap()
    );
    let literal =
        "pub const TEXT: &str = r#\"first\nsecond\"#;\nfn owner() {\nlet selected = TEXT;\n}\n";
    let before = Fixture::new(literal).snapshot();
    assert_eq!(
        before.render().unwrap(),
        Fixture::new(&literal.replace('\n', "\r\n"))
            .snapshot()
            .render()
            .unwrap()
    );
    let changed = Fixture::new(&literal.replace("first\nsecond", "first\\r\\nsecond")).snapshot();
    assert!(mismatch(&before, &changed).contains("declaration content changed"));
}

#[test]
fn field_and_unused_inherent_body_changes_are_not_hidden_by_same_extraction() {
    let baseline = Fixture::new(LOCAL).snapshot();
    for text in [
        LOCAL.replace("field: u8", "field: u16"),
        LOCAL.replace("{ 1 }", "{ 2 }"),
    ] {
        let changed = Fixture::new(&text).snapshot();
        assert_eq!(baseline.extraction, changed.extraction);
        assert!(mismatch(&baseline, &changed).contains("changed declarations"));
    }
}

#[test]
fn dependency_addition_removal_and_alias_retarget_change_graph() {
    let text = "fn owner() {\nstruct A;\nstruct B;\ntype Alias = A;\nstruct Local { field: Alias }\nlet selected: Option<Local> = None;\n}\n";
    let baseline = Fixture::new(text).snapshot();
    for changed in [
        text.replace("field: Alias", "field: ()"),
        text.replace("type Alias = A", "type Alias = B"),
    ] {
        let after = Fixture::new(&changed).snapshot();
        assert_eq!(baseline.extraction, after.extraction);
        mismatch(&baseline, &after);
        mismatch(&after, &baseline); // Addition as well as removal.
        assert_ne!(baseline.declarations, after.declarations);
    }
}

#[test]
fn identical_same_named_original_types_retarget_root_mapping() {
    let text = "mod a { pub struct Same; }\nmod b { pub struct Same; }\nuse a::Same as Alias;\nfn owner() {\nlet selected: Option<Alias> = None;\n}\n";
    let before = Fixture::new(text).snapshot();
    let after = Fixture::new(&text.replace("use a::", "use b::")).snapshot();
    assert_eq!(before.extraction, after.extraction);
    assert_ne!(before.references, after.references);
    assert!(mismatch(&before, &after).contains("resolved reference mapping changed"));
    assert!(
        before
            .declarations
            .contains_key("source:crate/mod:a/struct:Same")
    );
    assert!(
        after
            .declarations
            .contains_key("source:crate/mod:b/struct:Same")
    );
}

#[test]
fn source_package_library_path_and_selection_owner_are_identity() {
    let baseline = Fixture::new(LOCAL).snapshot();
    for fixture in [
        Fixture::at(LOCAL, "other-package", "context_lib", "src/lib.rs"),
        Fixture::at(LOCAL, "context-test", "other_library", "src/lib.rs"),
        Fixture::at(LOCAL, "context-test", "context_lib", "src/other.rs"),
        Fixture::new(&LOCAL.replace("fn owner", "fn another_owner")),
        Fixture::new(&format!("mod nested {{\n{LOCAL}\n}}")),
    ] {
        let after = fixture.snapshot();
        assert_eq!(baseline.extraction, after.extraction);
        assert!(mismatch(&baseline, &after).contains("source identity/owner changed"));
    }
    let mut outside = Fixture::new(LOCAL);
    outside.origin.source = outside.temp.path().join("../outside.rs");
    // The resolver rejects an invalid origin before snapshot construction.
    assert_eq!(
        outside.build("fn extracted()", None).unwrap_err().kind,
        Kind::SourceIdentity
    );
}

#[test]
fn original_content_and_implementations_are_captured_but_private_dependencies_are_terminal() {
    let text = "struct Private { hidden: u8 }\npub struct Original { private: Private }\nimpl Original { fn unused(&self) -> u8 { 1 } }\nfn owner() {\nlet selected: Option<Original> = None;\n}\n";
    let before = Fixture::new(text).snapshot();
    let original = &before.declarations["source:crate/struct:Original"];
    assert_eq!(original.role, "original");
    assert!(original.syntax.as_ref().unwrap().contains("Private"));
    assert!(original.implementations[0].contains("unused"));
    assert!(original.references.is_empty());
    assert!(original.edges.is_empty());
    assert!(
        !before
            .declarations
            .contains_key("source:crate/struct:Private")
    );
    for changed in [
        text.replace("private: Private", "private: Private, extra: u8"),
        text.replace("{ 1 }", "{ 2 }"),
    ] {
        let after = Fixture::new(&changed).snapshot();
        assert_eq!(before.extraction, after.extraction);
        mismatch(&before, &after);
    }
    // A terminal original node is not a transitive fingerprint of its private dependencies.
    let after = Fixture::new(&text.replace("hidden: u8", "hidden: u16")).snapshot();
    assert_eq!(before.render().unwrap(), after.render().unwrap());
}

#[test]
fn signature_output_and_required_inputs_are_review_data() {
    let fixture = Fixture::new("struct Original;\nfn owner() {\nlet selected = input;\n}\n");
    let before = fixture
        .build("fn extracted(input: Original)", Some("selected"))
        .unwrap();
    assert!(
        before
            .references
            .iter()
            .any(|mapping| mapping.origin == "signature")
    );
    assert!(before.required_inputs.iter().any(|name| name == "input"));
    for (signature, output) in [
        ("fn renamed(input: Original)", "selected"),
        ("fn extracted(input: Original) -> Original", "selected"),
        ("fn extracted(input: Original)", "Original"),
    ] {
        let after = fixture.build(signature, Some(output)).unwrap();
        mismatch(&before, &after);
    }
    let output = fixture
        .build("fn extracted(input: Original)", Some("Original"))
        .unwrap();
    assert!(
        output
            .references
            .iter()
            .any(|mapping| mapping.origin == "output")
    );
}

#[test]
fn ambiguous_same_name_local_anchors_fail_closed() {
    let text = "fn owner() {\n{ struct Local; }\n{ struct Local;\nlet selected: Option<Local> = None;\n}\n}\n";
    let error = Fixture::new(text)
        .build("fn extracted()", None)
        .unwrap_err();
    assert_eq!(error.kind, Kind::ContextSnapshot);
    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn malformed_token_paths_and_routes_leave_review_inputs_unchanged() {
    let fixture = Fixture::new(LOCAL);
    let before = files(fixture.temp.path());
    let snapshot = fixture.snapshot();
    let rendered = snapshot.render().unwrap();
    snapshot.check(&rendered).unwrap();
    let value: toml::Value = toml::from_str(&rendered).unwrap();
    for field in ["path", "route"] {
        let mut invalid = value.clone();
        let mapping = &mut invalid["references"].as_array_mut().unwrap()[0];
        let table = if field == "route" {
            mapping["target"].as_table_mut().unwrap()
        } else {
            mapping.as_table_mut().unwrap()
        };
        table.insert(field.into(), toml::Value::String("(".into()));
        let text = toml::to_string(&invalid).unwrap();
        // Valid strict-schema TOML: only token parsing should reject this baseline.
        let _: ContextSnapshot = toml::from_str(&text).unwrap();
        let error = snapshot.check(&text).unwrap_err();
        assert_eq!(error.kind, Kind::ContextSnapshot, "{field}");
        assert!(
            error
                .to_string()
                .contains("invalid tokens in context snapshot"),
            "{field}: {error}"
        );
        assert_eq!(snapshot.render().unwrap(), rendered);
        assert_eq!(files(fixture.temp.path()), before);
    }
}

#[test]
fn malformed_versions_schema_and_embedded_syntax_fail_read_only() {
    let fixture = Fixture::new(LOCAL);
    let before = files(fixture.temp.path());
    let snapshot = fixture.snapshot();
    let rendered = snapshot.render().unwrap();
    let value: toml::Value = toml::from_str(&rendered).unwrap();
    let mut cases = vec!["not = [ valid TOML".to_owned()];
    for field in [
        "format",
        "version",
        "source",
        "extraction",
        "references",
        "declarations",
    ] {
        let mut invalid = value.clone();
        invalid.as_table_mut().unwrap().remove(field);
        cases.push(toml::to_string(&invalid).unwrap());
    }
    for (field, replacement) in [
        ("format", toml::Value::String("other-format".into())),
        ("version", toml::Value::Integer(0)),
        ("version", toml::Value::Integer(2)),
        ("version", toml::Value::String("1".into())),
        ("unknown", toml::Value::Boolean(true)),
        ("extraction", toml::Value::String("fn {".into())),
    ] {
        let mut invalid = value.clone();
        invalid
            .as_table_mut()
            .unwrap()
            .insert(field.into(), replacement);
        cases.push(toml::to_string(&invalid).unwrap());
    }
    let key = "source:crate/fn:owner/struct:Local";
    for field in ["syntax", "implementations", "unknown"] {
        let mut invalid = value.clone();
        let replacement = if field == "implementations" {
            toml::Value::Array(vec![toml::Value::String("impl {".into())])
        } else {
            toml::Value::String("struct {".into())
        };
        invalid["declarations"][key]
            .as_table_mut()
            .unwrap()
            .insert(field.into(), replacement);
        cases.push(toml::to_string(&invalid).unwrap());
    }
    for nested_target in [false, true] {
        let mut invalid = value.clone();
        let mapping = &mut invalid["references"].as_array_mut().unwrap()[0];
        let table = if nested_target {
            mapping["target"].as_table_mut().unwrap()
        } else {
            mapping.as_table_mut().unwrap()
        };
        table.insert("unknown".into(), toml::Value::Boolean(true));
        cases.push(toml::to_string(&invalid).unwrap());
    }
    let mut invalid = value.clone();
    invalid["source"]
        .as_table_mut()
        .unwrap()
        .insert("unknown".into(), toml::Value::Boolean(true));
    cases.push(toml::to_string(&invalid).unwrap());
    for text in cases {
        let error = snapshot.check(&text).unwrap_err();
        assert_eq!(error.kind, Kind::ContextSnapshot, "{text}");
        assert_eq!(snapshot.render().unwrap(), rendered);
        assert_eq!(files(fixture.temp.path()), before);
    }
}
