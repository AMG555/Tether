//! Binary entry point. Thin on purpose: parse, dispatch, and exit with a real status code.
//!
//! Exit codes (documented so scripts can rely on them):
//!   0  success
//!   1  runtime error (wrong key, corrupt database, I/O)
//!   2  refused (destructive command without confirmation)
//!   3  bad usage

use clap::Parser;

fn main() {
    let cli = tether::cli::Cli::parse();
    match tether::cli::run(cli) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("tether: {e}");
            std::process::exit(1);
        }
    }
}
