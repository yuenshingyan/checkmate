//! Public failure boundaries through native Cargo, with one reused target per group.
mod support;
use support::Consumer;

fn automatic(signature: &str, line: usize, output: &str) -> String {
    format!(
        "checkmate::extract_fn! {{ fn_sig = {signature:?}, module = \"src/lib.rs\", start = {line}, end = {line}, output = {output}, context = \"auto\" }}\n"
    )
}

fn valid_source(c: &Consumer, source: &str) {
    c.write("src/lib.rs", source);
    // Do not mistake an invalid original library for a public macro rejection.
    c.run(&["check", "--lib"], &[]);
}

#[test]
fn copied_nominals_are_neither_each_other_nor_the_original() {
    let c = Consumer::new();
    valid_source(
        &c,
        "pub fn original(n: u8) -> impl std::any::Any {\nstruct Local { n: u8 }\nlet value = Local { n };\nvalue\n}\npub fn value(n: u8) -> u8 { n }\n",
    );
    let calls = format!(
        "{}{}",
        automatic("fn first(n: u8) -> Local", 3, "value"),
        automatic("fn second(n: u8) -> Local", 3, "value")
    );
    c.write("tests/probe.rs", &format!("{calls}\n#[test] fn identities() {{\nlet a = first(7); let b = second(7); let original = regression_consumer::original(7);\nassert_eq!(a.n, regression_consumer::value(7)); assert_eq!(b.n, a.n);\nassert_ne!(std::any::Any::type_id(&a), std::any::Any::type_id(&b));\nassert_ne!(std::any::Any::type_id(&a), std::any::Any::type_id(&original));\n}}\n"));
    c.test(&[]);
    // Infer the unnameable copied type instead of depending on generated names.
    for other in ["second(7)", "regression_consumer::original(7)"] {
        c.write("tests/probe.rs", &format!("{calls}\n#[test] fn incompatible() {{ let mut value = first(7); value = {other}; }}\n"));
        let report = c.test(&["E0308", "mismatched types"]);
        assert!(!report.contains("Checkmate [CM"), "{report}");
    }
}

#[test]
fn original_routes_and_private_members_are_not_widened() {
    let c = Consumer::new();
    // Current conservative policy rejects the explicitly written private route,
    // even though a different public reexport route exists for the same type.
    valid_source(
        &c,
        "mod hidden { pub struct Visible { pub n: u8 } }\npub use hidden::Visible;\npub fn original() {\nlet value = hidden::Visible { n: 7 };\n}\n",
    );
    c.write("tests/probe.rs", &automatic("fn extracted()", 4, "()"));
    c.test(&["CM006", "hidden"]);

    // Original associated methods are terminals for planning, but rustc still
    // enforces their privacy (unlike private methods on copied local types).
    valid_source(
        &c,
        "pub struct Visible { pub n: u8 }\nimpl Visible { fn secret() -> Self { Self { n: 7 } } }\npub fn original() {\nlet value = Visible::secret();\n}\n",
    );
    c.write(
        "tests/probe.rs",
        &automatic("fn extracted() -> Visible", 4, "value"),
    );
    let report = c.test(&["E0624", "secret", "private"]);
    assert!(!report.contains("Checkmate [CM"), "{report}");
}

#[test]
fn planner_failures_keep_categories_subjects_and_emit_no_partial_api() {
    let c = Consumer::new();
    c.write("manual.rs", "let value = 1u8;\n");
    // Source-valid cases beyond the simple parameter/private-item failures
    // in automatic.rs. In particular an UNUSED method must not be dropped.
    let cases = [
        (
            "pub fn original() {\nlet value = line!();\n}\n",
            "fn extracted() -> u32",
            2,
            "value",
            "CM007",
            "line",
        ),
        (
            "pub fn original() {\nstruct Local { n: u8 }\nimpl Local { fn view(&self) -> u8 { self.n } }\nlet value = Local { n: 7 };\n}\n",
            "fn extracted() -> u8",
            4,
            "value.view()",
            "CM011",
            "view",
        ),
        (
            "pub fn original() {\nlet n = 9u8;\n{ let n = 3u8;\nlet value = n;\n}\n}\n",
            "fn extracted() -> u8",
            4,
            "value",
            "CM004",
            "subject: n (Local)",
        ),
        (
            "pub fn original() {\nstruct Local<T> { n: T }\nlet value = Local { n: 7u8 };\n}\n",
            "fn extracted() -> Local<u8>",
            3,
            "value",
            "CM009",
            "subject: Local (Struct)",
        ),
        (
            "pub fn original() {\n#[derive(Clone)] struct Local;\nlet value = Local;\n}\n",
            "fn extracted() -> Local",
            3,
            "value",
            "CM007",
            "derive",
        ),
        (
            "#[cfg(all())]\npub fn original() {\nlet value = 7u8;\n}\n",
            "fn extracted() -> u8",
            3,
            "value",
            "CM008",
            "cfg",
        ),
        (
            "struct Hidden;\npub fn original() {\nstruct Leaf { value: Hidden }\ntype Root = Leaf;\nlet value: Option<Root> = None;\n}\n",
            "fn extracted() -> Option<Root>",
            5,
            "value",
            "CM006",
            "subject: Hidden (UnitStruct)",
        ),
        (
            "pub fn original() {\nstruct Local { n: u8 }\nimpl Local { fn unused(self) -> u32 { self.n.count_ones() } }\nlet value = Local { n: 7 };\n}\n",
            "fn extracted() -> Local",
            4,
            "value",
            "CM011",
            "subject: count_ones",
        ),
        (
            "pub const VALUE: u8 = 7;\npub fn original() {\nlet value = VALUE;\n}\n",
            "fn extracted(VALUE: u8) -> u8",
            3,
            "value",
            "CM010",
            "shadow original declaration `VALUE`",
        ),
    ];
    for (source, signature, line, output, code, subject) in cases {
        valid_source(&c, source);
        let invocation = automatic(signature, line, output).replace(
            "context = \"auto\"",
            "context = \"auto\", context_report = \"REVIEW\"",
        );
        // Occupy both API names: partial emission would produce E0428.
        // rustc suppresses missing-name secondary errors after compile_error!,
        // but duplicate definitions are diagnosed during expansion. Ordinary
        // macro backend restoration after failure is covered in automatic.rs.
        let fallback = ["extracted", "REVIEW"].map(|name| format!(
            "checkmate::extract_fn! {{ fn_sig = \"fn {name}() -> u8\", module = \"manual.rs\", start = 1, end = 1, output = value }}\n"
        )).join("");
        c.write("tests/probe.rs", &format!("{invocation}{fallback}"));
        let report = c.test(&[code, subject]);
        assert!(!report.contains("E0428"), "partial API emitted: {report}");
        assert!(report.contains("src/lib.rs"), "{report}");
        if subject == "subject: Hidden (UnitStruct)" {
            let chain = report
                .split_once("dependency chain:")
                .expect("transitive diagnostic chain")
                .1;
            assert!(
                chain.contains("-> Root") && chain.contains("-> Leaf"),
                "{report}"
            );
        }
        assert!(
            !report.contains("format = \"checkmate-context\""),
            "{report}"
        );
        assert!(!c.path("context.toml").exists());
    }
    // Positive control for the no-partial-API oracle: duplicate macro-generated
    // functions really are diagnosed, including in the presence of compile_error!.
    let collision = automatic("fn extracted() -> u8", 3, "value");
    let failure = automatic("fn rejected(VALUE: u8) -> u8", 3, "value");
    c.write(
        "tests/probe.rs",
        &format!("{collision}{collision}{failure}"),
    );
    c.test(&["E0428", "extracted", "CM010"]);
}
