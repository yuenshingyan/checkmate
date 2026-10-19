use super::*;

fn parse(text: &str) -> ParsedSource {
    ParsedSource::parse("src/example.rs", text).unwrap()
}

fn reject(source: &ParsedSource, start: usize, end: usize) -> String {
    source
        .select(start, end)
        .err()
        .expect("selection should fail")
        .to_string()
}

#[test]
fn selects_statement_sequence_and_tail_without_changing_text() {
    let text = "fn main() {\n    let a = 1;\n    let b = a + 2;\n    b\n}\n";
    let source = parse(text);
    let selection = source.select(2, 4).unwrap();
    assert_eq!(selection.statements.len(), 3);
    assert_eq!(selection.block.stmts.len(), 3);
    assert_eq!(selection.range.start.line, 2);
    assert_eq!(selection.range.end.line, 5);
    assert!(matches!(
        selection.statements.last(),
        Some(syn::Stmt::Expr(_, None))
    ));
    assert_eq!(source.text, text);
    assert_eq!(source.path, PathBuf::from("src/example.rs"));
}

#[test]
fn records_inline_module_function_and_nested_blocks() {
    let source = parse("mod model {\n fn run() {\n  if true {\n   let value = 1;\n  }\n }\n}\n");
    let selection = source.select(4, 4).unwrap();
    let kinds: Vec<_> = selection
        .enclosing
        .iter()
        .map(|scope| scope.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            EnclosingKind::File,
            EnclosingKind::Module("model".into()),
            EnclosingKind::Function("run".into()),
            EnclosingKind::Block,
            EnclosingKind::Block,
        ]
    );
    assert_eq!(selection.enclosing.last().unwrap().range.start.line, 3);
    assert_eq!(selection.statements.len(), 1);
}

#[test]
fn whole_nested_block_is_a_statement_in_its_parent() {
    let source = parse("fn f() {\n {\n  let x = 1;\n }\n}\n");
    let selection = source.select(2, 4).unwrap();
    assert_eq!(selection.enclosing.len(), 3);
    assert!(matches!(
        selection.statements[0],
        syn::Stmt::Expr(syn::Expr::Block(_), _)
    ));
}

#[test]
fn closure_and_method_bodies_keep_function_ancestry() {
    let source =
        parse("struct A;\nimpl A {\n fn f(&self) {\n  let closure = || {\n   42\n  };\n }\n}\n");
    let selection = source.select(5, 5).unwrap();
    assert!(
        selection
            .enclosing
            .iter()
            .any(|scope| scope.kind == EnclosingKind::Function("f".into()))
    );
    assert!(
        selection
            .enclosing
            .iter()
            .any(|scope| scope.kind == EnclosingKind::Closure)
    );
    assert_eq!(selection.statements.len(), 1);
}

#[test]
fn comments_blank_lines_crlf_and_missing_final_newline_are_preserved() {
    for newline in ["\n", "\r\n"] {
        let text = [
            "fn f() {",
            " // before",
            "",
            " let x = 1; // after",
            "",
            "}",
        ]
        .join(newline);
        let source = parse(&text);
        assert_eq!(source.select(2, 5).unwrap().statements.len(), 1);
        assert_eq!(source.text, text);
        assert!(reject(&source, 2, 3).contains("only comments/blank lines"));
    }
}

#[test]
fn locations_count_unicode_characters_not_utf8_bytes() {
    let source = parse("fn f() {\n    let café = \"🙂\";\n}\n");
    let selection = source.select(2, 2).unwrap();
    let syn::Stmt::Local(local) = &selection.statements[0] else {
        panic!()
    };
    let syn::Pat::Ident(pattern) = &local.pat else {
        panic!()
    };
    let name = SourceRange::of(&pattern.ident);
    assert_eq!(name.start, LineColumn { line: 2, column: 8 });
    assert_eq!(
        name.end,
        LineColumn {
            line: 2,
            column: 12
        }
    );
    let literal = SourceRange::of(local.init.as_ref().unwrap().expr.as_ref());
    assert_eq!(literal.start.column, 15);
    assert_eq!(literal.end.column, 18);
}

#[test]
fn multiple_statements_on_one_line_are_selected_together() {
    let source = parse("fn f() {\n let a = 1; let b = 2;\n}\n");
    assert_eq!(source.select(2, 2).unwrap().statements.len(), 2);
}

#[test]
fn rejects_partial_statement_headers_delimiters_and_sibling_scopes() {
    let source = parse(
        "fn f() {\n let x =\n  1;\n if true {\n  let a = 2;\n } else {\n  let b = 3;\n }\n}\n",
    );
    for (start, end) in [(1, 3), (2, 2), (3, 3), (4, 5), (5, 7), (8, 9), (1, 9)] {
        let error = reject(&source, start, end);
        assert!(error.contains(&format!("src/example.rs:{start}-{end}")));
        assert!(error.contains("select complete statements"));
    }
    assert_eq!(source.select(2, 3).unwrap().statements.len(), 1);
    assert_eq!(source.select(4, 8).unwrap().statements.len(), 1);
}

#[test]
fn rejects_ranges_crossing_functions_or_outside_source() {
    let source = parse("fn a() {\n 1;\n}\nfn b() {\n 2;\n}");
    assert!(reject(&source, 2, 5).contains("one block"));
    for (start, end) in [(0, 2), (3, 2), (2, 7), (usize::MAX, usize::MAX)] {
        assert!(reject(&source, start, end).contains("invalid inclusive line range"));
    }
    assert_eq!(source.select(5, 5).unwrap().statements.len(), 1);
}

#[test]
fn first_and_last_source_lines_cannot_smuggle_enclosing_syntax() {
    let source = parse("fn f() {\n 1\n}");
    assert!(reject(&source, 1, 1).contains("headers/delimiters"));
    assert!(reject(&source, 3, 3).contains("headers/delimiters"));
    assert_eq!(source.select(2, 2).unwrap().statements.len(), 1);
    let one_line = parse("fn f() { 1 }");
    assert!(reject(&one_line, 1, 1).contains("headers/delimiters"));
}

#[test]
fn parse_errors_name_original_file_and_location() {
    let error = ParsedSource::parse("bad.rs", "fn f() {\n let = 1;\n}")
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("bad.rs:2:"));
    assert!(error.contains("complete Rust source file"));
    assert!(
        ParsedSource::parse("empty.rs", "")
            .unwrap()
            .select(1, 1)
            .is_err()
    );
}

#[test]
fn span_intersection_is_half_open() {
    let a = SourceRange {
        start: LineColumn { line: 1, column: 0 },
        end: LineColumn { line: 1, column: 4 },
    };
    let b = SourceRange {
        start: a.end,
        end: LineColumn { line: 2, column: 0 },
    };
    assert!(!a.overlaps(b));
    assert!(a.contains(a));
}
