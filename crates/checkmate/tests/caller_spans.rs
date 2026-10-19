//! Original input spans work in rustc as well as metadata-capable editor hosts.
//! Token provenance alone must never approve forwarded, nested, or included calls.
mod support;
use support::Consumer;

const SOURCE: &str = "pub fn original(n: u8) -> u8 {\nlet result = n + 1;\nresult\n}\n";
const ARGS: &str = "snapshot = false, fn_sig = \"fn extracted(n: u8) -> u8\", module = \"src/lib.rs\", start = 2, end = 2, output = result, context = \"auto\"";

#[test]
fn argument_locations_accept_literal_calls_but_not_forwarded_or_included_tokens() {
    let c = Consumer::new();
    c.write("src/lib.rs", SOURCE);
    c.write("snippet.rs", "let result = n + 1;\n");
    c.write("checkmate.toml", "[snippets.value]\nfn_sig = \"fn configured(n: u8) -> u8\"\nmodule = \"src/lib.rs\"\nstart = 2\nend = 2\noutput = \"result\"\ncontext = \"auto\"\nsnapshot = \"snippet.rs\"\n");
    // Comments, Unicode, multiline arguments, and each delimiter form. The
    // compiler-owned input-token span, not the macro name, anchors verification.
    let calls = format!(
        "checkmate::extract_fn! {{ /* 🦀 */\n{ARGS}\n}}\ncheckmate::extract_fn![{}];\ncheckmate::extract_fn!(config = \"checkmate.toml\", snippet = \"value\");\n#[test] fn values() {{ for n in [0, 7, 253] {{ assert_eq!(extracted(n), n + 1); assert_eq!(square(n), n + 1); assert_eq!(configured(n), n + 1); }} }}\n",
        ARGS.replace("fn extracted(", "fn square(")
    );
    c.write("tests/probe.rs", &calls);
    c.test(&[]);

    // The wrapper preserves the input-token spans of its caller. It still isn't
    // a literal Checkmate invocation at the verified root.
    c.write("src/lib.rs", &format!("{SOURCE}\n#[macro_export]\nmacro_rules! forward {{ ($($args:tt)*) => {{ checkmate::extract_fn! {{ $($args)* }} }}; }}\n"));
    c.run(&["check", "--lib"], &[]);
    c.write(
        "tests/probe.rs",
        &format!("regression_consumer::forward! {{ {ARGS} }}\n"),
    );
    c.test(&["CM003", "literal fully qualified Checkmate invocations"]);
    c.write("src/lib.rs", SOURCE);
    c.write("tests/probe.rs", &format!("macro_rules! forward {{ ($($args:tt)*) => {{ checkmate::extract_fn! {{ $($args)* }} }}; }}\nforward! {{ {ARGS} }}\n"));
    c.test(&["CM003"]);

    c.write(
        "tests/fragments/body.rs",
        &format!("checkmate::extract_fn! {{ {ARGS} }}\n"),
    );
    c.write("tests/probe.rs", "include!(\"fragments/body.rs\");\n");
    c.test(&["CM003", "unique declared Cargo integration-test root"]);
    c.write(
        "tests/probe.rs",
        &format!("mod nested {{ checkmate::extract_fn! {{ {ARGS} }} }}\n"),
    );
    c.test(&["CM003", "literal crate-root macro invocations"]);

    // Restore an accepted case without cleaning the target directory.
    c.write("tests/probe.rs", &calls);
    c.test(&[]);
}
