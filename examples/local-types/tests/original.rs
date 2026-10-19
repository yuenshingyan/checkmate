#[test]
fn original_function_keeps_its_existing_behavior() {
    assert_eq!(
        checkmate_local_example::original("Alice".into(), true),
        ("Alice".into(), true)
    );
}
