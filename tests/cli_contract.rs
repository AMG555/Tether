//! The CLI is the whole product surface, so its contract is tested, not assumed.
//!
//! This exists because a size optimisation (`clap` with `default-features = false`) silently
//! deleted the `--help` flag. The binary got smaller, every test still passed, and the tool
//! could not explain itself. A footprint win that removes the manual is not a win.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_savepoint"))
}

#[test]
fn help_is_available_and_lists_the_commands() {
    let out = bin().arg("--help").output().expect("run --help");
    let text =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "--help exited {:?}: {text}", out.status.code());
    for cmd in ["init", "run", "recover", "list", "ask"] {
        assert!(text.contains(cmd), "--help does not mention `{cmd}`");
    }
    assert!(!text.contains("unexpected argument"), "--help must not be an error");
}

#[test]
fn every_subcommand_can_describe_itself() {
    for cmd in ["init", "run", "recover", "list", "ask", "simulate"] {
        let out = bin().args([cmd, "--help"]).output().expect("run sub --help");
        assert!(out.status.success(), "`{cmd} --help` failed");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("Usage:"), "`{cmd} --help` printed no usage");
    }
}

#[test]
fn the_companion_flags_exist() {
    let out = bin().args(["ask", "--help"]).output().expect("run");
    let text =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    for flag in ["--new", "--list", "--install", "--check", "--studio", "--character-studio"] {
        assert!(text.contains(flag), "`ask` no longer documents {flag}");
    }
}
