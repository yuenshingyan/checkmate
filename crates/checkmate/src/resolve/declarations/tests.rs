use super::*;
use crate::resolve::collect_references;
use crate::scope::ScopeIndex;
use quote::ToTokens;
use std::{
    fs,
    process::{Command, Output},
};

pub(crate) struct Fixture {
    pub temp: tempfile::TempDir,
    pub source: ParsedSource,
    pub index: ScopeIndex,
    pub origin: SourceOrigin,
}

impl Fixture {
    pub fn new(text: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"source-model\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(temp.path().join("src/lib.rs"), text).unwrap();
        let source = ParsedSource::parse("src/lib.rs", text.to_owned()).unwrap();
        let index = ScopeIndex::build(&source).unwrap();
        let origin = SourceOrigin::discover(temp.path(), &source.path).unwrap();
        Self {
            temp,
            source,
            index,
            origin,
        }
    }

    pub fn resolver(&self, site: ReuseSite) -> Resolver<'_> {
        Resolver::new(&self.source, &self.index, &self.origin, site).unwrap()
    }

    pub fn position(&self, needle: &str) -> LineColumn {
        let (line, text) = self
            .source
            .text
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(needle))
            .unwrap();
        LineColumn {
            line: line + 1,
            column: text.find(needle).unwrap(),
        }
    }

    pub fn resolve(
        &self,
        path: &str,
        usage: ReferenceUse,
        needle: &str,
        site: ReuseSite,
    ) -> syn::Result<Resolved> {
        let at = self.position(needle);
        self.resolver(site).path(
            self.index.scope_at(at).unwrap(),
            &syn::parse_str(path).unwrap(),
            usage,
            at,
        )
    }

    pub fn compile_original(&self) -> Output {
        rustc()
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--crate-name=source_model",
            ])
            .arg(self.temp.path().join("src/lib.rs"))
            .arg("-o")
            .arg(self.temp.path().join("libsource_model.rlib"))
            .output()
            .unwrap()
    }

    fn consumer(&self, imports: &str, body: &str) -> Output {
        let lib = self.temp.path().join("libsource_model.rlib");
        let output = self.compile_original();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let consumer = self.temp.path().join("consumer.rs");
        fs::write(&consumer, format!("{imports}\nfn main() {{ {body} }}")).unwrap();
        let executable = self.temp.path().join("consumer");
        let output = rustc()
            .arg("--edition=2024")
            .arg(&consumer)
            .arg("--extern")
            .arg(format!("source_model={}", lib.display()))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        if output.status.success() {
            assert!(Command::new(executable).status().unwrap().success());
        }
        assert_eq!(
            fs::read_to_string(self.temp.path().join("src/lib.rs")).unwrap(),
            self.source.text
        );
        output
    }
}

fn rustc() -> Command {
    Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
}

fn import(item: &Resolved) -> String {
    item.import_as(&syn::parse_str("Account").unwrap())
        .unwrap()
        .to_string()
}

#[test]
fn generated_import_returns_the_exact_original_library_struct() {
    let fixture = Fixture::new(
        "pub mod models { pub struct User { pub name: String } }\nuse crate::models::User as Member;\npub fn original(name: String) -> Member { Member { name } }\n",
    );
    let item = fixture
        .resolve(
            "Member",
            ReferenceUse::Constructor,
            "Member {",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    assert_eq!(
        import(&item),
        "use :: source_model :: models :: User as Account ;"
    );
    let result = fixture.consumer(&import(&item), "fn build(name: String) -> Account { Account { name } } let user: source_model::models::User = build(String::from(\"Bob\")); assert_eq!(user.name, \"Bob\");");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn qualified_modules_self_super_and_aliases_share_original_identity() {
    let fixture = Fixture::new(
        "pub mod models { pub struct User { pub name: String } }\npub mod directory { pub use super::models::{self as schema, User as Member}; pub fn source() {} }\n",
    );
    let resolver = fixture.resolver(ReuseSite::PackageIntegrationTest);
    let at = fixture.position("pub fn source");
    let scope = fixture.index.scope_at(at).unwrap();
    let paths = [
        "Member",
        "schema::User",
        "self::Member",
        "super::models::User",
        "crate::directory::Member",
    ];
    let mut resolved = Vec::new();
    for path in paths {
        resolved.push(
            resolver
                .path(
                    scope,
                    &syn::parse_str(path).unwrap(),
                    ReferenceUse::Type,
                    at,
                )
                .unwrap(),
        );
    }
    assert!(resolved.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn public_reexport_can_name_a_type_in_a_private_module() {
    let fixture = Fixture::new(
        "mod hidden { pub struct User { pub name: String } }\npub use hidden::User as Public;\n",
    );
    let item = fixture
        .resolve(
            "Public",
            ReferenceUse::Type,
            "pub use",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    assert_eq!(import(&item), "use :: source_model :: Public as Account ;");
    let output = fixture.consumer(
        &import(&item),
        "let _: source_model::Public = Account { name: String::new() };",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn private_items_and_private_routes_never_turn_into_local_copies() {
    for text in [
        "pub mod models { struct User; }",
        "mod models { pub struct User; }",
    ] {
        let fixture = Fixture::new(text);
        let error = fixture
            .resolve(
                "crate::models::User",
                ReferenceUse::Type,
                "mod models",
                ReuseSite::PackageIntegrationTest,
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("public"), "{error}");
        let item = fixture
            .resolve(
                "crate::models::User",
                ReferenceUse::Type,
                "mod models",
                ReuseSite::SameTarget,
            )
            .unwrap();
        assert!(matches!(item, Resolved::Original { .. }));
    }
}

#[test]
fn rustc_still_enforces_field_privacy_for_an_importable_original_type() {
    let fixture = Fixture::new("pub mod models { pub struct User { name: String } }");
    let item = fixture
        .resolve(
            "crate::models::User",
            ReferenceUse::Constructor,
            "pub mod",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    let output = fixture.consumer(&import(&item), "let _ = Account { name: String::new() };");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private"));
}

#[test]
fn same_target_qualification_does_not_bypass_module_privacy() {
    let fixture = Fixture::new("mod models { struct User; }");
    let item = fixture
        .resolve(
            "crate::models::User",
            ReferenceUse::Type,
            "mod models",
            ReuseSite::SameTarget,
        )
        .unwrap();
    let file = fixture.temp.path().join("same_target.rs");
    fs::write(
        &file,
        format!("{}\n{}\nfn main() {{}}", fixture.source.text, import(&item)),
    )
    .unwrap();
    let output = rustc()
        .arg("--edition=2024")
        .arg(file)
        .arg("-o")
        .arg(fixture.temp.path().join("same_target"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private"));
}

#[test]
fn function_local_types_keep_ids_and_never_receive_crate_paths() {
    let fixture = Fixture::new(
        "pub fn source() {\nstruct User { name: String }\nlet user = User { name: String::new() };\n}\n",
    );
    let item = fixture
        .resolve(
            "User",
            ReferenceUse::Constructor,
            "let user",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    assert!(matches!(item, Resolved::LocalType { .. }));
    assert!(item.import_as(&syn::parse_str("User").unwrap()).is_err());
}

#[test]
fn item_namespaces_and_enum_variants_are_checked_not_guessed() {
    let fixture = Fixture::new(
        "pub struct Record { pub n: i32 }\npub struct Unit;\npub enum Choice { Empty, Number(i32), Named { n: i32 } }\npub type Alias = Record;\npub const COUNT: usize = 2;\npub fn helper() {}\n",
    );
    for (path, usage) in [
        ("Record", ReferenceUse::Type),
        ("Unit", ReferenceUse::TypeOrConst),
        ("Choice::Empty", ReferenceUse::Value),
        ("Choice::Named", ReferenceUse::Constructor),
        ("Alias", ReferenceUse::Constructor),
        ("COUNT", ReferenceUse::Value),
        ("helper", ReferenceUse::Value),
    ] {
        assert!(
            fixture
                .resolve(path, usage, "pub struct", ReuseSite::SameTarget)
                .is_ok(),
            "{path}"
        );
    }
    for (path, usage) in [
        ("Record", ReferenceUse::Value),
        ("Choice::Empty", ReferenceUse::Type),
        ("Choice::Named", ReferenceUse::Value),
        ("Choice::Missing", ReferenceUse::Value),
        ("Alias", ReferenceUse::Value),
        ("Record::new", ReferenceUse::Value),
    ] {
        assert!(
            fixture
                .resolve(path, usage, "pub struct", ReuseSite::SameTarget)
                .is_err(),
            "{path}"
        );
    }
}

#[test]
fn prelude_is_explicit_small_and_never_overrides_indexed_names() {
    let fixture = Fixture::new("pub struct String { pub n: i32 }\npub fn source() {}\n");
    assert!(matches!(
        fixture
            .resolve(
                "String",
                ReferenceUse::Type,
                "pub fn",
                ReuseSite::SameTarget
            )
            .unwrap(),
        Resolved::Original { .. }
    ));
    for name in ["bool", "usize", "Vec", "Option", "Box", "Clone"] {
        assert!(matches!(
            fixture
                .resolve(name, ReferenceUse::Type, "pub fn", ReuseSite::SameTarget)
                .unwrap(),
            Resolved::External(_)
        ));
    }
    assert!(
        fixture
            .resolve("Debug", ReferenceUse::Type, "pub fn", ReuseSite::SameTarget)
            .is_err()
    );
    assert!(
        fixture
            .resolve(
                "Vec::new",
                ReferenceUse::Value,
                "pub fn",
                ReuseSite::SameTarget
            )
            .is_err()
    );
    assert!(
        fixture
            .resolve(
                "Option::Some",
                ReferenceUse::Value,
                "pub fn",
                ReuseSite::SameTarget
            )
            .is_ok()
    );
}

#[test]
fn raw_identifiers_and_custom_library_names_survive_emitted_paths() {
    let mut fixture = Fixture::new("pub mod r#type { pub struct r#match; }\n");
    let manifest = fixture.temp.path().join("Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[lib]\nname = \"custom_model\"\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fixture.origin = SourceOrigin::discover(fixture.temp.path(), &fixture.source.path).unwrap();
    let item = fixture
        .resolve(
            "crate::r#type::r#match",
            ReferenceUse::Type,
            "pub mod",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    assert_eq!(
        import(&item),
        "use :: custom_model :: r#type :: r#match as Account ;"
    );
}

#[test]
fn generic_arguments_remain_in_the_ast_not_in_base_imports() {
    let fixture = Fixture::new("pub struct Wrapper<T> { pub value: T }\n");
    let item = fixture
        .resolve(
            "Wrapper<String>",
            ReferenceUse::Type,
            "pub struct",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap();
    assert_eq!(import(&item), "use :: source_model :: Wrapper as Account ;");
    let output = fixture.consumer(
        &import(&item),
        "let _: Account<String> = source_model::Wrapper { value: String::new() };",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn distinct_type_and_value_declarations_are_ambiguous_generic_arguments() {
    let fixture = Fixture::new("pub struct Same { pub n: i32 }\npub const Same: usize = 2;\n");
    let error = fixture
        .resolve(
            "Same",
            ReferenceUse::TypeOrConst,
            "pub struct",
            ReuseSite::SameTarget,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("ambiguous"));
}

#[test]
fn bad_imports_unknown_members_and_external_values_are_explicit_errors() {
    for (text, path, usage, expected) in [
        ("use B as A; use A as B;", "A", ReferenceUse::Type, "cyclic"),
        (
            "pub mod models {}",
            "crate::models::Missing",
            ReferenceUse::Type,
            "unknown member",
        ),
        (
            "pub fn source() {}",
            "::missing::User",
            ReferenceUse::Type,
            "external root",
        ),
        (
            "pub fn source() {}",
            "std::mem::size_of",
            ReferenceUse::Value,
            "semantic resolution",
        ),
    ] {
        let fixture = Fixture::new(text);
        let error = fixture
            .resolve(path, usage, text, ReuseSite::SameTarget)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn reference_plan_resolves_signature_constructor_output_and_reports_locations() {
    let fixture = Fixture::new(
        "pub struct User { pub name: String }\npub fn source(name: String) {\nlet user = User { name };\n}\n",
    );
    let selection = fixture.source.select(3, 3).unwrap();
    let references = collect_references(
        &fixture.source,
        &fixture.index,
        &selection,
        &syn::parse_str("fn build(name: String) -> User").unwrap(),
        Some(&syn::parse_str("user").unwrap()),
    )
    .unwrap();
    let resolved = fixture
        .resolver(ReuseSite::PackageIntegrationTest)
        .references(&references, &selection)
        .unwrap();
    assert_eq!(resolved.len(), references.occurrences.len());
    assert_eq!(
        resolved
            .iter()
            .filter(|item| matches!(item, Resolved::Original { .. }))
            .count(),
        2
    );
    let references = collect_references(
        &fixture.source,
        &fixture.index,
        &selection,
        &syn::parse_str("fn build(name: Unknown) -> User").unwrap(),
        None,
    )
    .unwrap();
    let error = fixture
        .resolver(ReuseSite::SameTarget)
        .references(&references, &selection)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("src/lib.rs:3-3")
            && error.contains("Signature")
            && error.contains("Unknown"),
        "{error}"
    );
}

#[test]
fn method_and_qualified_trait_work_cannot_be_silently_dropped() {
    let fixture = Fixture::new("pub fn source(values: &[i32]) {\nlet n = values.len();\n}\n");
    let selection = fixture.source.select(2, 2).unwrap();
    let refs = collect_references(
        &fixture.source,
        &fixture.index,
        &selection,
        &syn::parse_str("fn count(values: &[i32]) -> usize").unwrap(),
        Some(&syn::parse_str("n").unwrap()),
    )
    .unwrap();
    let error = fixture
        .resolver(ReuseSite::SameTarget)
        .references(&refs, &selection)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("method `len`") && error.contains("Source 2:"),
        "{error}"
    );
    assert!(
        refs.deferred_operations
            .iter()
            .all(|operation| operation.kind == DeferredKind::Method)
    );
}

#[test]
fn cargo_aliases_do_not_prove_cross_target_dependency_identity() {
    let mut fixture = Fixture::new("pub fn source() {}");
    let manifest = fixture.temp.path().join("Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[dependencies]\nrenamed = {{ package = \"other\", version = \"1\" }}\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fixture.origin = SourceOrigin::discover(fixture.temp.path(), &fixture.source.path).unwrap();
    assert!(
        fixture
            .resolve(
                "renamed::User",
                ReferenceUse::Type,
                "pub fn",
                ReuseSite::SameTarget
            )
            .is_ok()
    );
    let err = fixture
        .resolve(
            "renamed::User",
            ReferenceUse::Type,
            "pub fn",
            ReuseSite::PackageIntegrationTest,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("identity across"), "{err}");
}

#[test]
fn standard_library_shadowing_invalidates_the_prelude_mapping() {
    let mut fixture = Fixture::new("pub fn source() {}");
    let manifest = fixture.temp.path().join("Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n[dev-dependencies]\ncore = {{ package = \"other\", version = \"1\" }}\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fixture.origin = SourceOrigin::discover(fixture.temp.path(), &fixture.source.path).unwrap();
    let result = Resolver::new(
        &fixture.source,
        &fixture.index,
        &fixture.origin,
        ReuseSite::SameTarget,
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("prelude mapping is not proven")
    );
}

#[test]
fn mismatched_or_stale_source_identity_and_binary_reuse_fail() {
    let mut fixture = Fixture::new("pub struct User;");
    fixture.source.text.push('\n');
    assert!(
        Resolver::new(
            &fixture.source,
            &fixture.index,
            &fixture.origin,
            ReuseSite::SameTarget
        )
        .is_err()
    );
    fixture.source.text.pop();
    fixture.origin.target_kind = TargetKind::Binary;
    assert!(
        Resolver::new(
            &fixture.source,
            &fixture.index,
            &fixture.origin,
            ReuseSite::PackageIntegrationTest
        )
        .is_err()
    );
    let path = QualifiedPath {
        absolute: false,
        segments: vec!["crate".into(), "User".into()],
    };
    assert_eq!(
        path.syntax().unwrap().to_token_stream().to_string(),
        "crate :: User"
    );
}
