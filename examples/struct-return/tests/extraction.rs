use checkmate::extract_fn;
// Still explicit today: automatic context discovery is not publicly enabled.
use checkmate_struct_example::models::User as Account;

extract_fn!(config = "checkmate.toml", snippet = "build_user");
extract_fn!(config = "checkmate.toml", snippet = "user_fields");
extract_fn!(config = "checkmate.toml", snippet = "count_values");

#[test]
fn extracted_function_returns_the_original_struct_type() {
    let user: checkmate_struct_example::models::User = build_user("Bob".into(), false);
    assert_eq!(user.name, "Bob");
    assert!(!user.active);
}

#[test]
fn tuple_output_tests_fields_without_returning_the_struct() {
    assert_eq!(user_fields("Carol".into(), true), ("Carol".into(), true));
}

#[test]
fn original_containing_function_keeps_its_own_inputs() {
    let user = checkmate_struct_example::demo();
    assert_eq!(user.name, "Alice");
    assert!(user.active);
}

#[test]
fn ordinary_extraction_leaves_method_resolution_to_rust() {
    assert_eq!(count_values(&[1, 2, 3]), 3);
    assert_eq!(count_values(&[]), 0);
    assert_eq!(checkmate_struct_example::count(&[1, 2, 3]), 3);
}
