use super::*;
use proc_macro2::LineColumn;
use syn::spanned::Spanned;

fn request() -> Request<'static> {
    Request {
        snippet: "example",
        path: Path::new("src/example.rs"),
        start: 2,
        end: 4,
    }
}

fn range(start: usize, end: usize) -> SourceRange {
    SourceRange {
        start: LineColumn {
            line: start,
            column: 0,
        },
        end: LineColumn {
            line: end,
            column: 1,
        },
    }
}

fn error() -> Error {
    Error::new(Span::call_site(), "the original message")
}

fn subject(name: &str) -> Subject {
    Subject {
        name: name.into(),
        kind: Some(DeclarationKind::Function),
    }
}

#[test]
fn small_error_and_metadata_preserving_context() {
    assert_eq!(std::mem::size_of::<Error>(), std::mem::size_of::<usize>());
    let original = syn::parse_str::<syn::Expr>("a +").err().unwrap();
    let span = original.span();
    let error = Error::from(original)
        .with_site(Origin::Source, range(2, 3))
        .with_subject("thing", Some(DeclarationKind::Struct))
        .with_chain(vec![subject("dependency")], 7)
        .context("replacement message");
    assert_eq!(error.kind, Kind::Syntax);
    assert_eq!(error.span().start(), span.start());
    assert_eq!(error.span().end(), span.end());
    assert_eq!(error.site.unwrap().range, range(2, 3));
    assert_eq!(error.subject.as_ref().unwrap().name, "thing");
    assert_eq!(error.chain, vec![subject("dependency")]);
    assert_eq!(error.omitted_chain, 7);
    assert_eq!(error.to_string(), "replacement message");
    assert_eq!(self::error().kind, Kind::UnsupportedContext);
}

#[test]
fn every_kind_has_a_distinct_stable_code_and_remedy() {
    let kinds = [
        Kind::Syntax,
        Kind::Selection,
        Kind::SourceIdentity,
        Kind::MissingInput,
        Kind::UnknownItem,
        Kind::InaccessibleItem,
        Kind::Macro,
        Kind::Conditional,
        Kind::UnsafePromotion,
        Kind::UnsupportedContext,
        Kind::MethodResolution,
        Kind::InternalInvariant,
    ];
    let mut remedies = std::collections::HashSet::new();
    for (index, kind) in kinds.into_iter().enumerate() {
        let output = error().with_kind(kind).render(&request(), "", None);
        assert!(output.contains(&format!("Checkmate [CM{:03}] {kind:?}:", index + 1)));
        let remedy = output
            .lines()
            .find(|line| line.starts_with("help:"))
            .unwrap();
        assert!(remedies.insert(remedy.to_owned()));
        assert!(output.contains("do not remap rustc type, borrow, or privacy errors"));
        assert!(!remedy.contains("pub"));
        assert!(!remedy.contains("annotation"));
        assert!(!remedy.contains("guess"));
    }
}

#[test]
fn request_identity_selection_and_three_line_preview() {
    let output = error().render(&request(), "zero\none\ntwo\nthree\nfour\nfive", None);
    assert!(output.contains("snippet: example"));
    assert!(output.contains("source file: src/example.rs"));
    assert!(output.contains("source selection: lines 2..=4"));
    assert!(output.contains("2 | one\n  3 | two\n  4 | three"));
    assert!(!output.contains("| zero"));
    assert!(!output.contains("| four"));
    let extended = Request {
        end: 100,
        ..request()
    };
    let output = error().render(&extended, "zero\none\ntwo\nthree\nfour", None);
    assert_eq!(
        output.lines().filter(|line| line.contains(" | ")).count(),
        3
    );
    assert!(output.contains("additional lines omitted"));
}

#[test]
fn source_sites_override_selection_and_ignore_generated_text() {
    let output = error().with_site(Origin::Source, range(1, 1)).render(
        &request(),
        "SOURCE_ONLY\nnot selected",
        Some("GENERATED_ONLY"),
    );
    assert!(output.contains("source location: 1:0..1:1"));
    assert!(output.contains("1 | SOURCE_ONLY"));
    assert!(!output.contains("GENERATED_ONLY"));
    assert!(!output.contains("| not selected"));
}

#[test]
fn generated_positions_never_reuse_source_preview() {
    for (origin, label) in [
        (Origin::Signature, "generated signature"),
        (Origin::Output, "generated output"),
    ] {
        let error = error().with_site(origin, range(1, 1));
        let output = error.render(&request(), "SOURCE_SECRET", Some("GENERATED_ONLY"));
        assert!(output.contains(&format!("{label} location: 1:0..1:1")));
        assert!(output.contains("1 | GENERATED_ONLY"));
        assert!(!output.contains("SOURCE_SECRET"));
        let absent = error.render(&request(), "SOURCE_SECRET", None);
        assert!(absent.contains(&format!("{label} preview unavailable")));
        assert!(!absent.contains("SOURCE_SECRET"));
    }
    let fallback = error().render(&request(), "first\nSOURCE_ONLY", Some("GENERATED_ONLY"));
    assert!(fallback.contains("SOURCE_ONLY"));
    assert!(!fallback.contains("GENERATED_ONLY"));
}

#[test]
fn exclusive_end_at_column_zero_does_not_preview_next_line() {
    let site = SourceRange {
        start: LineColumn { line: 1, column: 0 },
        end: LineColumn { line: 2, column: 0 },
    };
    let output =
        error()
            .with_site(Origin::Source, site)
            .render(&request(), "included\nEXCLUDED", None);
    assert!(output.contains("1 | included"));
    assert!(!output.contains("EXCLUDED"));
}

#[test]
fn unicode_survives_and_long_preview_lines_stay_bounded() {
    let text = format!("α🦀漢字{}", "🦀".repeat(1000));
    let request = Request {
        start: 1,
        end: 1,
        snippet: "日本語🦀",
        ..request()
    };
    let output = error().render(&request, &text, None);
    assert!(output.contains("日本語🦀"));
    assert!(output.contains("α🦀漢字"));
    let preview = output
        .lines()
        .find_map(|line| line.split_once(" | ").map(|(_, text)| text))
        .unwrap();
    assert!(preview.chars().count() <= 160);
    assert!(preview.ends_with(TRUNCATED));
    assert!(output.len() <= MAX_BYTES);
}

#[test]
fn malicious_controls_are_escaped_in_every_user_field() {
    let hostile = "bad\n\r\t\0\u{1b}[31m\u{202e}\u{2066}\u{061c}\u{200f}\u{2028}";
    let request = Request {
        snippet: hostile,
        path: Path::new(hostile),
        start: 1,
        end: 1,
    };
    let error = Error::new(Span::call_site(), hostile)
        .with_subject(hostile, None)
        .with_chain(vec![subject(hostile)], 0);
    let output = error.render(&request, "\t\u{1b}\u{202e}\u{2069}", None);
    assert!(output.contains("\\n\\r\\t\\u{0}"));
    assert!(output.contains("\\u{202e}"));
    assert!(output.chars().all(|c| c == '\n' || !unsafe_character(c)));
    let preview = output
        .lines()
        .find_map(|line| line.split_once(" | ").map(|(_, text)| text))
        .unwrap();
    assert!(preview.chars().count() <= 160);
}

#[test]
fn huge_fields_and_chains_are_bounded_and_preserve_both_ends() {
    let huge = "🦀\u{202e}".repeat(20_000);
    let request = Request {
        snippet: &huge,
        path: Path::new(&huge),
        start: 1,
        end: usize::MAX,
    };
    let mut chain = vec![subject(&huge); 2000];
    chain[0] = subject("FIRST");
    chain[1999] = subject("LAST");
    let error = Error::new(Span::call_site(), &huge)
        .with_subject(&huge, Some(DeclarationKind::Local))
        .with_chain(chain, 9);
    let source = format!("{huge}\n{huge}\n{huge}\n{huge}");
    let output = error.render(&request, &source, None);
    assert!(output.len() <= 8192, "{} bytes", output.len());
    assert!(output.contains("FIRST"));
    assert!(output.contains("LAST"));
    assert!(output.contains("2003 dependencies omitted"));
    assert!(output.contains(TRUNCATED));
    assert_eq!(
        output.lines().filter(|line| line.contains(" | ")).count(),
        3
    );
    assert_eq!(output, error.render(&request, &source, None));
}

#[test]
fn extreme_ranges_and_omission_counts_do_not_overflow() {
    for (start, end) in [(0, 0), (3, 1), (usize::MAX, usize::MAX), (1, usize::MAX)] {
        let request = Request {
            start,
            end,
            ..request()
        };
        let output = error().render(&request, "one\ntwo\nthree\nfour", None);
        assert!(output.len() <= MAX_BYTES);
        let site = SourceRange {
            start: LineColumn {
                line: start,
                column: usize::MAX,
            },
            end: LineColumn {
                line: end,
                column: usize::MAX,
            },
        };
        let output = error()
            .with_site(Origin::Signature, site)
            .with_chain(vec![subject("x"); 20], usize::MAX)
            .render(&request, "", Some("generated"));
        assert!(output.contains(&format!("{} dependencies omitted", usize::MAX)));
        assert!(output.len() <= MAX_BYTES);
    }
}

#[test]
fn global_budget_handles_utf8_and_stops_after_truncation() {
    let mut out = Bounded::new(MAX_BYTES);
    let _ = out.write_str(&"🦀".repeat(MAX_BYTES));
    let _ = out.write_str("must not appear");
    let output = out.finish();
    assert!(output.len() <= MAX_BYTES);
    assert!(output.ends_with(TRUNCATED));
    assert!(!output.contains("must not appear"));
}

#[test]
fn compile_error_conversion_uses_invocation_span_and_rendered_message() {
    let invocation: syn::Expr = syn::parse_str("\n\ninvoke()").unwrap();
    let invocation = invocation.span();
    let expected = error().render(&request(), "first\nsecond", None);
    let converted = error().into_syn_error(invocation, &request(), "first\nsecond", None);
    assert_eq!(converted.to_string(), expected);
    assert_eq!(converted.span().start(), invocation.start());
    assert_eq!(converted.span().end(), invocation.end());
    let tokens = converted.to_compile_error();
    let parsed: syn::ExprMacro = syn::parse2(tokens).unwrap();
    assert_eq!(
        parsed.mac.path.segments.last().unwrap().ident,
        "compile_error"
    );
    let message: syn::LitStr = syn::parse2(parsed.mac.tokens).unwrap();
    assert_eq!(message.value(), expected);
    assert_eq!(message.span().start(), invocation.start());
}
