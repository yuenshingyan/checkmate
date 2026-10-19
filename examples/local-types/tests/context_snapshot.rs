checkmate::extract_fn!(config = "context-snapshots.toml", snippet = "snapshot_user");

#[test]
fn reviewed_context_still_returns_the_expected_fields() {
    let user = snapshot_user("Bob".into(), false);
    assert_eq!(user.name, "Bob");
    assert!(!user.active);
}

// Explicit, read-only inspection. Copy/review the TOML yourself; this test does
// not create or update any baseline. Temporarily omit the context_snapshot option
// to inspect a new model when an existing baseline intentionally needs updating.
#[test]
#[ignore]
fn print_context_for_review() {
    println!("--- CONTEXT BEGIN ---\n{}--- CONTEXT END ---", user_context());
}
