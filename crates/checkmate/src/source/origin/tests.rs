use super::*;
use std::fs;

fn package(extra: &str, files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        format!("[package]\nname = \"demo-pkg\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{extra}"),
    )
    .unwrap();
    for file in files {
        let path = root.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }
    root
}

#[test]
fn explicit_binary_inference_requires_exactly_one_source_root() {
    for candidates in [
        vec![],
        vec!["src/bin/tool.rs"],
        vec!["src/bin/tool/main.rs"],
        vec!["src/bin/tool.rs", "src/bin/tool/main.rs"],
    ] {
        let mut inputs = vec!["src/lib.rs"];
        inputs.extend(&candidates);
        let root = package("[[bin]]\nname = \"tool\"\n", &inputs);
        let paths: Vec<_> = std::iter::once("Cargo.toml")
            .chain(inputs.iter().copied())
            .collect();
        let before: Vec<_> = paths
            .iter()
            .map(|p| fs::read(root.path().join(p)).unwrap())
            .collect();
        let source = if candidates.len() == 1 {
            candidates[0]
        } else {
            "src/lib.rs"
        };
        let result = SourceOrigin::discover(root.path(), Path::new(source));
        if candidates.len() == 1 {
            let origin = result.unwrap();
            assert_eq!(origin.target_kind, TargetKind::Binary);
            assert_eq!(origin.target_name, "tool");
            assert_eq!(
                origin.source,
                root.path().join(source).canonicalize().unwrap()
            );
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, Kind::SourceIdentity);
            assert!(
                error
                    .to_string()
                    .contains("binary tool: ambiguous or missing inferred root"),
                "{error}"
            );
        }
        for (path, bytes) in paths.iter().zip(before) {
            assert_eq!(fs::read(root.path().join(path)).unwrap(), bytes);
        }
    }
}

#[test]
fn malformed_source_fields_report_their_specific_validation_errors() {
    for (extra, message) in [
        ("autobins = 'yes'", "package.autobins must be a boolean"),
        ("[lib]\npath = 3", "path must be a string"),
    ] {
        let root = package(extra, &["src/lib.rs"]);
        let manifest = fs::read(root.path().join("Cargo.toml")).unwrap();
        let error = SourceOrigin::discover(root.path(), Path::new("src/lib.rs")).unwrap_err();
        assert_eq!(error.kind, Kind::SourceIdentity);
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(fs::read(root.path().join("Cargo.toml")).unwrap(), manifest);
        assert_eq!(fs::read(root.path().join("src/lib.rs")).unwrap(), b"");
    }
}

#[test]
fn conventional_roots_have_distinct_target_identities() {
    let root = package("", &["src/lib.rs", "src/main.rs"]);
    let lib = SourceOrigin::discover(root.path(), Path::new("src/lib.rs")).unwrap();
    let bin = SourceOrigin::discover(root.path(), Path::new("src/main.rs")).unwrap();
    assert_eq!(lib.target_kind, TargetKind::Library);
    assert_eq!(lib.target_name, "demo_pkg");
    assert_eq!(bin.target_kind, TargetKind::Binary);
    assert_eq!(bin.target_name, "demo-pkg");
    assert_ne!(lib.source, bin.source);
    assert!(lib.manifest.is_file());
    assert_eq!(lib.package_root, root.path().canonicalize().unwrap());
}

#[test]
fn explicit_target_paths_override_defaults_and_can_share_a_package() {
    let root = package(
        "[lib]\nname = \"shared\"\npath = \"lib/entry.rs\"\n[[bin]]\nname = \"demo-pkg\"\npath = \"app/entry.rs\"\n",
        &["lib/entry.rs", "app/entry.rs", "src/main.rs"],
    );
    assert_eq!(
        SourceOrigin::discover(root.path(), Path::new("lib/entry.rs"))
            .unwrap()
            .target_name,
        "shared"
    );
    assert_eq!(
        SourceOrigin::discover(root.path(), Path::new("app/entry.rs"))
            .unwrap()
            .target_kind,
        TargetKind::Binary
    );
    assert!(SourceOrigin::discover(root.path(), Path::new("src/main.rs")).is_err());
}

#[test]
fn auto_binary_forms_and_explicit_inferred_roots_are_supported() {
    let root = package(
        "[[bin]]\nname = \"tool\"\n",
        &["src/bin/tool.rs", "src/bin/other/main.rs"],
    );
    for (file, target) in [
        ("src/bin/tool.rs", "tool"),
        ("src/bin/other/main.rs", "other"),
    ] {
        assert_eq!(
            SourceOrigin::discover(root.path(), Path::new(file))
                .unwrap()
                .target_name,
            target
        );
    }
}

#[test]
fn arbitrary_module_files_and_disabled_auto_targets_are_not_guessed() {
    let root = package(
        "autobins = false\nautolib = false\n",
        &["src/main.rs", "src/lib.rs", "src/model.rs", "tests/test.rs"],
    );
    for path in ["src/main.rs", "src/lib.rs", "src/model.rs", "tests/test.rs"] {
        assert!(
            SourceOrigin::discover(root.path(), Path::new(path))
                .unwrap_err()
                .to_string()
                .contains("unsupported source origin")
        );
    }
}

#[test]
fn duplicated_source_roots_and_ambiguous_auto_binary_roots_are_errors() {
    let root = package(
        "[[bin]]\nname = \"one\"\npath = \"entry.rs\"\n[[bin]]\nname = \"two\"\npath = \"entry.rs\"\n",
        &["entry.rs"],
    );
    assert!(
        SourceOrigin::discover(root.path(), Path::new("entry.rs"))
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
    let root = package("", &["src/bin/tool.rs", "src/bin/tool/main.rs"]);
    assert!(
        SourceOrigin::discover(root.path(), Path::new("src/bin/tool.rs"))
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
}

#[test]
fn conflicting_default_binary_names_and_unresolved_editions_are_rejected() {
    let root = package("", &["src/main.rs", "src/bin/demo-pkg.rs"]);
    assert!(
        SourceOrigin::discover(root.path(), Path::new("src/main.rs"))
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
    let root = package("", &["src/lib.rs"]);
    for edition in ["edition = '2015'", "edition.workspace = true", ""] {
        fs::write(
            root.path().join("Cargo.toml"),
            format!("[package]\nname = 'demo'\nversion = '0.1.0'\n{edition}\n"),
        )
        .unwrap();
        assert!(
            SourceOrigin::discover(root.path(), Path::new("src/lib.rs"))
                .unwrap_err()
                .to_string()
                .contains("supported edition")
        );
    }
}

#[test]
fn dependency_aliases_are_candidates_not_active_visibility_proofs() {
    let root = package(
        r#"
[dependencies]
renamed-lib = { package = "original", version = "1", optional = true }
[dev-dependencies]
helper = { workspace = true }
[target.'cfg(unix)'.dependencies]
platform = "1"
"#,
        &["src/lib.rs"],
    );
    let origin = SourceOrigin::discover(root.path(), Path::new("src/lib.rs")).unwrap();
    for name in ["std", "core", "renamed_lib", "helper", "platform"] {
        assert!(origin.extern_candidates.contains(name));
    }
    assert!(!origin.extern_candidates.contains("original"));
}

#[test]
fn invalid_manifests_and_unmapped_paths_fail_explicitly() {
    for manifest in [
        "[workspace]",
        "broken toml",
        "[package]\nname = 'x'\nautobins = 'yes'",
        "[package]\nname = 'x'\n[lib]\npath = 3",
    ] {
        let root = package("", &["src/main.rs"]);
        fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
        assert!(SourceOrigin::discover(root.path(), Path::new("src/main.rs")).is_err());
    }
    let root = package("", &["external/main.rs"]);
    assert!(SourceOrigin::discover(root.path(), Path::new("external/main.rs")).is_err());
    assert!(SourceOrigin::discover(root.path(), Path::new("missing.rs")).is_err());
}
