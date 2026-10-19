use std::{fs, path::Path};

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::LitStr;

use crate::arguments::Invocation;
use crate::generate::{FunctionTemplate, expand};
use crate::snapshot::snapshot_difference;
use crate::source::{bounded_line, preview};

fn args(extra: TokenStream2) -> Invocation {
    syn::parse2(quote! {
        context = "explicit",
        fn_sig = "fn add_if(a: i32, b: i32) -> i32",
        module = "tests/fixtures/snippet.rs",
        // These tests isolate range/syntax/emission errors, not snapshot drift.
        snapshot = false,
        #extra
    })
    .unwrap()
}

fn expand_fixture(args: Invocation) -> syn::Result<TokenStream2> {
    expand(args, Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn expansion_error(args: Invocation) -> String {
    expand_fixture(args).unwrap_err().to_string()
}

#[test]
fn generates_valid_test_only_items() {
    let generated = expand_fixture(args(quote! { start = 4, end = 8, output = c })).unwrap();
    let file: syn::File = syn::parse2(generated).unwrap();
    assert_eq!(file.items.len(), 2);
    let syn::Item::Fn(function) = &file.items[1] else {
        panic!("expected a function")
    };
    assert_eq!(function.sig.ident, "add_if");
    assert!(function.attrs[0].meta.path().is_ident("cfg"));
    assert_eq!(function.block.stmts.len(), 2);
}

#[test]
fn rejects_invalid_ranges() {
    assert!(expansion_error(args(quote! { start = 0, end = 8 })).contains("at least 1"));
    assert!(expansion_error(args(quote! { start = 8, end = 4 })).contains("at least start"));
    assert!(expansion_error(args(quote! { start = 4, end = 999 })).contains("has only"));
}

#[test]
fn incomplete_syntax_has_source_range_numbered_preview_and_guidance() {
    let error = expansion_error(args(quote! { start = 4, end = 6 }));
    assert!(error.contains("snippet.rs:4-6"));
    assert!(error.contains("not a complete Rust"));
    assert!(error.contains("4 |     let c = if a >= 0 {"));
    assert!(error.contains("including closing delimiters"));
}

#[test]
fn rejects_missing_source() {
    let mut invocation = args(quote! { start = 1, end = 1 });
    let Invocation::Inline(args) = &mut invocation else {
        unreachable!()
    };
    args.module = LitStr::new("tests/fixtures/missing.rs", args.module.span());
    let error = expansion_error(invocation);
    assert!(error.contains("cannot read"));
    assert!(error.contains("missing.rs:1-1"));
}

#[test]
fn validates_options() {
    for input in [
        quote! { fn_sig = "fn f()" },
        quote! { unknown = 1 },
        quote! { start = 1, start = 2 },
        quote! { module = "x" start = 1 },
        quote! { config = "x" },
        quote! { snippet = "x" },
        quote! { config = "x", snippet = "y", start = 1 },
        quote! { config = "x", snippet = "y", snapshot = "z" },
        quote! { config = "x", config = "y", snippet = "z" },
    ] {
        assert!(syn::parse2::<Invocation>(input).is_err());
    }
}

#[test]
fn accepts_signature_and_placeholder_body() {
    for signature in ["fn f()", "pub fn f();", "fn f() { /* generated */ }"] {
        assert!(syn::parse_str::<FunctionTemplate>(signature).is_ok());
    }
    for signature in ["not a function", "fn f() { todo!() }", "fn f() {} junk"] {
        assert!(syn::parse_str::<FunctionTemplate>(signature).is_err());
    }
}

#[test]
fn rejects_output_after_tail_expression() {
    let error = expansion_error(args(quote! { start = 9, end = 9, output = a }));
    assert!(error.contains("cannot append output"));
    assert!(error.contains("snippet.rs:9-9"));
}

#[test]
fn snapshot_comparison_normalizes_line_endings_but_preserves_content() {
    for baseline in ["a\nb", "a\nb\n", "a\r\nb\r\n"] {
        assert!(snapshot_difference(baseline, &["a", "b"], 7).is_none());
    }
    for baseline in ["a\n b", "a\nb\n\n", "a", "a\nc"] {
        assert!(snapshot_difference(baseline, &["a", "b"], 7).is_some());
    }
    assert!(snapshot_difference("a\n", &["a", ""], 7).is_some());
    let error = snapshot_difference("a\nb", &["a", "c"], 7).unwrap();
    assert!(error.contains("source line 8 (snapshot line 2)"));
    assert!(error.contains("expected: \"b\""));
    assert!(error.contains("actual: \"c\""));
}

const CONFIG: &str = r#"
[snippets.demo]
context = "explicit"
fn_sig = "fn demo(a: i32) -> i32"
module = "source.rs"
start = 2
end = 2
output = "c"
snapshot = "snapshots/demo.rs"
"#;

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("definitions/snapshots")).unwrap();
    fs::write(
        root.path().join("definitions/source.rs"),
        "// outside\nlet c = a + 1;\n",
    )
    .unwrap();
    fs::write(root.path().join("definitions/checkmate.toml"), CONFIG).unwrap();
    fs::write(
        root.path().join("definitions/snapshots/demo.rs"),
        "let c = a + 1;\n",
    )
    .unwrap();
    root
}

fn configured(root: &Path) -> syn::Result<TokenStream2> {
    expand(
        syn::parse2(quote! {
            config = "definitions/checkmate.toml", snippet = "demo",
        })
        .unwrap(),
        root,
    )
}

#[test]
fn config_paths_are_relative_to_config_and_all_files_are_tracked() {
    let root = project();
    let generated = configured(root.path()).unwrap();
    let file: syn::File = syn::parse2(generated).unwrap();
    assert_eq!(file.items.len(), 4); // three dependencies and the function
    let tracked: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| {
            let syn::Item::Const(item) = item else {
                return None;
            };
            let syn::Expr::Macro(expr) = item.expr.as_ref() else {
                panic!("expected dependency macro")
            };
            assert!(expr.mac.path.is_ident("include_str"));
            Some(
                syn::parse2::<syn::LitStr>(expr.mac.tokens.clone())
                    .unwrap()
                    .value(),
            )
        })
        .collect();
    assert_eq!(tracked.len(), 3);
    for path in [
        "definitions/checkmate.toml",
        "definitions/source.rs",
        "definitions/snapshots/demo.rs",
    ] {
        // Compare decoded paths, not their platform-dependent Rust escaping.
        assert!(
            tracked
                .iter()
                .any(|value| Path::new(value) == root.path().join(path))
        );
    }
}

#[test]
fn inline_snapshot_uses_package_root() {
    let root = project();
    let generated = expand(
        syn::parse2(quote! {
            context = "explicit",
            fn_sig = "fn demo(a: i32) -> i32",
            module = "definitions/source.rs", start = 2, end = 2, output = c,
            snapshot = "definitions/snapshots/demo.rs",
        })
        .unwrap(),
        root.path(),
    )
    .unwrap();
    let file: syn::File = syn::parse2(generated).unwrap();
    assert_eq!(file.items.len(), 3);
}

#[test]
fn drift_fails_before_syntax_parsing_and_never_updates_snapshot() {
    let root = project();
    let path = root.path().join("definitions/snapshots/demo.rs");
    let before = fs::read(&path).unwrap();
    fs::write(
        root.path().join("definitions/source.rs"),
        "// outside\nlet c = {\n",
    )
    .unwrap();
    let error = configured(root.path()).unwrap_err().to_string();
    assert!(error.contains("[snippets.demo]"));
    assert!(error.contains("source.rs:2-2"));
    assert!(error.contains("snippet differs from snapshot"));
    assert!(error.contains("expected: \"let c = a + 1;\""));
    assert!(error.contains("actual: \"let c = {\""));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn moving_lines_requires_updating_range_but_not_snapshot() {
    let root = project();
    fs::write(
        root.path().join("definitions/source.rs"),
        "// added\n// outside\nlet c = a + 1;\n",
    )
    .unwrap();
    assert!(
        configured(root.path())
            .unwrap_err()
            .to_string()
            .contains("snippet differs")
    );
    fs::write(
        root.path().join("definitions/checkmate.toml"),
        CONFIG.replace("= 2", "= 3"),
    )
    .unwrap();
    assert!(configured(root.path()).is_ok());
}

#[test]
fn missing_snapshot_requires_explicit_creation() {
    let root = project();
    let path = root.path().join("definitions/snapshots/demo.rs");
    fs::remove_file(&path).unwrap();
    let error = configured(root.path()).unwrap_err().to_string();
    assert!(error.contains("create a reviewed snapshot"));
    assert!(!path.exists());
}

#[test]
fn config_errors_name_the_configuration() {
    let root = project();
    let path = root.path().join("definitions/checkmate.toml");
    for text in [
        "not toml".to_string(),
        CONFIG.replace("start = 2", "start = -1"),
        CONFIG.replace("start = 2", "start = \"2\""),
        CONFIG.replace("output = \"c\"", "output = \"let\""),
        CONFIG.replace("output = \"c\"", "outpt = \"c\""),
        CONFIG.replace("[snippets.demo]", "[snippets.other]"),
        CONFIG.replace("fn demo(a: i32) -> i32", "not a signature"),
        CONFIG.replace("module = \"source.rs\"", ""),
    ] {
        fs::write(&path, text).unwrap();
        assert!(
            configured(root.path())
                .unwrap_err()
                .to_string()
                .contains("checkmate.toml")
        );
    }
    fs::remove_file(&path).unwrap();
    assert!(
        configured(root.path())
            .unwrap_err()
            .to_string()
            .contains("cannot read")
    );
}

#[test]
fn config_can_omit_output_and_uses_a_reviewed_default_snapshot() {
    let root = project();
    let config = CONFIG
        .replace("output = \"c\"", "")
        .replace("snapshot = \"snapshots/demo.rs\"", "");
    fs::write(root.path().join("definitions/checkmate.toml"), config).unwrap();
    fs::write(
        root.path().join("definitions/source.rs"),
        "// outside\na + 1\n",
    )
    .unwrap();
    assert!(
        configured(root.path())
            .unwrap_err()
            .to_string()
            .contains("snippet differs")
    );
    // Explicit review in the test harness, never an automatic compiler update.
    fs::write(root.path().join("definitions/snapshots/demo.rs"), "a + 1\n").unwrap();
    assert!(configured(root.path()).is_ok());
}

#[test]
fn diagnostic_previews_are_bounded_and_unicode_safe() {
    let text = "🙂".repeat(300);
    assert_eq!(bounded_line(&text).chars().count(), 201);
    let lines = vec!["hello"; 20];
    let text = preview(&lines, 7);
    assert!(text.contains("7 | hello"));
    assert!(text.contains("18 | hello"));
    assert!(!text.contains("19 |"));
    assert!(text.contains("omitted"));
}
