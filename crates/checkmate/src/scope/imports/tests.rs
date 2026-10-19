use super::*;
use std::fs;
use std::path::Path;

fn project(text: &str) -> (tempfile::TempDir, ParsedSource, ScopeIndex, SourceOrigin) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("Cargo.toml"), "[package]\nname = 'test-root'\nversion = '0.1.0'\nedition = '2024'\n[dependencies]\nrenamed = { package = 'actual', version = '1', optional = true }\n").unwrap();
    fs::write(root.path().join("src/lib.rs"), text).unwrap();
    let source = ParsedSource::parse("src/lib.rs", text).unwrap();
    let index = ScopeIndex::build(&source).unwrap();
    let origin = SourceOrigin::discover(root.path(), Path::new("src/lib.rs")).unwrap();
    (root, source, index, origin)
}

fn import(index: &ScopeIndex, alias: &str) -> ImportId {
    ImportId(
        index
            .imports
            .iter()
            .position(|import| import.alias == alias)
            .unwrap(),
    )
}

fn origin(index: &ScopeIndex, source: &SourceOrigin, alias: &str) -> PathOrigin {
    index.import_origin(import(index, alias), source).unwrap()
}

fn segments(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).into()).collect()
}

#[test]
fn nested_use_trees_and_self_aliases_keep_original_paths() {
    let (_root, _source, index, source_origin) = project(
        r#"
mod models { pub struct User { name: String } pub mod inner { pub struct Item; } }
use crate::models::{self as model, User as Person, inner::{self, Item as Thing}};
use model::User as Another;
"#,
    );
    assert_eq!(index.imports.len(), 5);
    assert_eq!(
        origin(&index, &source_origin, "Person"),
        PathOrigin::Crate(segments(&["models", "User"]))
    );
    assert_eq!(
        origin(&index, &source_origin, "Another"),
        origin(&index, &source_origin, "Person")
    );
    assert_eq!(
        origin(&index, &source_origin, "Thing"),
        PathOrigin::Crate(segments(&["models", "inner", "Item"]))
    );
    let module = &index.imports[import(&index, "model").0];
    assert!(module.type_only);
    assert_eq!(module.path.segments, segments(&["crate", "models"]));
    assert_eq!(
        index.lookup(ScopeId(0), Namespace::Value, "model", module.range.end),
        Lookup::Missing
    );
    assert_eq!(
        index.lookup(ScopeId(0), Namespace::Type, "model", module.range.end),
        Lookup::Import(import(&index, "model"))
    );
}

#[test]
fn crate_self_and_repeated_super_paths_preserve_source_module_meaning() {
    let (_root, _source, index, source_origin) = project(
        r#"
struct Root;
mod outer {
 struct Parent;
 mod inner {
  struct Local;
  use self::Local as Here;
  use super::Parent as Up;
  use super::super::Root as Top;
  use crate::Root as Absolute;
 }
}
"#,
    );
    for (alias, path) in [
        ("Here", vec!["outer", "inner", "Local"]),
        ("Up", vec!["outer", "Parent"]),
        ("Top", vec!["Root"]),
        ("Absolute", vec!["Root"]),
    ] {
        assert_eq!(
            origin(&index, &source_origin, alias),
            PathOrigin::Crate(segments(&path))
        );
    }
}

#[test]
fn lexical_aliases_are_hoisted_but_do_not_leak_across_modules_or_blocks() {
    let (_root, _source, index, source_origin) = project(
        r#"
use crate::outer as top;
mod outer {
 use crate::Root as Name;
 fn f() { use super::Root as Name; let _: Name; }
}
struct Root;
"#,
    );
    let aliases: Vec<_> = index
        .imports
        .iter()
        .enumerate()
        .filter(|(_, import)| import.alias == "Name")
        .collect();
    assert_eq!(aliases.len(), 2);
    assert_ne!(aliases[0].1.scope, aliases[1].1.scope);
    assert_eq!(
        index.lookup(ScopeId(0), Namespace::Type, "Name", aliases[1].1.range.end),
        Lookup::Missing
    );
    assert_eq!(
        index
            .import_origin(ImportId(aliases[1].0), &source_origin)
            .unwrap(),
        PathOrigin::Crate(segments(&["Root"]))
    );
    let block = aliases[1].1.scope;
    assert_eq!(
        index.lookup(block, Namespace::Type, "Name", aliases[1].1.range.start),
        Lookup::Import(ImportId(aliases[1].0))
    );
}

#[test]
fn absolute_paths_dependency_aliases_and_std_are_external_candidates() {
    let (_root, _source, index, source_origin) = project(
        r#"
use ::unknown_dependency::Type as Explicit;
use renamed::Type as Renamed;
use std::collections::HashMap as Map;
"#,
    );
    assert!(index.imports[import(&index, "Explicit").0].path.absolute);
    for (alias, path) in [
        ("Explicit", vec!["unknown_dependency", "Type"]),
        ("Renamed", vec!["renamed", "Type"]),
        ("Map", vec!["std", "collections", "HashMap"]),
    ] {
        assert_eq!(
            origin(&index, &source_origin, alias),
            PathOrigin::External(segments(&path))
        );
    }
}

#[test]
fn lexical_module_named_std_shadows_extern_candidate() {
    let (_root, _source, index, source_origin) =
        project("mod std { pub struct Local; } use std::Local as Alias;");
    assert_eq!(
        origin(&index, &source_origin, "Alias"),
        PathOrigin::Crate(segments(&["std", "Local"]))
    );
}

#[test]
fn imports_preserve_visibility_without_asserting_accessibility() {
    let (_root, _source, index, source_origin) = project(
        "mod hidden { struct Secret; } mod child { pub(super) use crate::hidden::Secret as Alias; }",
    );
    let alias = &index.imports[import(&index, "Alias").0];
    assert!(alias.visibility.contains("super"));
    // Provenance is known, but rustc would reject this private target. This pass
    // neither marks it accessible nor widens the original visibility.
    assert_eq!(
        origin(&index, &source_origin, "Alias"),
        PathOrigin::Crate(segments(&["hidden", "Secret"]))
    );
}

#[test]
fn raw_identifiers_keep_spelling_and_normalize_semantic_names() {
    let (_root, _source, index, source_origin) =
        project("mod r#type { pub struct User; } use crate::r#type::User as r#alias;");
    let alias = &index.imports[import(&index, "alias").0];
    assert_eq!(alias.path.segments, segments(&["crate", "r#type", "User"]));
    assert_eq!(
        origin(&index, &source_origin, "alias"),
        PathOrigin::Crate(segments(&["type", "User"]))
    );
}

#[test]
fn unknown_roots_cycles_and_escaping_super_are_explicit_errors() {
    for (text, alias, message) in [
        (
            "use missing::Type as Alias;",
            "Alias",
            "ambiguous path root",
        ),
        ("use super::Type as Alias;", "Alias", "escapes"),
        (
            "use Second as First; use First as Second;",
            "First",
            "cyclic import aliases",
        ),
    ] {
        let (_root, _source, index, source_origin) = project(text);
        let error = index
            .import_origin(import(&index, alias), &source_origin)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
        assert!(error.contains("src/lib.rs:"));
    }
}

#[test]
fn unindexed_import_namespaces_and_binding_patterns_fail_conservatively() {
    for (text, message) in [
        ("use crate::a::*;", "glob imports"),
        ("use crate::a::Trait as _;", "underscore imports"),
        ("use crate::a::A as X; use crate::b::B as X;", "overlaps"),
        ("use crate::a::A as X; struct X;", "overlaps"),
        ("struct X; use crate::a::A as X;", "overlaps"),
        (
            "use crate::LIMIT; fn f() { let LIMIT = 1; }",
            "target-kind resolution",
        ),
        ("#[path = \"elsewhere.rs\"] mod x;", "custom #[path]"),
    ] {
        let source = ParsedSource::parse("unsupported.rs", text).unwrap();
        assert!(
            ScopeIndex::build(&source)
                .unwrap_err()
                .to_string()
                .contains(message),
            "{text}"
        );
    }
}

#[test]
fn local_items_are_not_misrepresented_as_crate_paths() {
    let (_root, _source, index, source_origin) =
        project("fn f() { struct Local; use Local as Alias; }");
    assert!(matches!(
        origin(&index, &source_origin, "Alias"),
        PathOrigin::Declaration { .. }
    ));
    let (_root, _source, index, source_origin) =
        project("fn f() { mod local { use self::Type as Alias; struct Type; } }");
    assert!(
        index
            .import_origin(import(&index, "Alias"), &source_origin)
            .unwrap_err()
            .to_string()
            .contains("block-local module")
    );
}

#[test]
fn path_normalization_requires_matching_verified_origin_and_no_dropped_generics() {
    let (root, _source, index, source_origin) = project("struct User; use crate::User as Alias;");
    let at = LineColumn { line: 1, column: 0 };
    assert_eq!(
        index
            .path_origin(
                ScopeId(0),
                &syn::parse_str("Alias").unwrap(),
                Namespace::Type,
                at,
                &source_origin
            )
            .unwrap(),
        PathOrigin::Crate(segments(&["User"]))
    );
    assert!(
        index
            .path_origin(
                ScopeId(0),
                &syn::parse_str("Alias<u8>").unwrap(),
                Namespace::Type,
                at,
                &source_origin
            )
            .is_err()
    );
    fs::write(root.path().join("src/main.rs"), "fn main() {}").unwrap();
    let other = SourceOrigin::discover(root.path(), Path::new("src/main.rs")).unwrap();
    assert!(
        index
            .import_origin(import(&index, "Alias"), &other)
            .unwrap_err()
            .to_string()
            .contains("different indexed file")
    );
}
