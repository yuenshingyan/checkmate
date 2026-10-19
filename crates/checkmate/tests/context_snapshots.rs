//! Native public-macro context snapshot regressions. Each group shares a target.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct TempCargo(tempfile::TempDir);

impl TempCargo {
    fn new() -> Self {
        let c = Self(tempfile::tempdir().unwrap());
        c.write("Cargo.toml", &format!("[package]\nname = \"context-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dev-dependencies]\ncheckmate = {{ path = {:?} }}\n", env!("CARGO_MANIFEST_DIR")));
        c.write("src/lib.rs", SOURCE);
        c
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn run(&self, args: &[&str], errors: &[&str]) -> String {
        // Include all existing inputs, including Cargo.lock after its creation,
        // reports and baselines; intentional harness edits happen BEFORE this.
        fn collect(dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().unwrap() == "target" {
                    continue;
                }
                if path.is_dir() {
                    collect(&path, files);
                } else {
                    files.insert(path.clone(), fs::read(path).unwrap());
                }
            }
        }
        let mut before = BTreeMap::new();
        collect(self.0.path(), &mut before);
        let mut command = Command::new(env!("CARGO"));
        if self.path("without-test.sh").exists() {
            command.env("RUSTC_WRAPPER", self.path("without-test.sh"));
        }
        let output = command
            .args(args)
            .env("CARGO_TARGET_DIR", self.path("target"))
            .env("CARGO_NET_OFFLINE", "true")
            .current_dir(self.0.path())
            .output()
            .unwrap();
        for (path, bytes) in before {
            assert_eq!(
                fs::read(&path).unwrap(),
                bytes,
                "Cargo modified {}",
                path.display()
            );
        }
        let report = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            output.status.success(),
            errors.is_empty(),
            "{args:?}:\n{report}"
        );
        assert!(!report.contains("proc macro panicked"), "{report}");
        for error in errors {
            assert!(report.contains(error), "missing {error:?}:\n{report}");
        }
        report
    }

    fn check(&self, errors: &[&str]) -> String {
        self.run(&["test", "--quiet", "--test", "probe"], errors)
    }

    fn review(&self, baseline: &str) -> String {
        assert!(!self.path(baseline).exists());
        // Compilation alone must neither print review data nor create a file.
        let build = self.run(&["test", "--quiet", "--test", "probe", "--no-run"], &[]);
        assert!(!build.contains("checkmate-context"), "{build}");
        assert!(!self.path(baseline).exists());
        let output = self.run(
            &["test", "--quiet", "--test", "probe", "--", "--nocapture"],
            &[],
        );
        assert!(!self.path(baseline).exists());
        let (_, rest) = output
            .split_once("BEGIN_CONTEXT\n")
            .expect("user test report start");
        let (candidate, rest) = rest
            .split_once("END_CONTEXT")
            .expect("user test report end");
        assert!(!rest.contains("BEGIN_CONTEXT"));
        assert!(candidate.contains("format = \"checkmate-context\""));
        assert!(candidate.contains("version = 1\n"));
        assert!(candidate.contains("Counter"));
        assert!(!candidate.contains(self.0.path().to_str().unwrap()));
        // Explicit approval in the outer test harness, never in child Cargo.
        self.write(baseline, candidate);
        candidate.to_owned()
    }
}

const SOURCE: &str = "pub fn helper(n: u8) -> u8 { n + 1 }\npub fn original(n: u8) -> u8 {\nstruct Counter { n: u8 }\nimpl Counter { fn finish(self) -> u8 { self.n + 1 } }\nlet counter = Counter { n: helper(n) };\ncounter.finish()\n}\n";
const LINE: &str = "let counter = Counter { n: helper(n) };\n";
const PRINT: &str = "#[test]\nfn review() { println!(\"BEGIN_CONTEXT\\n{}END_CONTEXT\", CONTEXT()); assert_eq!(extracted(4), context_consumer::original(4)); }\n";

fn inline(options: &str) -> String {
    format!(
        "checkmate::extract_fn! {{ snapshot = false, fn_sig = \"fn extracted(n: u8) -> u8\", module = \"src/lib.rs\", start = 5, end = 5, output = counter.finish(), context = \"auto\", {options} }}\n"
    )
}

#[test]
fn explicit_review_validation_and_baseline_only_incremental_changes() {
    let c = TempCargo::new();
    let guarded = inline("context_snapshot = \"context.toml\", context_report = \"CONTEXT\"");
    c.write("tests/probe.rs", &format!("{guarded}{PRINT}"));
    c.check(&["CM013", "cannot read context snapshot", "context.toml"]);
    assert!(!c.path("context.toml").exists());

    c.write(
        "tests/probe.rs",
        &format!("{}{PRINT}", inline("context_report = \"CONTEXT\"")),
    );
    let baseline = c.review("context.toml");
    c.write("tests/probe.rs", &format!("{guarded}{PRINT}"));
    c.check(&[]);

    // Only the baseline changes from here: source/config/caller remain identical.
    c.write("context.toml", "[not valid TOML");
    c.check(&["CM013", "invalid context snapshot TOML"]);
    c.write("context.toml", &baseline);
    c.check(&[]);
    c.write(
        "context.toml",
        &baseline.replacen("version = 1", "version = 999", 1),
    );
    c.check(&["CM013", "unsupported context snapshot version"]);
    c.write("context.toml", &baseline);
    c.check(&[]);
    assert!(baseline.contains("n + 1"));
    c.write("context.toml", &baseline.replace("n + 1", "n + 2"));
    c.check(&["CM013", "context snapshot differs", "changed declarations"]);
    c.write("context.toml", &baseline);
    c.check(&[]);
}

#[test]
fn config_relative_guards_coexist_and_detect_unselected_context_changes() {
    let c = TempCargo::new();
    let config = "[snippets.demo]\ncontext = \"auto\"\nfn_sig = \"fn extracted(n: u8) -> u8\"\nmodule = \"../src/lib.rs\"\nstart = 5\nend = 5\noutput = \"counter.finish()\"\nsnapshot = \"snippet.rs\"\ncontext_report = \"CONTEXT\"\n";
    c.write("review/checkmate.toml", config);
    c.write("review/snippet.rs", LINE);
    c.write("tests/probe.rs", &format!("checkmate::extract_fn!(config = \"review/checkmate.toml\", snippet = \"demo\");\n{PRINT}"));
    c.review("review/context.toml");
    c.write(
        "review/checkmate.toml",
        &format!("{config}context_snapshot = \"context.toml\"\n"),
    );
    c.check(&[]);
    assert!(!c.path("context.toml").exists());

    for (source, detail) in [
        (
            SOURCE.replace("struct Counter { n: u8 }", "struct Counter { pub n: u8 }"),
            "declaration content changed",
        ),
        (SOURCE.replace("{ n + 1 }", "{ n + 2 }"), "helper"),
        (
            SOURCE.replace("self.n + 1", "self.n + 2"),
            "inherent implementation content changed",
        ),
    ] {
        assert_eq!(source.lines().nth(4).unwrap(), LINE.trim_end());
        c.write("src/lib.rs", &source);
        c.check(&["CM013", "context snapshot differs", detail]);
        c.write("src/lib.rs", SOURCE);
        c.check(&[]);
    }
    c.write("review/snippet.rs", "let counter = Counter { n: 0 };\n");
    c.check(&["CM002", "snippet differs from snapshot"]);
    c.write("review/snippet.rs", LINE);
    c.check(&[]);

    // Same selected spelling, different alias resolution (both targets exist).
    let alias = "pub fn first(n: u8) -> u8 { n + 1 }\npub fn second(n: u8) -> u8 { n + 2 }\n";
    let source = SOURCE.replace(
        "pub fn helper(n: u8) -> u8 { n + 1 }",
        "use first as helper;",
    );
    c.write("src/lib.rs", &format!("{source}{alias}"));
    c.write("review/checkmate.toml", config);
    fs::remove_file(c.path("review/context.toml")).unwrap();
    c.review("review/context.toml");
    c.write(
        "review/checkmate.toml",
        &format!("{config}context_snapshot = \"context.toml\"\n"),
    );
    c.check(&[]);
    c.write(
        "src/lib.rs",
        &format!(
            "{}{alias}",
            source.replace("use first as helper", "use second as helper")
        ),
    );
    c.check(&[
        "CM013",
        "context snapshot differs",
        "resolved reference mapping changed",
    ]);
}

#[test]
fn report_names_auto_only_options_and_test_only_visibility() {
    let c = TempCargo::new();
    for (name, prefix) in [
        ("CONTEXT", "const CONTEXT: &str = \"occupied\";\n"),
        ("extracted", ""),
    ] {
        c.write(
            "tests/probe.rs",
            &format!("{prefix}{}", inline(&format!("context_report = {name:?}"))),
        );
        c.check(&["CM013", "context_report name conflicts"]);
    }
    // Peer macro-produced names are Rust's duplicate-item boundary, not parsed
    // source bindings. This fails closed with E0428, without rewriting any input.
    let first = inline("context_report = \"review_data\"").replace("fn extracted", "fn first");
    let second = inline("context_report = \"review_data\"").replace("fn extracted", "fn second");
    c.write("tests/probe.rs", &format!("{first}{second}"));
    c.check(&["E0428", "review_data"]);

    // A review constant used to capture a binding pattern and silently return
    // 2 rather than the original 1. Accessors plus identifier checks avoid this.
    let pattern_source = "pub fn original(text: &str) -> u8 {\nlet answer = match text { n => 1, _ => 2 };\nanswer\n}\n";
    c.write("src/lib.rs", pattern_source);
    let pattern_call = "checkmate::extract_fn! { snapshot = false, fn_sig = \"fn extracted(text: &str) -> u8\", module = \"src/lib.rs\", start = 2, end = 2, output = answer, context = \"auto\", context_report = \"n\" }\n";
    c.write("tests/probe.rs", pattern_call);
    c.check(&["CM013", "generated-plan identifier"]);
    c.write("tests/probe.rs", &format!("{}\n#[test] fn unchanged() {{ assert_eq!(extracted(\"hello\"), context_consumer::original(\"hello\")); assert!(review_data().contains(\"checkmate-context\")); }}\n", pattern_call.replace("context_report = \"n\"", "context_report = \"review_data\"")));
    c.check(&[]);
    // Out-of-line caller scopes are deliberately not read by placement proof.
    // A function accessor (unlike a const) cannot capture this unseen pattern.
    c.write(
        "src/lib.rs",
        "pub fn original() -> u8 {\nlet answer = 1;\nanswer\n}\n",
    );
    c.write(
        "tests/support.rs",
        "use super::*;\npub fn answer(text: &str) -> u8 { match text { n => 1, _ => 2 } }\n",
    );
    c.write("tests/probe.rs", "checkmate::extract_fn! { snapshot = false, fn_sig = \"fn extracted() -> u8\", module = \"src/lib.rs\", start = 2, end = 2, output = answer, context = \"auto\", context_report = \"n\" }\nmod support;\n#[test] fn unchanged_child() { assert_eq!(support::answer(\"hello\"), 1); assert_eq!(extracted(), 1); assert!(n().contains(\"checkmate-context\")); }\n");
    c.check(&[]);
    c.write("src/lib.rs", SOURCE);
    for name in ["n", "None"] {
        c.write(
            "tests/probe.rs",
            &inline(&format!("context_report = {name:?}")),
        );
        c.check(&["CM013", "context_report name conflicts"]);
    }
    let method_source = SOURCE.replace(
        "self.n + 1 } }",
        "self.n + 1 } fn unused(self, method_arg: u8) -> u8 { method_arg } }",
    );
    c.write("src/lib.rs", &method_source);
    c.write("tests/probe.rs", &inline("context_report = \"method_arg\""));
    c.check(&["CM013", "generated-plan identifier"]);
    c.write("src/lib.rs", SOURCE);

    for option in [
        "context_snapshot = \"missing.toml\"",
        "context_report = \"CONTEXT\"",
    ] {
        let caller = inline(option).replace("context = \"auto\"", "context = \"explicit\"");
        c.write("tests/probe.rs", &caller);
        c.check(&["requires context = \"auto\""]);
        c.write("checkmate.toml", &format!("[snippets.demo]\ncontext = \"explicit\"\nsnapshot = false\nfn_sig = \"fn extracted(n: u8) -> u8\"\nmodule = \"src/lib.rs\"\nstart = 5\nend = 5\noutput = \"counter.finish()\"\n{option}\n"));
        c.write(
            "tests/probe.rs",
            "checkmate::extract_fn!(config = \"checkmate.toml\", snippet = \"demo\");\n",
        );
        c.check(&["requires context = \"auto\""]);
    }
    c.write(
        "tests/probe.rs",
        &format!("{}{PRINT}", inline("context_report = \"CONTEXT\"")),
    );
    c.check(&[]);
    // Preserve Cargo integration-target identity but remove rustc's --test.
    // harness=false is insufficient: Cargo supplies --cfg test in that case.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        c.write("without-test.sh", "#!/bin/bash\nargs=()\nfor arg in \"$@\"; do\n  if [[ \"$arg\" != --test ]]; then args+=(\"$arg\"); fi\ndone\nexec \"${args[@]}\"\n");
        fs::set_permissions(c.path("without-test.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        c.write(
            "tests/probe.rs",
            &format!("{}fn main() {{}}\n", inline("context_report = \"CONTEXT\"")),
        );
        c.check(&[]);
        c.write(
            "tests/probe.rs",
            &format!(
                "{}fn main() {{ let _: &str = CONTEXT(); }}\n",
                inline("context_report = \"CONTEXT\"")
            ),
        );
        c.check(&["E0425", "cannot find", "`CONTEXT`"]);
        // Check the extracted function independently: gating only the review
        // accessor must not satisfy the automatic non-test emission contract.
        c.write(
            "tests/probe.rs",
            &format!(
                "{}fn main() {{ let _ = extracted(7); }}\n",
                inline("context_report = \"CONTEXT\"")
            ),
        );
        c.check(&["E0425", "cannot find", "`extracted`"]);
    }
}
