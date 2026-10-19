checkmate::extract_fn! {
    fn_sig = "fn automatic_user(name: String, active: bool) -> Account",
    module = "src/lib.rs",
    start = 17,
    end = 20,
    output = user,
    snapshot = "snapshots/user.rs",
}

#[test]
fn automatically_reuses_the_original_struct_without_a_context_import() {
    let user = automatic_user("Bob".into(), false);
    assert_eq!(user.name, "Bob");
    assert!(!user.active);
    let _: checkmate_struct_example::models::User = user;
}
