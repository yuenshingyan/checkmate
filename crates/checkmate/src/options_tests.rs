use std::path::Path;

use proc_macro2::TokenStream;
use quote::quote;

use super::{Context, Invocation};
use crate::config::Resolved;

fn inline(extra: TokenStream) -> syn::Result<Invocation> {
    syn::parse2(quote! {
        fn_sig = "fn demo() -> i32", module = "source.rs",
        start = 2, end = 3, output = answer, snapshot = "demo.rs",
        #extra
    })
}

fn configured(extra: &str) -> syn::Result<Resolved> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("options.toml"),
        format!(
            r#"[snippets.actual_key]
fn_sig = "fn demo() -> i32"
module = "source.rs"
start = 2
end = 3
output = "answer"
snapshot = "demo.rs"
{extra}
"#
        ),
    )
    .unwrap();
    syn::parse2::<Invocation>(quote! {
        config = "options.toml", snippet = "actual_key"
    })
    .unwrap()
    .resolve(root.path())
}

#[test]
fn inline_and_toml_context_parity_and_snippet_identity() {
    assert_eq!(Context::default(), Context::Explicit);
    for (inline_option, toml_option, expected) in [
        (quote! {}, "", Context::Explicit),
        (
            quote! { context = "auto" },
            "context = \"auto\"",
            Context::Auto,
        ),
    ] {
        let inline = inline(inline_option)
            .unwrap()
            .resolve(Path::new("."))
            .unwrap();
        let configured = configured(toml_option).unwrap();
        assert_eq!(inline.args.context, expected);
        assert_eq!(configured.args.context, expected);
        assert_eq!(inline.snippet, "fn demo() -> i32");
        assert_eq!(configured.snippet, "actual_key");
        assert_eq!(inline.args.fn_sig.value(), configured.args.fn_sig.value());
        assert_eq!(inline.args.module.value(), configured.args.module.value());
        assert_eq!(
            inline.args.start.to_string(),
            configured.args.start.to_string()
        );
        assert_eq!(inline.args.end.to_string(), configured.args.end.to_string());
        let inline_output = inline.args.output.unwrap();
        let config_output = configured.args.output.unwrap();
        assert_eq!(
            quote!(#inline_output).to_string(),
            quote!(#config_output).to_string()
        );
        assert_eq!(
            inline.args.snapshot.unwrap().value(),
            configured.args.snapshot.unwrap().value()
        );
        assert!(inline.args.context_snapshot.is_none());
        assert!(inline.args.context_report.is_none());
        assert!(configured.args.context_snapshot.is_none());
        assert!(configured.args.context_report.is_none());
        assert!(inline.config.is_none());
        assert!(inline.context.is_empty());
        assert!(configured.config.is_some());
        assert!(configured.context.contains("[snippets.actual_key]"));
    }
}

#[test]
fn context_baseline_options_have_inline_toml_parity() {
    for report in ["BASELINE", "r#type", "_baseline", "基準"] {
        let inline = inline(quote! {
            context = "auto", context_snapshot = "../baselines/demo.json",
            context_report = #report
        })
        .unwrap()
        .resolve(Path::new("inline-root"))
        .unwrap();
        let configured = configured(&format!(
            "context = \"auto\"\ncontext_snapshot = \"../baselines/demo.json\"\ncontext_report = {report:?}"
        ))
        .unwrap();
        for args in [&inline.args, &configured.args] {
            assert_eq!(args.context, Context::Auto);
            assert_eq!(
                args.context_snapshot.as_ref().unwrap().value(),
                "../baselines/demo.json"
            );
            assert_eq!(args.context_report.as_ref().unwrap().value(), report);
            assert_eq!(args.snapshot.as_ref().unwrap().value(), "demo.rs");
        }
        assert_eq!(inline.root, Path::new("inline-root"));
        assert_eq!(
            configured.root,
            configured.config.as_ref().unwrap().parent().unwrap()
        );
    }
    // Each option is independently optional.
    for name in ["context_snapshot", "context_report"] {
        let key = syn::Ident::new(name, proc_macro2::Span::call_site());
        assert!(inline(quote! { context = "auto", #key = "BASELINE" }).is_ok());
        assert!(configured(&format!("context = \"auto\"\n{name} = \"BASELINE\"")).is_ok());
    }
}

#[test]
fn context_baseline_options_require_auto() {
    for name in ["context_snapshot", "context_report"] {
        let key = syn::Ident::new(name, proc_macro2::Span::call_site());
        let expected = format!("{name} requires context = \"auto\"");
        assert!(
            inline(quote! { #key = "BASELINE" })
                .err()
                .unwrap()
                .to_string()
                .contains(&expected)
        );
        let error = configured(&format!("{name} = \"BASELINE\""))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(&expected));
        assert!(error.contains("[snippets.actual_key]"));
    }
}

#[test]
fn context_baseline_options_reject_nonstrings_duplicates_and_mixing() {
    for name in ["context_snapshot", "context_report"] {
        let key = syn::Ident::new(name, proc_macro2::Span::call_site());
        for value in [
            quote!(true),
            quote!(1),
            quote!(1.5),
            quote!([]),
            quote!({}),
            quote!(BASELINE),
        ] {
            assert!(inline(quote! { context = "auto", #key = #value }).is_err());
        }
        for value in ["true", "1", "1.5", "[]", "{}", "1979-05-27"] {
            assert!(configured(&format!("context = \"auto\"\n{name} = {value}")).is_err());
        }
        assert!(
            inline(quote! { context = "auto", #key = "BASELINE", #key = "BASELINE" })
                .err()
                .unwrap()
                .to_string()
                .contains(&format!("duplicate option `{name}`"))
        );
        assert!(
            configured(&format!(
                "context = \"auto\"\n{name} = \"BASELINE\"\n{name} = \"BASELINE\""
            ))
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate")
        );
        for input in [
            quote! { config = "options.toml", snippet = "actual_key", #key = "BASELINE" },
            quote! { #key = "BASELINE", config = "options.toml", snippet = "actual_key" },
            quote! { config = "options.toml", #key = "BASELINE" },
            quote! { snippet = "actual_key", #key = "BASELINE" },
        ] {
            assert!(
                syn::parse2::<Invocation>(input)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("cannot be mixed with inline options")
            );
        }
    }
}

#[test]
fn context_report_requires_one_identifier_in_both_forms() {
    for value in [
        "",
        "_",
        " _ ",
        "r#_",
        "type",
        "self",
        "r#self",
        "foo::bar",
        "foo bar",
        "foo-bar",
        "123",
        "report.json",
        "REPORT;",
        "const REPORT: &str = x",
    ] {
        assert!(
            inline(quote! { context = "auto", context_report = #value })
                .err()
                .unwrap()
                .to_string()
                .contains("context_report must be one Rust identifier"),
            "{value:?}"
        );
        let error = configured(&format!("context = \"auto\"\ncontext_report = {value:?}"))
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("context_report must be one Rust identifier"),
            "{value:?}"
        );
        assert!(error.contains("[snippets.actual_key]"));
    }
}

#[test]
fn toml_context_options_validate_selected_definition() {
    // Like context/output, semantic validation applies only to the selected definition.
    let other =
        "\n[snippets.other]\nfn_sig = \"fn other()\"\nmodule = \"other.rs\"\nstart = 1\nend = 2\n";
    for options in [
        "context_snapshot = \"baseline.json\"",
        "context_report = \"BASELINE\"",
        "context = \"auto\"\ncontext_report = \"not an identifier\"",
    ] {
        assert!(configured(&format!("{other}{options}")).is_ok());
    }
    // Deserialization still checks types, unknown fields, and duplicates everywhere.
    for options in [
        "context_report = 1",
        "context_snapshot = false",
        "unknown = 1",
        "context_report = \"A\"\ncontext_report = \"B\"",
    ] {
        assert!(configured(&format!("{other}{options}")).is_err());
    }
}

#[test]
fn invalid_context_strings_fail_in_both_forms() {
    for value in ["", "explicit", "Auto", "AUTO", " auto", "auto ", "unknown"] {
        let error = inline(quote! { context = #value }).err().unwrap();
        assert!(error.to_string().contains("invalid context"));
        let error = configured(&format!("context = {value:?}")).err().unwrap();
        assert!(error.to_string().contains("invalid context"));
    }
}

#[test]
fn nonstring_context_fails_in_both_forms() {
    for value in [
        quote!(true),
        quote!(1),
        quote!(1.5),
        quote!(['a']),
        quote!({}),
        quote!(auto),
    ] {
        assert!(inline(quote! { context = #value }).is_err());
    }
    for value in ["true", "1", "1.5", "[]", "{}", "1979-05-27"] {
        assert!(configured(&format!("context = {value}")).is_err());
    }
}

#[test]
fn duplicate_context_fails_in_both_forms() {
    let error = inline(quote! { context = "auto", context = "auto" })
        .err()
        .unwrap();
    assert!(error.to_string().contains("duplicate option `context`"));
    let error = configured("context = \"auto\"\ncontext = \"auto\"")
        .err()
        .unwrap();
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn unknown_options_still_fail() {
    for extra in [
        quote! { unknown = "auto" },
        quote! { context = "auto", unknown = 1 },
    ] {
        assert!(
            inline(extra)
                .err()
                .unwrap()
                .to_string()
                .contains("unknown option")
        );
    }
    for extra in ["unknown = 1", "context = \"auto\"\nunknown = 1"] {
        assert!(
            configured(extra)
                .err()
                .unwrap()
                .to_string()
                .contains("unknown field")
        );
    }
}

#[test]
fn config_or_snippet_cannot_mix_with_inline_context() {
    for input in [
        quote! { config = "options.toml", snippet = "actual_key", context = "auto" },
        quote! { context = "auto", config = "options.toml", snippet = "actual_key" },
        quote! { config = "options.toml", context = "auto" },
        quote! { snippet = "actual_key", context = "auto" },
    ] {
        let error = syn::parse2::<Invocation>(input).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("cannot be mixed with inline options")
        );
    }
}
