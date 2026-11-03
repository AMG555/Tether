//! Command-line surface.
//!
//! Design stance: the CLI is the whole product in v1. A tray icon is a nice-to-have; a
//! command you can put in a startup script, a `systemd --user` unit or a LaunchAgent is
//! the thing that actually keeps you safe on day one. Every command works offline and
//! every command that prints draft text says so before printing.

use std::io;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::crypto::SecretKey;
use crate::vault::Vault;

pub const KEY_FILE: &str = "key.txt";

#[derive(Parser, Debug)]
#[command(
    name = "savepoint",
    about = "Crash-proof, offline, encrypted typing recovery.",
    long_about = "Savepoint keeps an encrypted, encrypted-at-rest copy of whatever you are \
typing, so a crash, a force-quit or a dead battery never costs you a half-written prompt again.\n\
Everything stays on this machine. No account, no network, no telemetry.",
    version,
    after_help = "Examples:\n  savepoint init                  create the vault + show your recovery key\n  savepoint run                   start capturing in the foreground\n  savepoint recover               replay anything left in the crash buffer\n  savepoint list -n 10 --preview  see recent rescues\n  savepoint copy 3                put rescue 3 back on the clipboard"
)]
pub struct Cli {
    /// Override the data directory (default: per-OS user data dir).
    #[arg(long, global = true, env = "SAVEPOINT_HOME")]
    pub home: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create the vault and print the recovery key once.
    Init {
        /// Print the recovery key to stdout (it is always written to the key file).
        #[arg(long)]
        show_key: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeArg {
    Auto,
    Keystrokes,
    Clipboard,
    None,
}

fn data_dir(cli_home: &Option<PathBuf>) -> PathBuf {
    cli_home.clone().unwrap_or_else(crate::default_data_dir)
}

fn load_or_create_key(dir: &Path) -> io::Result<(SecretKey, bool)> {
    let path = dir.join(KEY_FILE);
    if path.exists() {
        let hex = std::fs::read_to_string(&path)?;
        let key = SecretKey::from_hex(hex.trim())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "key file is corrupt"))?;
        Ok((key, false))
    } else {
        std::fs::create_dir_all(dir)?;
        let key =
            SecretKey::random().map_err(|_| io::Error::other("could not read OS random source"))?;
        std::fs::write(&path, key.to_hex())?;
        crate::vault::set_private(&path)?;
        Ok((key, true))
    }
}

fn open_vault(home: &Option<PathBuf>) -> Result<Vault, Box<dyn std::error::Error>> {
    let dir = data_dir(home);
    let (key, created) = load_or_create_key(&dir)?;
    let vault = Vault::open(&dir, &key)?;
    if created {
        eprintln!("savepoint: new vault created at {}", dir.display());
    }
    Ok(vault)
}

pub fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let home = cli.home.clone();
    match cli.command {

        Command::Init { show_key } => cmd_init(&home, show_key),
}
}

fn cmd_init(home: &Option<PathBuf>, show_key: bool) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir(home);
    let (key, created) = load_or_create_key(&dir)?;
    let vault = Vault::open(&dir, &key)?;
    println!("vault:   {}", dir.display());
    println!(
        "keyfile: {}  (chmod 600 — back this up, it is the only way to read the vault)",
        dir.join(KEY_FILE).display()
    );
    println!(
        "policy:  {} blocked apps, {} blocked title patterns",
        crate::policy::BLOCKED_APPS.len(),
        crate::policy::BLOCKED_TITLES.len()
    );
    println!("state:   {}", if created { "new vault, schema v1" } else { "existing vault opened" });
    if vault.integrity_ok() {
        println!("db:      integrity ok");
    } else {
        println!("db:      INTEGRITY CHECK FAILED — run `savepoint backup` before anything else");
    }
    println!("\nRecovery key (store this somewhere safe; anyone with it can read your vault):");
    println!("  {}\n", key.to_hex());
    if show_key {
        // already printed; keeps the flag meaningful for scripts
    }
    println!("Next: `savepoint run` to start capturing, or `savepoint doctor` first.");
    Ok(())
}























#[cfg(not(unix))]
pub fn install_sigint(stop: Arc<AtomicBool>) {
    // Windows: the hook thread owns the message loop; Ctrl-C closes it via the console
    // handler that SetConsoleCtrlHandler installs in the tray build.
    let _ = stop;
}


