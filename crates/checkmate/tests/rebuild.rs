//! Exercise rustc diagnostics and dependency tracking through a real consumer.
use std::{fs, path::Path, process::Command};

fn check(root: &Path, expected_error: Option<&str>) {
    let output = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--quiet", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(root.join("target"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(message) = expected_error {
        assert!(!output.status.success(), "expected compile failure");
        assert!(stderr.contains(message), "missing {message:?}:\n{stderr}");
    } else {
        assert!(
            output.status.success(),
            "{stderr}\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn external_source_config_and_snapshot_edits_trigger_recompilation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "checkmate-consumer"
version = "0.0.0"
edition = "2024"
[workspace]
[dev-dependencies]
checkmate = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(test)]
mod tests {
    use checkmate::extract_fn;
    extract_fn!(config = "checkmate.toml", snippet = "add");
    #[test]
    fn runs() { assert_eq!(add(1), 2); }
}
"#,
    )
    .unwrap();
    let config = r#"
[snippets.add]
fn_sig = "fn add(a: i32) -> i32"
module = "external.rs"
start = 2
end = 2
output = "c"
snapshot = "snapshot.rs"
"#;
    fs::write(root.join("checkmate.toml"), config).unwrap();
    fs::write(
        root.join("external.rs"),
        "// external source, not a module\nlet c = a + 1;\n",
    )
    .unwrap();
    fs::write(root.join("snapshot.rs"), "let c = a + 1;\n").unwrap();
    check(root, None);

    fs::write(root.join("snapshot.rs"), "let c = a + 2;\n").unwrap();
    check(root, Some("snippet differs from snapshot"));
    fs::write(root.join("snapshot.rs"), "let c = a + 1;\n").unwrap();
    check(root, None);

    fs::write(root.join("checkmate.toml"), config.replace("= 2", "= 1")).unwrap();
    check(root, Some("external.rs:1-1"));
    fs::write(root.join("checkmate.toml"), config).unwrap();
    check(root, None);

    fs::write(
        root.join("external.rs"),
        "// new line\n// external source, not a module\nlet c = a + 1;\n",
    )
    .unwrap();
    check(root, Some("snippet differs from snapshot"));
    fs::write(root.join("checkmate.toml"), config.replace("= 2", "= 3")).unwrap();
    check(root, None);
}
