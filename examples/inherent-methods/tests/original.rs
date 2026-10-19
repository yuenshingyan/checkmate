#[test]
fn original_behavior_is_unchanged() {
    for n in [0, 7, 254] {
        assert_eq!(checkmate_inherent_example::original(n), n + 1);
    }
}
