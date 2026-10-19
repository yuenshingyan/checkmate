// This file is deliberately not imported as a Rust module.
fn original(a: i32, b: i32) -> i32 {
    // The selected statements start on line 4.
    let c = if a >= 0 {
        a + b
    } else {
        a
    };
    c
}
