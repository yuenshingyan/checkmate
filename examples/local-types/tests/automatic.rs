/* café 🙂 */ checkmate::extract_fn!(config = "automatic.toml", snippet = "build_user");

#[test]
fn copied_local_struct_exposes_fields_without_manual_context() {
    let user = build_user("Bob".into(), false);
    assert_eq!(user.name, "Bob");
    assert!(!user.active);
    assert_eq!((user.name, user.active), checkmate_local_example::original("Bob".into(), false));
}
