use super::Invocation;
use crate::config::Resolved;
use proc_macro2::TokenStream;
use quote::quote;
use std::{fs, path::Path};

fn inline(root: &Path, signature: &str, options: TokenStream) -> syn::Result<Resolved> {
    syn::parse2::<Invocation>(quote! {
        fn_sig = #signature, module = "source.rs", start = 1, end = 1,
        #options
    })?
    .resolve(root)
}

fn configured(root: &Path, signature: &str, options: &str) -> syn::Result<Resolved> {
    fs::create_dir_all(root.join("definitions")).unwrap();
    let signature = toml::Value::String(signature.into());
    fs::write(root.join("definitions/options.toml"), format!(
        "[snippets.different_key]\nfn_sig = {signature}\nmodule = \"source.rs\"\nstart = 1\nend = 1\n{options}\n"
    )).unwrap();
    syn::parse2::<Invocation>(quote! {
        config = "definitions/options.toml", snippet = "different_key"
    })?
    .resolve(root)
}

#[test]
fn omitted_snapshot_uses_the_parsed_function_name_in_both_modes_and_path_bases() {
    let root = tempfile::tempdir().unwrap();
    for (signature, expected) in [
        ("fn extracted()", "snapshots/extracted.rs"),
        ("pub fn extracted() -> i32;", "snapshots/extracted.rs"),
        (
            "fn extracted() { /* placeholder */ }",
            "snapshots/extracted.rs",
        ),
        ("fn r#type()", "snapshots/type.rs"),
        ("fn 算出()", "snapshots/算出.rs"),
    ] {
        for (tokens, text) in [
            (quote! {}, ""),
            (quote! { context = "auto" }, "context = \"auto\""),
            (quote! { context = "explicit" }, "context = \"explicit\""),
        ] {
            let direct = inline(root.path(), signature, tokens).unwrap();
            let named = configured(root.path(), signature, text).unwrap();
            assert_eq!(direct.root, root.path());
            assert_eq!(named.root, root.path().join("definitions"));
            assert_eq!(named.snippet, "different_key");
            for resolved in [direct, named] {
                assert!(!resolved.args.snapshot_disabled);
                assert_eq!(resolved.args.snapshot.unwrap().value(), expected);
                assert!(resolved.args.context_snapshot.is_none());
                assert!(!resolved.root.join(expected).exists());
                assert!(!resolved.root.join("snapshots").exists());
            }
        }
    }
}

#[test]
fn explicit_path_and_false_override_defaults_in_both_forms() {
    let root = tempfile::tempdir().unwrap();
    for context in [false, true] {
        let (tokens, text) = if context {
            (quote! { context = "auto", }, "context = \"auto\"\n")
        } else {
            (quote! { context = "explicit", }, "context = \"explicit\"\n")
        };
        for resolved in [
            inline(
                root.path(),
                "fn demo()",
                quote! { #tokens snapshot = "../custom.rs" },
            )
            .unwrap(),
            configured(
                root.path(),
                "fn demo()",
                &format!("{text}snapshot = \"../custom.rs\""),
            )
            .unwrap(),
        ] {
            assert_eq!(resolved.args.snapshot.unwrap().value(), "../custom.rs");
            assert!(!resolved.args.snapshot_disabled);
        }
        for resolved in [
            inline(
                root.path(),
                "fn demo()",
                quote! { #tokens snapshot = false },
            )
            .unwrap(),
            configured(root.path(), "fn demo()", &format!("{text}snapshot = false")).unwrap(),
        ] {
            assert!(resolved.args.snapshot.is_none());
            assert!(resolved.args.snapshot_disabled);
        }
    }
}

#[test]
fn snapshot_false_does_not_disable_or_default_context_baselines() {
    let root = tempfile::tempdir().unwrap();
    for resolved in [
        inline(
            root.path(),
            "fn demo()",
            quote! {
                context = "auto", snapshot = false, context_snapshot = "context.toml"
            },
        )
        .unwrap(),
        configured(
            root.path(),
            "fn demo()",
            "context = \"auto\"\nsnapshot = false\ncontext_snapshot = \"context.toml\"",
        )
        .unwrap(),
    ] {
        assert!(resolved.args.snapshot.is_none());
        assert_eq!(
            resolved.args.context_snapshot.unwrap().value(),
            "context.toml"
        );
    }
}

#[test]
fn invalid_snapshot_values_duplicates_and_mixed_config_options_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for (tokens, text) in [
        (quote! { true }, "true"),
        (quote! { "" }, "\"\""),
        (quote! { 42 }, "42"),
        (quote! { [] }, "[]"),
        (quote! { {} }, "{}"),
    ] {
        assert!(inline(root.path(), "fn demo()", quote! { snapshot = #tokens }).is_err());
        let error = configured(root.path(), "fn demo()", &format!("snapshot = {text}"))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("options.toml"), "{error}");
    }
    for tokens in [
        quote! { snapshot = false, snapshot = false },
        quote! { snapshot = false, snapshot = "custom.rs" },
        quote! { snapshot = "custom.rs", snapshot = false },
    ] {
        let error = inline(root.path(), "fn demo()", tokens)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("duplicate option `snapshot`"), "{error}");
    }
    assert!(
        configured(
            root.path(),
            "fn demo()",
            "snapshot = false\nsnapshot = false"
        )
        .is_err()
    );
    let error = syn::parse2::<Invocation>(quote! {
        config = "definitions/options.toml", snippet = "different_key", snapshot = false
    })
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("cannot be mixed"), "{error}");
}

#[test]
fn deriving_default_paths_requires_a_valid_signature_and_never_creates_files() {
    let root = tempfile::tempdir().unwrap();
    for signature in [
        "not a signature",
        "fn ../escape()",
        "fn f() { panic!() }",
        "fn f() {} junk",
    ] {
        let error = inline(root.path(), signature, quote! {})
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("invalid fn_sig"), "{error}");
        let error = configured(root.path(), signature, "")
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("invalid fn_sig") && error.contains("[snippets.different_key]"),
            "{error}"
        );
    }
    assert!(!root.path().join("snapshots").exists());
    assert!(!root.path().join("definitions/snapshots").exists());
}
