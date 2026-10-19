//! Native RUSTC_WRAPPER: Cargo supplies the compiler as the first argument.
use std::{
    env,
    ffi::OsStr,
    process::{self, Command},
};

fn main() {
    let mut args = env::args_os().skip(1);
    let compiler = args.next().expect("missing compiler argument");
    let status = Command::new(compiler)
        .args(args.filter(|arg| arg != OsStr::new("--test")))
        .status()
        .expect("could not launch compiler");
    process::exit(status.code().unwrap_or(1));
}
