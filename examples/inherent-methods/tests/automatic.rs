checkmate::extract_fn!(config = "automatic.toml", snippet = "finish_counter");

#[test]
fn copied_inherent_methods_match_the_original_on_bounded_inputs() {
    for n in [0, 7, 254] {
        assert_eq!(finish_counter(n), checkmate_inherent_example::original(n));
    }
}
