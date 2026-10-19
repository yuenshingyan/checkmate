//! Default automatic mode through real Cargo; explicit mode never acts as fallback.
mod support;
use support::Consumer;

const SOURCE: &str = "pub struct Account { pub n: u8 }\npub fn original(n: u8) -> Account {\nlet account = Account { n };\naccount\n}\n";
const BODY: &str = "let account = Account { n };\n";

fn configured(context: &str, guard: &str) -> String {
    format!(
        "[snippets.named]\nfn_sig = \"fn configured(n: u8) -> Account\"\nmodule = \"../src/lib.rs\"\nstart = 3\nend = 3\noutput = \"account\"\ncontext_report = \"configured_context\"\n{context}\n{guard}\n"
    )
}
fn caller(context: &str, guard: &str) -> String {
    format!(
        r#"
checkmate::extract_fn! {{ fn_sig = "fn extracted(n: u8) -> Account", module = "src/lib.rs", start = 3, end = 3, output = account, context_report = "inline_context", {context} {guard} }}
checkmate::extract_fn!(config = "review/definitions.toml", snippet = "named");
#[test] fn values() {{
    for n in [0, 7, 255] {{
        let a: regression_consumer::Account = extracted(n);
        let b: regression_consumer::Account = configured(n);
        assert_eq!(a.n, n); assert_eq!(b.n, n);
    }}
}}
#[test] fn inspect() {{
    println!("BEGIN_INLINE\n{{}}END_INLINE", inline_context());
    println!("BEGIN_CONFIG\n{{}}END_CONFIG", configured_context());
}}
"#
    )
}

#[test]
fn omitted_context_discovers_original_identity_with_default_and_context_guards() {
    let c = Consumer::new();
    c.write("src/lib.rs", SOURCE);
    // Both context and snapshot are omitted: no import supplies Account.
    c.write("snapshots/extracted.rs", BODY);
    c.write("review/snapshots/configured.rs", BODY);
    c.write("review/definitions.toml", &configured("", ""));
    c.write("tests/probe.rs", &caller("", ""));
    let report = c.run(&["test", "--test", "probe", "--", "--nocapture"], &[]);
    for (label, path) in [
        ("INLINE", "review/inline.context.toml"),
        ("CONFIG", "review/configured.context.toml"),
    ] {
        assert!(!c.path(path).exists());
        let model = report
            .split(&format!("BEGIN_{label}\n"))
            .nth(1)
            .unwrap()
            .split(&format!("END_{label}"))
            .next()
            .unwrap();
        let data: toml::Value = toml::from_str(model).unwrap();
        assert_eq!(data["format"].as_str(), Some("checkmate-context"));
        assert_eq!(
            data["source"]["library"].as_str(),
            Some("regression_consumer")
        );
        assert_eq!(
            data["declarations"]["source:crate/struct:Account"]["role"].as_str(),
            Some("original")
        );
        assert!(!model.contains(c.path("").to_str().unwrap()));
        c.write(path, model); // Explicit outer-harness inspection/approval, never compilation.
    }
    let inline_guard = "context_snapshot = \"review/inline.context.toml\",";
    let config_guard = "context_snapshot = \"configured.context.toml\"";
    c.write("review/definitions.toml", &configured("", config_guard));
    c.write("tests/probe.rs", &caller("", inline_guard));
    c.test(&[]);
    // Explicit auto is the same mode and must accept the very same baselines.
    c.write(
        "review/definitions.toml",
        &configured("context = \"auto\"", config_guard),
    );
    c.write(
        "tests/probe.rs",
        &caller("context = \"auto\",", inline_guard),
    );
    c.test(&[]);
    c.write("review/definitions.toml", &configured("", config_guard));
    c.write("tests/probe.rs", &caller("", inline_guard));
    c.write("review/inline.context.toml", "invalid baseline");
    c.test(&["CM013", "invalid context snapshot TOML"]);
}

#[test]
fn omitted_context_never_falls_back_for_unproven_sources_or_placements() {
    let c = Consumer::new();
    let source = "pub fn original(n: u8) -> u8 {\nlet value = n + 1;\nvalue\n}\n";
    c.write("src/lib.rs", source);
    c.write("external.rs", source);
    let args = "fn_sig = \"fn extracted(n: u8) -> u8\", module = \"src/lib.rs\", start = 2, end = 2, output = value, snapshot = false";
    let check = "#[test] fn value() { assert_eq!(extracted(7), 8); }";
    for text in [
        format!("mod nested {{ checkmate::extract_fn! {{ {args} }} {check} }}"),
        format!("use checkmate::extract_fn; extract_fn! {{ {args} }} {check}"),
        format!(
            "checkmate::extract_fn! {{ {} }} {check}",
            args.replace("src/lib.rs", "external.rs")
        ),
    ] {
        c.write("tests/probe.rs", &text);
        c.test(&["CM003"]);
        // The same selection and assertions work only after explicit opt-out.
        c.write(
            "tests/probe.rs",
            &text.replace("fn_sig =", "context = \"explicit\", fn_sig ="),
        );
        c.test(&[]);
    }
    c.write(
        "tests/probe.rs",
        &format!(
            "checkmate::extract_fn! {{ {} }}",
            args.replace("fn extracted(n: u8)", "fn extracted()")
        ),
    );
    c.test(&["CM004", "required input `n`"]);
}

#[test]
fn explicit_mode_rejects_context_options_in_both_forms() {
    let c = Consumer::new();
    c.write("src/lib.rs", SOURCE);
    for option in [
        "context_report = \"review\"",
        "context_snapshot = \"missing.context.toml\"",
    ] {
        c.write("tests/probe.rs", &format!("checkmate::extract_fn! {{ context = \"explicit\", snapshot = false, fn_sig = \"fn ordinary(n: u8) -> Account\", module = \"src/lib.rs\", start = 3, end = 3, output = account, {option} }}"));
        c.test(&["requires context = \"auto\""]);
        c.write("review/definitions.toml", &format!("[snippets.named]\ncontext = \"explicit\"\nsnapshot = false\nfn_sig = \"fn ordinary(n: u8) -> Account\"\nmodule = \"../src/lib.rs\"\nstart = 3\nend = 3\noutput = \"account\"\n{option}\n"));
        c.write(
            "tests/probe.rs",
            "checkmate::extract_fn!(config = \"review/definitions.toml\", snippet = \"named\");",
        );
        c.test(&["requires context = \"auto\""]);
        assert!(!c.path("review/missing.context.toml").exists());
    }
}
