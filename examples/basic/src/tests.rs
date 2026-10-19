#![cfg(test)]

use checkmate::extract_fn;

extract_fn!(config = "checkmate.toml", snippet = "add_if");

#[test]
fn test_add_success() {
    assert_eq!(add_if(2.0, 2.0), 4.0);
}

#[test]
fn test_negative_input_is_unchanged() {
    assert_eq!(add_if(-2.0, 10.0), -2.0);
}

#[test]
fn test_zero_takes_addition_branch() {
    assert_eq!(add_if(0.0, 3.0), 3.0);
}
