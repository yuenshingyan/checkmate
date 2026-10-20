use super::{Caller, Input, placement};
use crate::source::origin::SourceOrigin;
use quote::ToTokens;
use std::{
    fs,
    path::{Path, PathBuf},
};
use syn::visit::Visit;

const SOURCE: &str = "pub fn original() {\nlet answer = 42;\n}\n";
const INVOCATION: &str = "::checkmate::extract_fn! { fn_sig = \"fn extracted() -> i32\", module = \"src/lib.rs\", start = 2, end = 2, output = answer, context = \"auto\" }\n";

struct Fixture {
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let f = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        f.manifest("", "");
        f.write("src/lib.rs", SOURCE);
        f.write("tests/probe.rs", INVOCATION);
        f
    }
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn path(&self, path: &str) -> PathBuf {
        self.root().join(path)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.path(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn manifest(&self, options: &str, targets: &str) {
        // A real dependency value is required for the literal macro-path proof.
        let dependency = toml::Value::String(env!("CARGO_MANIFEST_DIR").into());
        self.write("Cargo.toml", &format!("[package]\nname = \"source-model\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{options}\n[dev-dependencies]\ncheckmate = {{ path = {dependency} }}\n{targets}\n"));
    }
    fn caller(&self, file: &str, name: &str) -> Caller {
        // Unit tests use proc_macro2's fallback parser naturally. Obtain the
        // location from the actual parsed invocation, not a guessed line/column.
        struct Find(Option<(proc_macro2::LineColumn, String)>);
        impl<'ast> Visit<'ast> for Find {
            fn visit_macro(&mut self, mac: &'ast syn::Macro) {
                if mac.path.segments.last().unwrap().ident == "extract_fn" {
                    let anchor = mac.tokens.clone().into_iter().next().unwrap().span();
                    self.0 = Some((anchor.start(), mac.tokens.to_string()));
                }
            }
        }
        let syntax = syn::parse_file(&fs::read_to_string(self.path(file)).unwrap()).unwrap();
        let mut find = Find(None);
        find.visit_file(&syntax);
        let (at, tokens) = find.0.expect("fixture has a literal invocation");
        Caller {
            file: Some(self.path(file)),
            at,
            tokens,
            crate_name: Some(name.into()),
            package_name: Some("source-model".into()),
            bin_name: None,
        }
    }
    fn input(&self) -> Input {
        self.input_for(self.caller("tests/probe.rs", "probe"))
    }
    fn verify(&self, caller: &Caller) -> crate::diagnostics::Result<placement::Placement> {
        let origin = SourceOrigin::discover(self.root(), &self.path("src/lib.rs")).unwrap();
        placement::verify(&origin, caller, "extracted")
    }
}

fn failure(input: Input, needle: &str) {
    let error =
        super::run(input).expect_err("automatic errors must not fall back to ordinary extraction");
    assert!(error.contains(needle), "expected {needle:?}: {error}");
    assert!(error.contains("task011-unit"), "{error}");
}

#[test]
fn unavailable_caller_location_explains_editor_requirement_not_bad_source() {
    let f = Fixture::new();
    let mut input = f.input();
    input.caller.file = None;
    let error = super::run(input).unwrap_err();
    for expected in [
        "CM003",
        "compiler invocation file is unavailable",
        "Editor macro hosts need source-location support",
        "docs/editor-support.md",
        "does not mean the selected source path is wrong",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn pipeline_imports_original_copies_local_discloses_identity_and_tracks_all_inputs() {
    let f = Fixture::new();
    let source = "pub struct Original { pub value: i32 }\npub fn original() {\nstruct Local { value: Original }\nimpl Local { fn into_original(self) -> Original { self.value } }\nlet result = Local { value: Original { value: 42 } };\n}\n";
    f.write("src/lib.rs", source);
    f.write("checkmate.toml", "# deliberately read-only configuration\n");
    f.write(
        "snippet.snap",
        "let result = Local { value: Original { value: 42 } };\n",
    );
    let paths = [
        "src/lib.rs",
        "tests/probe.rs",
        "Cargo.toml",
        "checkmate.toml",
        "snippet.snap",
    ];
    let before: Vec<_> = paths.iter().map(|p| fs::read(f.path(p)).unwrap()).collect();
    let mut input = f.input();
    input.start = 5;
    input.end = 5;
    input.signature = "pub fn extracted() -> Local".into();
    input.output = Some("result".into());
    input.config = Some(f.path("checkmate.toml"));
    input.snapshot = Some(f.path("snippet.snap"));
    let output = super::run(input).unwrap();
    let syntax = syn::parse_file(&output).unwrap();
    assert!(
        syntax.items.iter().any(|i| matches!(i, syn::Item::Use(_))),
        "{output}"
    );
    assert!(output.contains(":: source_model :: Original"), "{output}");
    assert!(output.contains("NEW nominal identity"), "{output}");
    assert_eq!(
        syntax
            .items
            .iter()
            .filter(|i| matches!(i, syn::Item::Struct(_)))
            .count(),
        1
    );
    for item in &syntax.items {
        let attrs = match item {
            syn::Item::Const(i) => &i.attrs,
            syn::Item::Use(i) => &i.attrs,
            syn::Item::Struct(i) => &i.attrs,
            syn::Item::Impl(i) => &i.attrs,
            syn::Item::Fn(i) => &i.attrs,
            other => panic!("unexpected generated item: {}", other.to_token_stream()),
        };
        assert!(
            attrs
                .iter()
                .any(|a| a.meta.to_token_stream().to_string() == "cfg (test)")
        );
    }
    let dependencies: Vec<_> = syntax
        .items
        .iter()
        .filter_map(|i| {
            let syn::Item::Const(i) = i else { return None };
            let syn::Expr::Macro(expr) = &*i.expr else {
                panic!("dependency must be include_str!")
            };
            assert_eq!(expr.mac.path.segments.last().unwrap().ident, "include_str");
            Some(
                syn::parse2::<syn::LitStr>(expr.mac.tokens.clone())
                    .unwrap()
                    .value(),
            )
        })
        .collect();
    // macOS temp roots may have both /var and canonical /private/var
    // spellings; both guards are valid, but must cover exactly these inputs.
    let actual: std::collections::BTreeSet<_> = dependencies
        .iter()
        .map(|p| Path::new(p).canonicalize().unwrap())
        .collect();
    let expected: std::collections::BTreeSet<_> = paths
        .iter()
        .map(|p| f.path(p).canonicalize().unwrap())
        .collect();
    assert_eq!(actual, expected);
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(fs::read(f.path(path)).unwrap(), bytes, "modified {path}");
    }
}

#[test]
fn explicit_and_automatic_flat_and_directory_targets_are_verified() {
    for (file, options, targets) in [
        ("tests/probe.rs", "", ""),
        ("tests/probe/main.rs", "", ""),
        (
            "custom/caller.rs",
            "autotests = false",
            "[[test]]\nname = \"probe\"\npath = \"custom/caller.rs\"",
        ),
    ] {
        let f = Fixture::new();
        fs::remove_file(f.path("tests/probe.rs")).unwrap();
        f.write(file, &format!("// location intentionally offset\n\n{INVOCATION}\n#[test] fn user_test() {{ extracted(); }}\n"));
        f.manifest(options, targets);
        let caller = f.caller(file, "probe");
        assert_eq!(caller.at.line, 3);
        let placement = f.verify(&caller).unwrap();
        assert_eq!(placement.caller, f.path(file).canonicalize().unwrap());
        assert!(!placement.occupied.contains("extracted"));
        let mut input = f.input_for(caller);
        input.caller.tokens = input
            .caller
            .tokens
            .parse::<proc_macro2::TokenStream>()
            .unwrap()
            .to_string();
        assert!(super::run(input).is_ok());
    }
}
impl Fixture {
    fn input_for(&self, caller: Caller) -> Input {
        Input {
            package: self.root().into(),
            source: self.path("src/lib.rs"),
            config: None,
            snapshot: None,
            context_snapshot: None,
            context_report: None,
            snippet: "task011-unit".into(),
            signature: "fn extracted() -> i32".into(),
            output: Some("answer".into()),
            start: 2,
            end: 2,
            caller,
        }
    }
}

#[test]
fn explicit_test_inference_requires_exactly_one_caller_root() {
    for candidates in [
        vec![],
        vec!["tests/probe.rs"],
        vec!["tests/probe/main.rs"],
        vec!["tests/probe.rs", "tests/probe/main.rs"],
    ] {
        let f = Fixture::new();
        fs::remove_file(f.path("tests/probe.rs")).unwrap();
        f.manifest("", "[[test]]\nname = \"probe\"");
        f.write("custom/caller.rs", INVOCATION);
        for path in &candidates {
            f.write(path, INVOCATION);
        }
        let file = candidates.first().copied().unwrap_or("custom/caller.rs");
        let caller = f.caller(file, "probe");
        let paths: Vec<_> = ["Cargo.toml", "src/lib.rs", "custom/caller.rs"]
            .into_iter()
            .chain(candidates.iter().copied())
            .collect();
        let before: Vec<_> = paths.iter().map(|p| fs::read(f.path(p)).unwrap()).collect();
        let result = f.verify(&caller);
        if candidates.len() == 1 {
            // Placement exposes the verified root, not a target kind/name;
            // verification proves this caller's `probe` integration-test identity.
            let placement = result.unwrap();
            assert_eq!(placement.caller, f.path(file).canonicalize().unwrap());
            assert_eq!(caller.crate_name.as_deref(), Some("probe"));
        } else {
            let error = result.err().expect("inference must fail closed");
            assert_eq!(error.kind, crate::diagnostics::Kind::SourceIdentity);
            assert!(
                error.to_string().contains(
                    "explicit target has ambiguous/missing inferred path; specify its Cargo path"
                ),
                "{error}"
            );
        }
        for (path, bytes) in paths.iter().zip(before) {
            assert_eq!(fs::read(f.path(path)).unwrap(), bytes);
        }
    }
}

#[test]
fn malformed_target_fields_do_not_bypass_placement_validation() {
    for (options, targets, message) in [
        (
            "autotests = 'yes'",
            "",
            "Cargo auto-target switches must be booleans",
        ),
        (
            "",
            "[[test]]\npath = 'tests/probe.rs'",
            "explicit Cargo targets require a name for placement proof",
        ),
        (
            "",
            "[[test]]\nname = 'probe'\npath = 3",
            "Cargo target paths must be strings",
        ),
    ] {
        let f = Fixture::new();
        f.manifest(options, targets);
        let paths = ["Cargo.toml", "src/lib.rs", "tests/probe.rs"];
        let before: Vec<_> = paths.iter().map(|p| fs::read(f.path(p)).unwrap()).collect();
        let error = f
            .verify(&f.caller("tests/probe.rs", "probe"))
            .err()
            .expect("invalid Cargo field must fail");
        assert_eq!(error.kind, crate::diagnostics::Kind::SourceIdentity);
        assert!(error.to_string().contains(message), "{error}");
        for (path, bytes) in paths.iter().zip(before) {
            assert_eq!(fs::read(f.path(path)).unwrap(), bytes);
        }
    }
}

#[test]
fn unrelated_implicit_default_binary_does_not_invalidate_the_test_root() {
    let f = Fixture::new();
    f.write("src/main.rs", "fn main() {}\n");
    f.manifest("", "[[bin]]\nname = \"source-model\"\n");
    assert!(f.verify(&f.input().caller).is_ok());
    assert!(super::run(f.input()).is_ok());
}

#[test]
fn missing_locations_and_unproven_compiler_identities_are_errors() {
    let f = Fixture::new();
    for case in 0..7 {
        let mut input = f.input();
        let needle = match case {
            0 => {
                input.caller.file = None;
                "file is unavailable"
            }
            1 => {
                input.caller.at.line = 0;
                "source locations"
            }
            2 => {
                input.caller.crate_name = None;
                "crate identity"
            }
            3 => {
                input.caller.crate_name = Some("wrong".into());
                "unique declared"
            }
            4 => {
                input.caller.package_name = None;
                "package identity"
            }
            5 => {
                input.caller.package_name = Some("wrong".into());
                "package identity"
            }
            _ => {
                input.caller.bin_name = Some("probe".into());
                "integration-test"
            }
        };
        failure(input, needle);
    }
}

#[test]
fn nested_modules_and_included_files_are_not_root_invocations() {
    let f = Fixture::new();
    f.write(
        "tests/probe.rs",
        &format!("mod nested {{\n{INVOCATION}\n}}"),
    );
    failure(f.input(), "crate-root");
    f.write("tests/body.rs", INVOCATION);
    f.write("tests/probe.rs", "include!(\"body.rs\");\n");
    failure(
        f.input_for(f.caller("tests/body.rs", "probe")),
        "unique declared",
    );
    f.write(
        "tests/probe.rs",
        &format!("include!(\"body.rs\");\n{INVOCATION}"),
    );
    failure(f.input(), "other root macros");
}

#[test]
fn duplicate_target_names_source_files_and_other_target_classes_are_rejected() {
    for (targets, needle) in [
        (
            "[[test]]\nname = \"probe\"\npath = \"tests/probe.rs\"\n[[test]]\nname = \"probe\"\npath = \"other.rs\"",
            "unique declared",
        ),
        (
            "[[test]]\nname = \"probe\"\npath = \"tests/probe.rs\"\n[[test]]\nname = \"second\"\npath = \"tests/probe.rs\"",
            "multiple test targets",
        ),
        (
            "[[example]]\nname = \"probe\"\npath = \"other.rs\"",
            "overlaps another",
        ),
    ] {
        let f = Fixture::new();
        f.write("other.rs", "fn main() {}\n");
        f.manifest("", targets);
        assert!(f.verify(&f.input().caller).is_err());
        failure(f.input(), needle);
    }
    let f = Fixture::new();
    f.write("tests/probe/main.rs", INVOCATION);
    failure(f.input(), "unique declared");
}

#[test]
fn cargo_build_script_names_do_not_prove_integration_test_placement() {
    for name in ["build_script_build", "build_script_custom_builder"] {
        let f = Fixture::new();
        let path = format!("tests/{name}.rs");
        f.write(&path, INVOCATION);
        f.write(
            "build.rs",
            &format!("#[path = {path:?}] mod included;\nfn main() {{}}\n"),
        );
        failure(
            f.input_for(f.caller(&path, name)),
            "reserved for Cargo build scripts",
        );
    }
}

#[test]
fn unsafe_caller_names_macros_attributes_and_function_collisions_fail_closed() {
    for (prefix, needle) in [
        ("use source_model::*;", "glob imports"),
        ("extern crate source_model;", "extern crate"),
        ("unknown!();", "other root macros"),
        ("#[unknown] fn helper() {}", "attributes"),
        ("use source_model as std;", "shadow"),
        ("use source_model as core;", "shadow"),
        ("mod source_model {}", "shadow"),
        ("mod checkmate {}", "shadow"),
        ("fn extracted() {}", "conflicts"),
    ] {
        let f = Fixture::new();
        f.write("tests/probe.rs", &format!("{prefix}\n{INVOCATION}"));
        failure(f.input(), needle);
    }
}

#[test]
fn token_mismatch_and_unproven_macro_dependency_are_errors() {
    let f = Fixture::new();
    let mut input = f.input();
    input.caller.tokens = "different = true".into();
    failure(input, "tokens do not match");
    f.write(
        "Cargo.toml",
        "[package]\nname = \"source-model\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    failure(f.input(), "fully qualified");
}

#[test]
fn unsupported_sources_binary_placement_and_snapshot_failures_never_fallback_or_write() {
    let f = Fixture::new();
    f.write("src/module.rs", SOURCE);
    f.write("src/main.rs", SOURCE);
    for (source, needle) in [
        ("src/module.rs", "unsupported source origin"),
        ("src/main.rs", "library source root"),
        ("missing.rs", "cannot read"),
    ] {
        let mut input = f.input();
        input.source = f.path(source);
        failure(input, needle);
    }
    f.write("bad.snap", "different\n");
    for (snapshot, needle) in [
        ("bad.snap", "differs from snapshot"),
        ("missing.snap", "cannot read"),
    ] {
        let mut input = f.input();
        input.snapshot = Some(f.path(snapshot));
        failure(input, needle);
    }
    assert_eq!(
        fs::read_to_string(f.path("bad.snap")).unwrap(),
        "different\n"
    );
    assert!(!f.path("missing.snap").exists());
    assert_eq!(fs::read_to_string(f.path("src/lib.rs")).unwrap(), SOURCE);
    f.write(
        "src/lib.rs",
        "pub fn original() {\nlet answer = line!();\n}\n",
    );
    let bytes = fs::read(f.path("src/lib.rs")).unwrap();
    failure(f.input(), "macro");
    assert_eq!(fs::read(f.path("src/lib.rs")).unwrap(), bytes);
}
