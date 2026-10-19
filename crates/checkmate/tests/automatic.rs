//! Public automatic-context regressions through rustc, not the fallback parser.
//! Each group reuses one consumer and target directory across incremental builds.
use std::{fs, path::PathBuf, process::Command};

struct Consumer {
    directory: tempfile::TempDir,
    inputs: Vec<&'static str>,
}

impl Consumer {
    fn new() -> Self {
        let mut this = Self {
            directory: tempfile::tempdir().unwrap(),
            inputs: Vec::new(),
        };
        this.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"automatic-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dev-dependencies]\ncheckmate = {{ path = {:?} }}\n",
                env!("CARGO_MANIFEST_DIR")
            ),
        );
        this
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn write(&mut self, name: &'static str, text: &str) {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        if !self.inputs.contains(&name) {
            self.inputs.push(name);
        }
    }

    fn check(&self, errors: &[&str]) {
        self.run(&["test", "--test", "probe"], errors);
    }

    fn run(&self, args: &[&str], errors: &[&str]) {
        // Capture AFTER intentional edits, BEFORE Cargo. Assert each input
        // separately, even on expected failures (especially snapshot baselines).
        let before: Vec<_> = self
            .inputs
            .iter()
            .map(|name| (*name, fs::read(self.path(name)).unwrap()))
            .collect();
        let output = Command::new(env!("CARGO"))
            .args(args)
            .args(["--offline", "--quiet", "--manifest-path"])
            .arg(self.path("Cargo.toml"))
            .env("CARGO_TARGET_DIR", self.path("target"))
            .current_dir(self.directory.path())
            .output()
            .unwrap();
        for (name, bytes) in before {
            assert_eq!(
                fs::read(self.path(name)).unwrap(),
                bytes,
                "Cargo changed {name}"
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
        assert!(
            !report.contains("proc macro panicked")
                && !report.contains("automatic analysis failed")
                && !report.contains("procedural macro API is used outside"),
            "backend failure: {report}"
        );
        for error in errors {
            assert!(report.contains(error), "missing {error:?}:\n{report}");
        }
    }
}

fn automatic(signature: &str, start: usize, end: usize, output: &str) -> String {
    format!(
        "checkmate::extract_fn! {{ snapshot = false, fn_sig = {signature:?}, module = \"src/lib.rs\", start = {start}, end = {end}, output = {output}, context = \"auto\" }}\n"
    )
}

const LOCAL: &str = "pub fn original(n: u8) -> u8 {\nstruct Counter { n: u8 }\nimpl Counter { fn finish(self) -> u8 { self.n + 1 } }\nlet counter = Counter { n };\ncounter.finish()\n}\n";

#[test]
fn originals_local_nominals_methods_and_manual_backend_restoration() {
    let mut c = Consumer::new();
    c.write("src/lib.rs", &format!("{LOCAL}pub struct Account {{ pub name: std::string::String }}\npub fn account(name: std::string::String) -> Account {{\nlet account = Account {{ name }};\naccount\n}}\npub fn nominal_id() -> std::any::TypeId {{\nstruct Local;\nlet _value = Local;\nstd::any::TypeId::of::<Local>()\n}}\n"));
    c.write("checkmate.toml", "[snippets.local]\ncontext = \"auto\"\nfn_sig = \"fn configured(n: u8) -> u8\"\nmodule = \"src/lib.rs\"\nstart = 4\nend = 4\noutput = \"counter.finish()\"\nsnapshot = \"snapshot.rs\"\n");
    c.write("snapshot.rs", "let counter = Counter { n };\n");
    c.write("manual.rs", "let value = n + 2;\n");
    let manual = "checkmate::extract_fn! { snapshot = false, fn_sig = \"fn manual(n: u8) -> u8\", module = \"manual.rs\", start = 1, end = 1, output = value }\n";
    c.write("tests/probe.rs", &format!(
        "{manual}{}checkmate::extract_fn!(config = \"checkmate.toml\", snippet = \"local\");\n{}{}{}{}\n#[test]\nfn runs() {{\nfor n in [0, 7, 253] {{ assert_eq!(inline(n), automatic_consumer::original(n)); assert_eq!(configured(n), inline(n)); assert_eq!(manual(n), n + 2); assert_eq!(manual_after(n), n + 2); }}\nlet _: automatic_consumer::Account = build(std::string::String::from(\"Ada\"));\nassert_eq!(build(std::string::String::from(\"Ada\")).name, \"Ada\");\nassert_eq!(local(8).finish(), 9);\nassert_ne!(std::any::Any::type_id(&nominal()), automatic_consumer::nominal_id());\n}}\n",
        automatic("fn inline(n: u8) -> u8", 4, 4, "counter.finish()"),
        automatic("fn build(name: std::string::String) -> Account", 9, 9, "account"),
        automatic("fn local(n: u8) -> Counter", 4, 4, "counter"),
        automatic("fn nominal() -> Local", 14, 14, "_value"),
        manual.replace("fn manual(", "fn manual_after(")
    ));
    c.check(&[]);

    // Real compiler tokens normalize CRLF inside multiline literals. Also
    // exercise an explicit renamed dependency and raw-identifier macro path.
    let manifest = fs::read_to_string(c.path("Cargo.toml")).unwrap();
    c.write(
        "Cargo.toml",
        &manifest.replace("checkmate = { path", "cm = { package = \"checkmate\", path"),
    );
    let caller = fs::read_to_string(c.path("tests/probe.rs")).unwrap();
    let caller = caller
        .replace("checkmate::extract_fn", "r#cm::extract_fn")
        .replace(
            "fn_sig = \"fn inline(n: u8) -> u8\"",
            "fn_sig = r#\"fn inline(\n    n: u8,\n) -> u8\"#",
        )
        .replace('\n', "\r\n");
    assert!(caller.contains("\r\n    n: u8,"));
    c.write("tests/probe.rs", &caller);
    c.check(&[]);
}

#[test]
fn semantic_failures_have_public_diagnostics() {
    let mut c = Consumer::new();
    c.write("src/lib.rs", LOCAL);
    c.write("manual.rs", "let value = 42;\n");
    c.write(
        "tests/probe.rs",
        &format!("{}\ncheckmate::extract_fn! {{ snapshot = false, fn_sig = \"fn after_error() -> i32\", module = \"manual.rs\", start = 1, end = 1, output = value }}\n", automatic("fn missing() -> u8", 4, 4, "counter.finish()")),
    );
    c.check(&["CM004", "typed fn_sig parameter"]);

    c.write(
        "src/lib.rs",
        "struct Hidden { n: u8 }\npub fn original(n: u8) {\nlet hidden = Hidden { n };\n}\n",
    );
    c.write(
        "tests/probe.rs",
        &automatic("fn private(n: u8) -> Hidden", 3, 3, "hidden"),
    );
    c.check(&["CM006", "Hidden"]);

    c.write(
        "src/lib.rs",
        "pub struct Visible { n: u8 }\npub fn original(n: u8) {\nlet visible = Visible { n };\n}\n",
    );
    c.write(
        "tests/probe.rs",
        &automatic("fn private_field(n: u8) -> Visible", 3, 3, "visible"),
    );
    c.check(&["field `n`", "is private"]);

    c.write(
        "src/lib.rs",
        &LOCAL.replace("self.n + 1", "self.n.count_ones() as u8"),
    );
    c.write(
        "tests/probe.rs",
        &automatic("fn unsupported(n: u8) -> u8", 4, 4, "counter.finish()"),
    );
    c.check(&["CM011", "count_ones"]);
}

#[test]
fn only_verified_literal_integration_test_roots_are_accepted() {
    let mut c = Consumer::new();
    c.write("src/lib.rs", LOCAL);
    let invocation = automatic("fn extracted(n: u8) -> u8", 4, 4, "counter.finish()");
    c.write("tests/probe.rs", &invocation);
    c.check(&[]);
    for caller in [
        format!("mod nested {{ {invocation} }}"),
        format!("macro_rules! wrapper {{ () => {{ {invocation} }} }}\nwrapper!();"),
        format!("use std::collections::*;\n{invocation}"),
        format!("#[derive(Clone)] struct Unrelated;\n{invocation}"),
    ] {
        c.write("tests/probe.rs", &caller);
        c.check(&["CM003"]);
    }
    // An otherwise valid Rust file is not sufficient: the source must be a
    // verified Cargo library root, not an arbitrary external/module file.
    c.write("external.rs", LOCAL);
    c.write(
        "tests/probe.rs",
        &invocation.replace("src/lib.rs", "external.rs"),
    );
    c.check(&["CM003", "source origin"]);

    c.write("tests/probe.rs", "");
    c.write(
        "src/lib.rs",
        &format!("{LOCAL}\n#[cfg(test)]\n{invocation}"),
    );
    // Establish placement before indexing original scopes: same-target use
    // fails with the actual target-identity reason, not incidental source syntax.
    c.run(&["test", "--lib"], &["CM003", "caller target identity"]);

    // Build scripts lack CARGO_BIN_NAME. A same-named test root included as a
    // module must not be accepted solely from its file/crate/package metadata.
    c.write("src/lib.rs", LOCAL);
    c.write("tests/build_script_build.rs", &invocation);
    c.write(
        "build.rs",
        "#[path = \"tests/build_script_build.rs\"] mod included;\nfn main() {}\n",
    );
    let manifest = fs::read_to_string(c.path("Cargo.toml")).unwrap();
    c.write(
        "Cargo.toml",
        &format!(
            "{manifest}\n[build-dependencies]\ncheckmate = {{ path = {:?} }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    );
    c.run(
        &["check", "--lib"],
        &["CM003", "reserved for Cargo build scripts"],
    );
}

#[test]
fn incremental_reads_track_context_config_snapshot_caller_and_manifest() {
    let mut c = Consumer::new();
    c.write("src/lib.rs", LOCAL);
    let config = "[snippets.local]\ncontext = \"auto\"\nfn_sig = \"fn extracted(n: u8) -> u8\"\nmodule = \"src/lib.rs\"\nstart = 4\nend = 4\noutput = \"counter.finish()\"\nsnapshot = \"snapshot.rs\"\n";
    let caller = "checkmate::extract_fn!(config = \"checkmate.toml\", snippet = \"local\");\n#[test]\nfn runs() { assert_eq!(extracted(7), automatic_consumer::original(7)); }\n";
    c.write("checkmate.toml", config);
    c.write("snapshot.rs", "let counter = Counter { n };\n");
    c.write("tests/probe.rs", caller);
    c.check(&[]);

    c.write("snapshot.rs", "let counter = Counter { n: 0 };\n");
    c.check(&["CM002", "snippet differs from snapshot"]);
    c.write("snapshot.rs", "let counter = Counter { n };\n");
    c.check(&[]);

    c.write(
        "checkmate.toml",
        &config.replace("fn extracted(n: u8)", "fn extracted()"),
    );
    c.check(&["CM004"]);
    c.write("checkmate.toml", config);
    c.check(&[]);

    // The selected line and snapshot never change. Only copied declarations
    // and method bodies change, so caching just the selected text is unsound.
    c.write("src/lib.rs", &LOCAL.replace("self.n + 1", "self.n + 2"));
    c.check(&[]); // Compares the generated method against the rebuilt original.
    c.write("src/lib.rs", &LOCAL.replace("n: u8 }", "n: u16 }"));
    c.check(&["mismatched types"]);
    c.write(
        "src/lib.rs",
        &LOCAL.replace("self.n + 1", "self.n.count_ones() as u8"),
    );
    c.check(&["CM011", "count_ones"]);
    c.write("src/lib.rs", LOCAL);
    c.check(&[]);

    c.write(
        "tests/probe.rs",
        &format!("use std::collections::*;\n{caller}"),
    );
    c.check(&["CM003", "glob imports"]);
    c.write("tests/probe.rs", caller);
    c.check(&[]);

    let manifest = fs::read_to_string(c.path("Cargo.toml")).unwrap();
    // SourceOrigin reads the manifest too. Removing explicit edition is valid
    // Cargo (defaults to 2015), but deliberately outside automatic's contract.
    c.write(
        "tests/probe.rs",
        &format!("extern crate core;\nextern crate checkmate;\nextern crate automatic_consumer;\n{caller}"),
    );
    c.write("Cargo.toml", &manifest.replace("edition = \"2024\"\n", ""));
    c.check(&["CM003", "explicit supported edition"]);
    c.write("Cargo.toml", &manifest);
    c.write("tests/probe.rs", caller);
    c.check(&[]);
}
