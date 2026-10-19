mod tests;

fn main() {
    // Code snippet to be verified.
    let a = 1.0;
    let b = 2.0;
    let c = if a >= 0.0 {
        a + b
    } else {
        a
    };

    println!("{c}");
}
