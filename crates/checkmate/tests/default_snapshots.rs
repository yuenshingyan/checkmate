//! Default snippet guards through native Cargo. Baselines are hand-authored here,
//! before compilation; Consumer inventories every input across every Cargo run.
mod support;
use std::path::Path;
use support::Consumer;

const LINE: &str = "let value = n + 1;\n";
const SOURCE: &str = "pub fn original(n: u8) -> u8 {\nlet value = n + 1;\nvalue\n}\n";

fn inline(name: &str, auto: bool, options: &str) -> String {
    let context = if auto {
        "context = \"auto\","
    } else {
        "context = \"explicit\","
    };
    format!(
        "checkmate::extract_fn! {{ fn_sig = \"fn {name}(n: u8) -> u8\", module = \"src/lib.rs\", start = 2, end = 2, output = value, {context} {options} }}\n"
    )
}

fn probe(invocation: &str, name: &str) -> String {
    format!(
        "{invocation}\n#[test] fn result() {{ for n in [0, 7, 200] {{ assert_eq!({name}(n), n + 1); assert_eq!({name}(n), regression_consumer::original(n)); }} }}\n"
    )
}

fn missing(c: &Consumer, resolved: &Path) {
    c.test(&["cannot read", resolved.to_str().unwrap()]);
    assert!(
        !resolved.exists(),
        "compilation created {}",
        resolved.display()
    );
}

fn drift(c: &Consumer, resolved: &Path) {
    c.test(&["snippet differs from snapshot", resolved.to_str().unwrap()]);
}

#[test]
fn inline_defaults_raw_identifiers_overrides_and_incremental_guards() {
    let c = Consumer::new();
    c.write("src/lib.rs", SOURCE);
    // Reuse one native target for both modes. No context snapshot is requested:
    // automatic context must remain usable without a second implicit baseline.
    for auto in [false, true] {
        for name in ["extracted", "r#type"] {
            let path = format!("snapshots/{}.rs", name.trim_start_matches("r#"));
            let call = inline(name, auto, "");
            c.write("tests/probe.rs", &probe(&call, name));
            missing(&c, &c.path(&path));
            assert!(!c.path("snapshots/r#type.rs").exists());
            c.write(&path, LINE); // Explicit outer-harness review, not macro output.
            c.test(&[]);

            // Only baseline bytes change; no source/caller/config touch or clean.
            c.write(&path, "let value = n + 9;\n");
            drift(&c, &c.path(&path));
            c.write(&path, LINE);
            c.test(&[]);

            // Explicit path wins even when the implicit default is wrong.
            c.write("review/approved.rs", LINE);
            c.write(&path, "wrong default\n");
            c.write(
                "tests/probe.rs",
                &probe(
                    &inline(name, auto, "snapshot = \"review/approved.rs\""),
                    name,
                ),
            );
            c.test(&[]);
            c.write("review/approved.rs", "wrong explicit baseline\n");
            drift(&c, &c.path("review/approved.rs"));
            c.write("review/approved.rs", LINE);
            c.test(&[]);

            c.write(
                "tests/probe.rs",
                &probe(&inline(name, auto, "snapshot = false"), name),
            );
            c.test(&[]); // Opt-out ignores an existing, drifting default.
            std::fs::remove_file(c.path(&path)).unwrap();
            c.test(&[]); // And permits its absence.
        }
    }
}

fn config(auto: bool, options: &str) -> String {
    let context = if auto {
        "context = \"auto\"\n"
    } else {
        "context = \"explicit\"\n"
    };
    format!(
        "[snippets.named_definition]\nfn_sig = \"fn r#type(n: u8) -> u8\"\nmodule = \"../src/lib.rs\"\nstart = 2\nend = 2\noutput = \"value\"\n{context}{options}\n"
    )
}

#[test]
fn config_directory_defaults_options_and_source_drift_precede_parsing() {
    let c = Consumer::new();
    c.write("src/lib.rs", SOURCE);
    c.write("tests/probe.rs", &probe("checkmate::extract_fn!(config = \"review/definitions.toml\", snippet = \"named_definition\");", "r#type"));
    // Wrong-location decoys cannot satisfy the config-relative function default.
    c.write("snapshots/type.rs", LINE);
    c.write("review/snapshots/named_definition.rs", LINE);
    c.write("review/snapshots/r#type.rs", LINE);
    let default = "review/snapshots/type.rs";
    // Resolution joins the config directory and option separately. Match that
    // exact displayed path, including Windows separators between those parts.
    let resolved_default = c.path("review").join("snapshots/type.rs");
    for auto in [false, true] {
        c.write("review/definitions.toml", &config(auto, ""));
        missing(&c, &resolved_default);
        c.write(default, LINE);
        c.test(&[]);
        c.write(default, "let value = n + 2;\n");
        drift(&c, &resolved_default);
        c.write(default, LINE);
        c.test(&[]);

        // Original library remains valid Rust, but the selected line alone is
        // incomplete. The snippet guard must win over body/automatic AST parsing.
        c.write(
            "src/lib.rs",
            "pub fn original(n: u8) -> u8 {\nlet value = (\nn + 1); value\n}\n",
        );
        let report = c.test(&[
            "snippet differs from snapshot",
            resolved_default.to_str().unwrap(),
        ]);
        assert!(
            !report.contains("not a complete Rust statement/expression sequence"),
            "{report}"
        );
        c.write("src/lib.rs", SOURCE);
        c.test(&[]);

        c.write(default, "wrong default\n");
        c.write("review/chosen.rs", LINE);
        c.write("chosen.rs", "wrong package-relative explicit path\n");
        c.write(
            "review/definitions.toml",
            &config(auto, "snapshot = \"chosen.rs\""),
        );
        c.test(&[]);
        c.write("review/chosen.rs", "wrong explicit baseline\n");
        drift(&c, &c.path("review").join("chosen.rs"));
        c.write("review/chosen.rs", LINE);
        c.test(&[]);
        c.write("review/definitions.toml", &config(auto, "snapshot = false"));
        c.test(&[]);
        std::fs::remove_file(c.path(default)).unwrap();
        c.test(&[]);

        for invalid in ["true", "\"\""] {
            c.write(
                "review/definitions.toml",
                &config(auto, &format!("snapshot = {invalid}")),
            );
            c.test(&["snapshot"]);
            c.write(
                "tests/probe.rs",
                &inline("rejected", auto, &format!("snapshot = {invalid}")),
            );
            c.test(&["snapshot"]);
            c.write("tests/probe.rs", &probe("checkmate::extract_fn!(config = \"review/definitions.toml\", snippet = \"named_definition\");", "r#type"));
        }
    }

    // context_snapshot is still explicit and auto-only, not a snippet opt-out.
    c.write(
        "review/definitions.toml",
        &config(
            false,
            "snapshot = false\ncontext_snapshot = \"context.toml\"",
        ),
    );
    c.test(&["requires context = \"auto\""]);
    c.write(
        "tests/probe.rs",
        &inline(
            "extracted",
            false,
            "snapshot = false, context_snapshot = \"context.toml\"",
        ),
    );
    c.test(&["requires context = \"auto\""]);
    c.write(
        "tests/probe.rs",
        &inline(
            "extracted",
            true,
            "snapshot = false, context_snapshot = \"context.toml\"",
        ),
    );
    c.test(&["cannot read context snapshot", "context.toml"]);
    assert!(!c.path("context.toml").exists());
}

#[test]
fn default_dependency_is_tracked_only_in_test_builds() {
    let c = Consumer::new();
    let manifest = std::fs::read_to_string(c.path("Cargo.toml"))
        .unwrap()
        .replace("[dev-dependencies]", "[dependencies]");
    c.write(
        "Cargo.toml",
        &format!("{manifest}\n[[bin]]\nname = \"driver\"\npath = \"driver/main.rs\"\n"),
    );
    c.write("src/lib.rs", SOURCE);
    c.write("snapshots/extracted.rs", LINE);
    c.write(
        "driver/main.rs",
        &format!(
            "{}fn main() {{}}\n#[test] fn result() {{ assert_eq!(extracted(7), 8); }}\n",
            inline("extracted", false, "")
        ),
    );
    c.run(&["build", "--bin", "driver"], &[]);
    c.run(&["test", "--bin", "driver"], &[]);

    // Same macro invocation, actually expanded in both modes. The non-test
    // artifact must not gain an include_str! dependency on the reviewed file.
    c.write("snapshots/extracted.rs", "drift\n");
    c.run(&["build", "--bin", "driver"], &[]);
    c.run(
        &["test", "--bin", "driver"],
        &["snippet differs from snapshot"],
    );
    c.write("snapshots/extracted.rs", LINE);
    c.run(&["test", "--bin", "driver"], &[]);
    c.run(&["build", "--bin", "driver"], &[]);
}
