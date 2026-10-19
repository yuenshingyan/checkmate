use checkmate::extract_fn;

extract_fn!(config = "tests/fixtures/definitions.toml", snippet = "add");
extract_fn!(snippet = "tail", config = "tests/fixtures/definitions.toml");

extract_fn! {
    fn_sig = "fn with_output(a: i32, b: i32) -> i32",
    module = "tests/fixtures/snippet.rs",
    start = 4,
    end = 8,
    output = c,
    snapshot = "tests/fixtures/snapshots/add.rs",
}

extract_fn! {
    fn_sig = "fn with_tail(a: i32, b: i32) -> i32 { /* generated */ }",
    module = "tests/fixtures/snippet.rs",
    start = 4,
    end = 9,
}

extract_fn! {
    fn_sig = "pub fn with_expression(a: i32, b: i32) -> (i32, bool);",
    module = "tests/fixtures/snippet.rs",
    start = 4,
    end = 8,
    output = (c, c >= 0),
}

extract_fn! {
    fn_sig = "fn one_line(c: i32) -> i32",
    module = "tests/fixtures/snippet.rs",
    start = 9,
    end = 9,
}

extract_fn! {
    fn_sig = "fn sum(values: &[i32]) -> i32",
    module = "tests/fixtures/loop.rs",
    start = 1,
    end = 4,
    output = total,
}

#[test]
fn output_can_follow_a_loop_without_a_semicolon() {
    assert_eq!(sum(&[1, 2, 3]), 6);
    assert_eq!(sum(&[]), 0);
}

#[test]
fn extracted_functions_run_both_branches() {
    for function in [with_output, with_tail, configured_add, configured_tail] {
        assert_eq!(function(2, 3), 5);
        assert_eq!(function(-2, 3), -2);
        assert_eq!(function(0, 3), 3);
    }
}

#[test]
fn output_can_be_an_expression() {
    assert_eq!(with_expression(2, 3), (5, true));
    assert_eq!(with_expression(-2, 3), (-2, false));
}

#[test]
fn single_line_ranges_are_inclusive() {
    assert_eq!(one_line(42), 42);
}
