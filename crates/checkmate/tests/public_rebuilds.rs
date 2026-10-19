//! Public review/rebuild lifecycle with custom Cargo targets and non-module inputs.
mod support;
use support::Consumer;

const SOURCE: &str = "pub fn original(n: u8) -> u8 {\nstruct Counter { n: u8 }\nimpl Counter { fn finish(self) -> u8 { self.n + 1 } }\nlet counter = Counter { n };\ncounter.finish()\n}\n";
const CONFIG: &str = "[snippets.automatic]\ncontext = \"auto\"\nfn_sig = \"fn extracted(n: u8) -> u8\"\nmodule = \"../source/library.rs\"\nstart = 4\nend = 4\noutput = \"counter.finish()\"\nsnapshot = \"body.rs\"\ncontext_report = \"review_data\"\n\n[snippets.ordinary]\nsnapshot = false\nfn_sig = \"fn ordinary(n: u8) -> u8\"\nmodule = \"../fixtures/body.txt\"\nstart = 1\nend = 1\noutput = \"value\"\n";
const CALLER: &str = "checkmate::extract_fn!(config = \"review/definitions.toml\", snippet = \"automatic\");\ncheckmate::extract_fn!(config = \"review/definitions.toml\", snippet = \"ordinary\");\n#[test]\nfn results() {\nlet auto: u8 = std::fs::read_to_string(\"expected_auto.txt\").unwrap().trim().parse().unwrap();\nlet explicit: u8 = std::fs::read_to_string(\"expected_ordinary.txt\").unwrap().trim().parse().unwrap();\nfor n in [0, 7, 200] { assert_eq!(extracted(n), n + auto); assert_eq!(extracted(n), regression_consumer::original(n)); assert_eq!(ordinary(n), n + explicit); }\n}\n#[test]\n#[ignore]\nfn inspect() { println!(\"BEGIN_REVIEW\\n{}END_REVIEW\", review_data()); }\n";

fn guarded(config: &str) -> String {
    config.replace(
        "context = \"auto\"",
        "context = \"auto\"\ncontext_snapshot = \"context.toml\"",
    )
}

fn inspect(c: &Consumer) -> String {
    let output = c.run(
        &["test", "--test", "probe", "--", "--ignored", "--nocapture"],
        &[],
    );
    let data = output
        .split_once("BEGIN_REVIEW\n")
        .unwrap()
        .1
        .split_once("END_REVIEW")
        .unwrap()
        .0;
    let model: toml::Value = toml::from_str(data).unwrap();
    assert_eq!(model["format"].as_str(), Some("checkmate-context"));
    assert_eq!(model["version"].as_integer(), Some(1));
    assert_eq!(model["source"]["path"].as_str(), Some("source/library.rs"));
    let counter = &model["declarations"]["source:crate/fn:original/struct:Counter"];
    assert_eq!(counter["role"].as_str(), Some("copied_struct"));
    assert!(counter["syntax"].as_str().unwrap().contains("n : u8"));
    assert_eq!(counter["implementations"].as_array().unwrap().len(), 1);
    assert!(!data.contains(c.path("").to_str().unwrap()));
    data.into()
}

#[test]
fn reviewed_context_and_external_inputs_rebuild_without_cleaning() {
    let c = Consumer::new();
    let manifest = std::fs::read_to_string(c.path("Cargo.toml"))
        .unwrap()
        .replace(
            "edition = \"2024\"",
            "edition = \"2024\"\nautotests = false",
        );
    c.write("Cargo.toml", &format!("{manifest}\n[lib]\npath = \"source/library.rs\"\n[[test]]\nname = \"probe\"\npath = \"verification/consumer.rs\"\n"));
    c.write("source/library.rs", SOURCE);
    c.write("verification/consumer.rs", CALLER);
    c.write("fixtures/body.txt", "let value = n + 2;\n");
    c.write("review/body.rs", "let counter = Counter { n };\n");
    c.write("expected_auto.txt", "1");
    c.write("expected_ordinary.txt", "2");
    c.write("review/definitions.toml", &guarded(CONFIG));
    c.run(&["build", "--lib"], &[]);
    c.test(&["CM013", "cannot read context snapshot"]);
    assert!(!c.path("review/context.toml").exists());

    // Only the outer test harness writes the reviewed data, after inspecting it.
    c.write("review/definitions.toml", CONFIG);
    c.test(&[]);
    let baseline = inspect(&c);
    assert!(!c.path("review/context.toml").exists());
    c.write("review/context.toml", &baseline);
    c.write("review/definitions.toml", &guarded(CONFIG));
    c.test(&[]);

    // Baseline-only and config-only changes, neither a Rust module dependency.
    c.write(
        "review/context.toml",
        &baseline.replace("copied_struct", "copied_enum"),
    );
    c.test(&["CM013", "changed declarations"]);
    c.write("review/context.toml", &baseline);
    c.test(&[]);
    c.write(
        "review/definitions.toml",
        &guarded(CONFIG).replace("counter.finish()", "counter.finish() + 1"),
    );
    c.test(&["CM013", "extraction syntax changed"]);
    c.write("review/definitions.toml", &guarded(CONFIG));
    c.test(&[]);

    // Expected values are read at runtime, not include_str!-tracked. If only the
    // selected source text were cached, the old function would return n + 2.
    c.write("fixtures/body.txt", "let value = n + 3;\n");
    c.write("expected_ordinary.txt", "3");
    c.test(&[]);

    // New IDs/positions in the source must not change a reviewed logical model.
    let moved = format!("pub fn unrelated(value: bool) -> bool {{ value }}\n{SOURCE}");
    let moved_config = CONFIG
        .replace("start = 4", "start = 5")
        .replace("end = 4", "end = 5");
    c.write("source/library.rs", &moved);
    c.write("review/definitions.toml", &guarded(&moved_config));
    c.test(&[]);
    assert_eq!(inspect(&c), baseline);

    // Only an unselected implementation changes; signature/body/output stay put.
    c.write(
        "source/library.rs",
        &moved.replace("self.n + 1", "self.n + 2"),
    );
    c.write("expected_auto.txt", "2");
    c.test(&["CM013", "inherent implementation content changed"]);
    c.write("review/definitions.toml", &moved_config);
    c.test(&[]);
    let updated = inspect(&c);
    assert_ne!(updated, baseline);
    let old: toml::Value = toml::from_str(&baseline).unwrap();
    let new: toml::Value = toml::from_str(&updated).unwrap();
    assert_eq!(old["extraction"], new["extraction"]);
    assert_eq!(old["source"], new["source"]);
    c.write("review/context.toml", &updated);
    c.write("review/definitions.toml", &guarded(&moved_config));
    c.test(&[]);
    c.write("review/context.toml", &baseline);
    c.test(&["CM013", "inherent implementation content changed"]);
    c.write("review/context.toml", &updated);
    c.test(&[]);

    // A normal production library build does not compile integration-test macros.
    // Actual generated-item cfg gating is separately exercised below and by the
    // native no-test compiler wrapper in context_snapshots.rs (automatic mode).
    c.write(
        "review/context.toml",
        "intentionally invalid test-only baseline",
    );
    c.run(&["build", "--lib"], &[]);
}

#[test]
fn ordinary_macro_expands_in_a_normal_build_but_its_function_is_test_only() {
    let c = Consumer::new();
    let manifest = std::fs::read_to_string(c.path("Cargo.toml"))
        .unwrap()
        .replace("[dev-dependencies]", "[dependencies]");
    c.write(
        "Cargo.toml",
        &format!("{manifest}\n[[bin]]\nname = \"driver\"\npath = \"driver/main.rs\"\n"),
    );
    c.write("fixtures/body.txt", "let value = n + 2;\n");
    let invocation = "checkmate::extract_fn! { snapshot = false, fn_sig = \"fn extracted(n: u8) -> u8\", module = \"fixtures/body.txt\", start = 1, end = 1, output = value }\n";
    c.write("driver/main.rs", &format!("{invocation}fn main() {{}}\n"));
    c.run(&["build", "--bin", "driver"], &[]);
    c.write(
        "driver/main.rs",
        &format!("{invocation}fn main() {{ let _ = extracted(1); }}\n"),
    );
    c.run(&["build", "--bin", "driver"], &["E0425", "extracted"]);
}
