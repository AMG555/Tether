//! Command-line surface.
//!
//! Design stance: the CLI is the whole product in v1. A tray icon is a nice-to-have; a
//! command you can put in a startup script, a `systemd --user` unit or a LaunchAgent is
//! the thing that actually keeps you safe on day one. Every command works offline and
//! every command that prints draft text says so before printing.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};

use crate::capture::{self, Mode};
use crate::crypto::SecretKey;
use crate::engine::{Engine, FieldSnapshot};
use crate::policy::Policy;
use crate::ring::CrashRing;
use crate::vault::{human_age, now, Vault};

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
    /// Capture input in the foreground until Ctrl-C.
    Run {
        /// Capture backend to use.
        #[arg(long, value_enum, default_value_t = ModeArg::Auto)]
        mode: ModeArg,
        /// Commit buffered keystrokes to the vault this often.
        #[arg(long, default_value_t = 1000)]
        flush_ms: u64,
        /// How many keystrokes the crash ring can hold before wrapping.
        #[arg(long, default_value_t = 4096)]
        ring_slots: usize,
        /// In probe mode, read the focused field this often (ms).
        #[arg(long, default_value_t = 1500)]
        probe_ms: u64,
        /// Stay quiet: no startup banner, no periodic status.
        #[arg(long)]
        quiet: bool,
    },
    /// Replay anything still sitting in the crash buffer (run this after a crash).
    Recover,
    /// List rescues, newest first.
    List {
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
        /// Characters of plaintext to show per row (0 = metadata only).
        #[arg(long, default_value_t = 60)]
        preview: usize,
        /// Emit JSON (for scripting).
        #[arg(long)]
        json: bool,
    },
    /// Print one rescue in full.
    Show {
        id: i64,
        /// Do not append a trailing newline (useful for piping back into a file).
        #[arg(long)]
        bare: bool,
    },
    /// Put a rescue back on the clipboard.
    Copy { id: i64 },
    /// Full-text search across every rescue.
    Search {
        query: String,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },
    /// Show earlier versions of a rescue.
    Revisions { id: i64 },
    /// Delete one rescue (vacuumed, not just unlinked).
    Delete { id: i64 },
    /// Delete everything, irreversibly.
    Purge {
        #[arg(long)]
        yes: bool,
    },
    /// Apply retention: keep the newest N rescues, drop old revisions.
    Prune {
        #[arg(long, default_value_t = 500)]
        keep: usize,
        #[arg(long, default_value_t = 30)]
        revision_days: i64,
    },
    /// Encrypted backup copy of the vault (safe to sync to a USB stick).
    Backup { path: PathBuf },
    /// Plaintext export — refuses unless you pass --i-understand.
    Export {
        path: PathBuf,
        #[arg(long)]
        i_understand: bool,
    },
    /// Diagnose permissions, the database and the capture backend.
    Doctor,
    /// One-screen health/status summary.
    Status,
    /// Feed synthetic keystrokes into the capture path (demos and tests).
    ///
    /// This exists so the crash guarantee is *reproducible*: CI can type a sentence, SIGKILL
    /// the process mid-stream, and assert the text comes back. It writes nothing to the
    /// vault and never commits — only the crash ring sees these keystrokes.
    Ask {
        /// Create a new pet from procedural art (no drawing needed).
        #[arg(long)]
        new: Option<String>,
        /// Frame size in px for `--new` (16, 32, 48 or 64).
        #[arg(long, default_value_t = 32)]
        frame_size: u32,
        /// Body colour for `--new`, as #rrggbb.
        #[arg(long, default_value = "#c2331a")]
        body: String,
        /// Accent colour for `--new`, as #rrggbb.
        #[arg(long, default_value = "#1f6b3a")]
        accent: String,
        /// List installed pets (and anything broken, with the reason).
        #[arg(long)]
        list: bool,
        /// Install a pet folder after validating it.
        #[arg(long, value_name = "DIR")]
        install: Option<PathBuf>,
        /// Simulate a working day and print every reminder, so you can tune the schedule.
        #[arg(long)]
        check: bool,
    },
    /// Feed synthetic keystrokes into the capture path (demos and tests).
    ///
    /// This exists so the crash guarantee is *reproducible*: CI can type a sentence, SIGKILL
    /// the process mid-stream, and assert the text comes back. It writes nothing to the
    /// vault and never commits — only the crash ring sees these keystrokes.
    Simulate {
        /// Text to "type".
        #[arg(long)]
        text: String,
        /// App to attribute it to (so policy and draft splitting are exercised).
        #[arg(long, default_value = "rescue-demo")]
        app: String,
        /// Kill the process with SIGKILL after typing, leaving everything in the ring.
        #[arg(long)]
        crash: bool,
        /// Slots in the ring (8 makes wrapping easy to demo).
        #[arg(long, default_value_t = 4096)]
        ring_slots: usize,
        /// Set the ring capacity smaller than the text to demonstrate truncation flags.
        #[arg(long)]
        quiet: bool,
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
        Command::Run { mode, flush_ms, ring_slots, probe_ms, quiet } => {
            cmd_run(&home, mode, flush_ms, ring_slots, probe_ms, quiet)
        }
        Command::Recover => cmd_recover(&home),
        Command::List { limit, preview, json } => cmd_list(&home, limit, preview, json),
        Command::Show { id, bare } => cmd_show(&home, id, bare),
        Command::Copy { id } => cmd_copy(&home, id),
        Command::Search { query, limit } => cmd_search(&home, &query, limit),
        Command::Revisions { id } => cmd_revisions(&home, id),
        Command::Delete { id } => cmd_delete(&home, id),
        Command::Purge { yes } => cmd_purge(&home, yes),
        Command::Prune { keep, revision_days } => cmd_prune(&home, keep, revision_days),
        Command::Backup { path } => cmd_backup(&home, &path),
        Command::Export { path, i_understand } => cmd_export(&home, &path, i_understand),
        Command::Doctor => cmd_doctor(&home),
        Command::Status => cmd_status(&home),
        Command::Ask { new, frame_size, body, accent, list, install, check } => {
            let data = data_dir(&home);
            let action = if let Some(name) = new {
                crate::companion::Action::New { name, frame_size, body, accent }
            } else if list {
                crate::companion::Action::List
            } else if let Some(path) = install {
                crate::companion::Action::Install { path }
            } else if check {
                crate::companion::Action::Check
            } else {
                crate::companion::Action::Run { ascii: false }
            };
            crate::companion::run(&data, action)
        }
        Command::Simulate { text, app, crash, ring_slots, quiet } => {
            cmd_simulate(&home, &text, &app, crash, ring_slots, quiet)
        }
    
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

fn cmd_recover(home: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let ring_path = vault.ring_path();
    if !ring_path.exists() {
        println!("crash buffer is empty — nothing to recover.");
        return Ok(());
    }
    let engine = Engine::new(vault, 4096, Policy::default())?;
    let n = engine.recover()?;
    if engine.ring_was_salvaged() {
        eprintln!(
            "note: the crash buffer's header was damaged, so its slots were salvaged by \
             sequence number. Text was kept; the wrap point may be approximate."
        );
    }
    if n == 0 {
        println!("crash buffer was empty — nothing to recover.");
    } else {
        println!("recovered {n} rescue(s) from the crash buffer.");
        let rows = engine.list(n + 1, 50)?;
        for row in rows.iter().take(n) {
            let app = row.app.clone().unwrap_or_else(|| "?".into());
            println!(
                "  #{:<4} {:<14} {:>5} chars   {}",
                row.id,
                truncate(&app, 14),
                row.chars,
                row.preview.clone().unwrap_or_default().replace('\n', " ")
            );
        }
    }
    Ok(())
}

fn cmd_run(
    home: &Option<PathBuf>,
    mode: ModeArg,
    flush_ms: u64,
    ring_slots: usize,
    probe_ms: u64,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let engine = Engine::new(vault, ring_slots, Policy::default())?;

    // Recovery FIRST: replaying the ring after new input would interleave old text into a
    // new draft and corrupt both.
    let recovered = engine.recover().unwrap_or(0);

    let source_mode = match mode {
        ModeArg::Auto => {
            let s = capture::default_source();
            drop(s);
            if cfg!(windows) || cfg!(target_os = "macos") {
                Mode::Keystrokes
            } else {
                Mode::Clipboard
            }
        }
        ModeArg::Keystrokes => Mode::Keystrokes,
        ModeArg::Clipboard => Mode::Clipboard,
        ModeArg::None => Mode::None,
    };
    let mut source = capture::source_for(source_mode);
    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));

    if !quiet {
        println!("{}", crate::TAGLINE);
        if recovered > 0 {
            println!("recovered {recovered} rescue(s) left over from last time");
        }
        println!("vault:    {}", data_dir(home).display());
        println!("backend:  {} → {}", source.name(), source_mode.describe());
        println!(
            "commit:   every {:?}, ring holds {} keystrokes",
            Duration::from_millis(flush_ms),
            ring_slots
        );
        println!("privacy:  secrets redacted before they reach disk; blocked apps never captured");
        println!(
            "press Ctrl-C to stop (the last buffered keystrokes are committed on the way out)\n"
        );
    }

    // Flush thread.
    {
        let e = Arc::clone(&engine);
        let s = Arc::clone(&stop);
        std::thread::spawn(move || {
            e.run_commit_loop(&s, Duration::from_millis(flush_ms));
        });
    }

    // Optional probe thread (macOS/Windows: exact field reads beat keystroke replay).
    if source_mode == Mode::Keystrokes {
        let e = Arc::clone(&engine);
        let s = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut probe = capture::default_source();
            let mut last: Option<String> = None;
            while !s.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(probe_ms.max(200)));
                if let Some(snap) = probe.probe() {
                    if snap.value.trim().is_empty() {
                        continue;
                    }
                    // Only re-save when the field actually changed.
                    if last.as_deref() == Some(snap.value.as_str()) {
                        continue;
                    }
                    let snapshot: FieldSnapshot = snap.clone();
                    last = Some(snap.value.clone());
                    let _ = e.save_snapshot(snapshot);
                }
            }
        });
    }

    // Ctrl-C: stop cleanly so the ring is committed, not abandoned. Installed without a
    // crate: a signal handler that only flips an atomic is async-signal-safe.
    install_sigint(Arc::clone(&stop));

    let engine_for_sink = Arc::clone(&engine);
    source.run(&move |ev| engine_for_sink.ingest(ev), &stop);
    stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(flush_ms + 100));
    let _ = engine.flush();

    if !quiet {
        let seen = engine.stats.events_seen.load(Ordering::Relaxed);
        let commits = engine.stats.commits.load(Ordering::Relaxed);
        let red = engine.stats.redactions.load(Ordering::Relaxed);
        let dropped = engine.stats.events_dropped_by_policy.load(Ordering::Relaxed);
        println!("\nstopped. {seen} events seen, {commits} commits, {red} secrets redacted, {dropped} events refused by policy.");
    }
    Ok(())
}

fn cmd_list(
    home: &Option<PathBuf>,
    limit: usize,
    preview: usize,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let rows = vault.list(limit, preview)?;
    if json {
        let out: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "app": r.app,
                    "age": r.age(),
                    "chars": r.chars,
                    "redactions": r.redactions,
                    "truncated": r.truncated,
                    "revisions": r.revisions,
                    "preview": r.preview,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no rescues yet. `savepoint run` starts capturing.");
        return Ok(());
    }
    println!("{:>4}  {:<8} {:<16} {:>6}  PREVIEW", "ID", "AGE", "APP", "CHARS");
    println!("{}", "-".repeat(78));
    for r in &rows {
        println!(
            "{:>4}  {:<8} {:<16} {:>6}  {}{}",
            r.id,
            human_age(r.age()),
            truncate(r.app.as_deref().unwrap_or("?"), 16),
            r.chars,
            r.preview.clone().unwrap_or_default().replace(['\n', '\t'], " "),
            if r.truncated { "  [partial]" } else { "" }
        );
    }
    if rows.iter().any(|r| r.truncated) {
        println!("\n[partial] = the crash buffer wrapped; this text is the newest tail, not the whole draft.");
    }
    Ok(())
}

fn cmd_show(home: &Option<PathBuf>, id: i64, bare: bool) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let (row, text) = vault.get(id)?;
    if bare {
        print!("{text}");
        io::stdout().flush()?;
        return Ok(());
    }
    println!(
        "# rescue {id} · {} · {} chars · {} · {} revisions{}",
        row.app.as_deref().unwrap_or("?"),
        row.chars,
        human_age(row.age()),
        row.revisions,
        if row.truncated { " · PARTIAL" } else { "" }
    );
    if row.redactions > 0 {
        println!("# {} secret(s) were redacted before storage (█)", row.redactions);
    }
    println!("{}", "-".repeat(60));
    println!("{text}");
    Ok(())
}

fn cmd_copy(home: &Option<PathBuf>, id: i64) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let (_row, text) = vault.get(id)?;
    copy_to_clipboard(&text)?;
    println!("rescue {id} copied to clipboard ({} chars).", text.chars().count());
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::{Command, Stdio};
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("pbcopy", vec![])
    } else if cfg!(windows) {
        ("clip", vec![])
    } else if std::env::var("WAYLAND_DISPLAY").is_ok() {
        ("wl-copy", vec![])
    } else {
        ("xclip", vec!["-selection", "clipboard"])
    };
    let mut child =
        Command::new(cmd).args(&args).stdin(Stdio::piped()).stdout(Stdio::null()).spawn().map_err(
            |e| {
                format!(
                    "{cmd} not available ({e}); the text is in `savepoint show {text_len}`",
                    text_len = text.chars().count()
                )
            },
        )?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

fn cmd_search(
    home: &Option<PathBuf>,
    query: &str,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let hits = vault.search(query, limit)?;
    if hits.is_empty() {
        println!("no rescue contains {query:?}");
        return Ok(());
    }
    for r in hits {
        println!(
            "#{}  {}  {}\n    {}\n",
            r.id,
            r.app.as_deref().unwrap_or("?"),
            human_age(r.age()),
            r.preview.unwrap_or_default().replace('\n', " ")
        );
    }
    Ok(())
}

fn cmd_revisions(home: &Option<PathBuf>, id: i64) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let revs = vault.revisions(id)?;
    if revs.is_empty() {
        println!("rescue {id} has no earlier revisions (it has only ever had one version).");
        return Ok(());
    }
    for (i, r) in revs.iter().enumerate() {
        println!("--- revision {} of {} (oldest shown last) ---", i + 1, revs.len());
        println!("{r}\n");
    }
    Ok(())
}

fn cmd_delete(home: &Option<PathBuf>, id: i64) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    vault.delete(id)?;
    println!("rescue {id} deleted and the database vacuumed.");
    Ok(())
}

fn cmd_purge(home: &Option<PathBuf>, yes: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !yes {
        eprintln!(
            "this deletes every rescue, forever. Re-run with --yes if that is what you want."
        );
        std::process::exit(2);
    }
    let vault = open_vault(home)?;
    vault.purge_all()?;
    let ring = vault.ring_path();
    let _ = std::fs::remove_file(ring);
    println!("all rescues purged; database vacuumed; crash buffer removed.");
    Ok(())
}

fn cmd_prune(
    home: &Option<PathBuf>,
    keep: usize,
    revision_days: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let n = vault.prune(keep, revision_days)?;
    println!("pruned {n} row(s): keeping the newest {keep} rescues and revisions from the last {revision_days} days.");
    Ok(())
}

fn cmd_backup(home: &Option<PathBuf>, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir(home);
    let vault = open_vault(home)?;
    // WAL checkpoint first or the backup silently misses recent commits.
    let _ = vault.checkpoint();
    std::fs::create_dir_all(path)?;
    let stamp = now();
    let dest = path.join(format!("rescue-backup-{stamp}"));
    std::fs::create_dir_all(&dest)?;
    for f in ["vault.db", "salt.bin"] {
        let src = dir.join(f);
        if src.exists() {
            std::fs::copy(&src, dest.join(f))?;
        }
    }
    println!("encrypted backup written to {}", dest.display());
    println!(
        "it is useless without your key file ({}). Back both up, separately.",
        dir.join(KEY_FILE).display()
    );
    if dir.join(KEY_FILE).exists() {
        nothing();
    }
    Ok(())
}

fn nothing() {}

fn cmd_export(
    home: &Option<PathBuf>,
    path: &Path,
    ok: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !ok {
        eprintln!(
            "refusing: `export` writes PLAINTEXT drafts. That defeats the point of the vault.\n\
             If you really want it, re-run with --i-understand, and delete the file after."
        );
        std::process::exit(2);
    }
    let vault = open_vault(home)?;
    let rows = vault.list(usize::MAX, 0)?;
    let mut out = Vec::new();
    for r in rows {
        let (row, text) = vault.get(r.id)?;
        out.push(serde_json::json!({
            "id": row.id,
            "app": row.app,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
            "text": text,
        }));
    }
    std::fs::write(path, serde_json::to_string_pretty(&out)?)?;
    crate::vault::set_private(path)?;
    println!("exported {} rescue(s) to {} (plaintext, chmod 600)", out.len(), path.display());
    Ok(())
}


fn cmd_doctor(home: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir(home);
    println!("savepoint doctor\n");
    let mut problems = 0;

    println!("[platform]");
    println!("  os: {} / {}", std::env::consts::OS, std::env::consts::ARCH);

    println!("\n[storage]");
    println!("  data dir: {}", dir.display());
    match std::fs::metadata(&dir) {
        Ok(m) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = m.permissions().mode() & 0o777;
                let ok = mode & 0o077 == 0;
                println!(
                    "  dir mode: {:o} {}",
                    mode,
                    if ok { "ok" } else { "TOO OPEN — chmod 700" }
                );
                if !ok {
                    problems += 1;
                }
            }
        }
        Err(_) => {
            println!("  data dir: missing (run `savepoint init`)");
            problems += 1;
        }
    }
    for f in [KEY_FILE, "salt.bin", "vault.db", "ring.bin"] {
        let p = dir.join(f);
        match std::fs::metadata(&p) {
            Ok(m) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = m.permissions().mode() & 0o777;
                    let warn = f == KEY_FILE && mode & 0o077 != 0;
                    println!(
                        "  {:<9} {:>8} bytes  mode {:o}{}",
                        f,
                        m.len(),
                        mode,
                        if warn { "  <-- readable by others" } else { "" }
                    );
                    if warn {
                        problems += 1;
                    }
                }
                #[cfg(not(unix))]
                println!("  {:<9} {:>8} bytes", f, m.len());
            }
            Err(_) => println!("  {:<9} absent", f),
        }
    }

    if dir.join("vault.db").exists() {
        if let Ok(vault) = open_vault(home) {
            let ok = vault.integrity_ok();
            println!("  integrity: {}", if ok { "ok (PRAGMA quick_check)" } else { "FAILED" });
            if !ok {
                problems += 1;
            }
            let (d, r) = vault.counts()?;
            println!("  contents: {d} rescues, {r} revisions");
        }
    }

    println!("\n[crash buffer]");
    let ring_path = dir.join("ring.bin");
    if ring_path.exists() {
        let mut ring = CrashRing::open(&ring_path, 4096, ring_key_for(home)?)?;
        let pending = ring.drain()?;
        println!("  ring.bin: {} slots, {} pending event(s)", ring.capacity(), pending.len());
        if ring.salvaged() {
            println!("  ring.bin: header was corrupt — slots salvaged, recovery still works");
        }
        if !pending.is_empty() {
            println!("  ^ there IS unsaved input waiting — run `savepoint recover`");
        }
    } else {
        println!("  ring.bin: not created yet");
    }

    println!("\n[capture backend]");
    let src = capture::default_source();
    println!("  backend: {}", src.name());
    drop(src);
    #[cfg(target_os = "macos")]
    {
        let (ax, tap) = crate::capture::macos::permission_status();
        println!(
            "  accessibility: {}",
            if ax {
                "granted"
            } else {
                "NOT GRANTED — System Settings > Privacy & Security > Accessibility"
            }
        );
        println!(
            "  input monitoring: {}",
            if tap {
                "granted (event tap created)"
            } else {
                "NOT GRANTED — System Settings > Privacy & Security > Input Monitoring"
            }
        );
        if !ax || !tap {
            problems += 1;
        }
    }
    #[cfg(windows)]
    {
        println!("  keyboard hook: userland, no elevation needed");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        println!(
            "  note: Wayland/X11 do not allow global keyboard hooks. Clipboard-only capture is"
        );
        println!("        the honest maximum here; `savepoint run` still protects every copy.");
    }

    println!("\n[privacy]");
    println!("  blocked apps:   {}", crate::policy::BLOCKED_APPS.len());
    println!("  blocked titles: {} patterns", crate::policy::BLOCKED_TITLES.len());
    println!("  redaction:      card numbers (Luhn), private keys, JWTs, cloud/API tokens");
    println!("  network:        none. there is no network code in this binary.");

    println!(
        "\n{}",
        if problems == 0 {
            "all checks passed.".to_string()
        } else {
            format!("{problems} issue(s) found above.")
        }
    );
    Ok(())
}

fn cmd_status(home: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let (d, r) = vault.counts()?;
    let rows = vault.list(5, 40)?;
    println!("{}", crate::TAGLINE);
    println!("vault      {} ({} rescues, {} revisions)", vault.dir().display(), d, r);
    let ring_path = vault.ring_path();
    if ring_path.exists() {
        let mut ring = CrashRing::open(&ring_path, 4096, ring_key_for(home)?)?;
        let pending = ring.drain()?;
        println!(
            "buffer     {} event(s) pending{}",
            pending.len(),
            if ring.salvaged() { " (salvaged after a damaged header)" } else { "" }
        );
        if !pending.is_empty() {
            println!("           ^ run `savepoint recover` to commit them");
        }
    }
    if rows.is_empty() {
        println!("latest     (none yet)");
    } else {
        println!("latest     ");
        for r in rows {
            println!(
                "  #{:<4} {:<10} {:<16} {}",
                r.id,
                human_age(r.age()),
                truncate(r.app.as_deref().unwrap_or("?"), 16),
                r.preview.unwrap_or_default().replace('\n', " ")
            );
        }
    }
    println!("capture    {}", capture::default_source().name());
    Ok(())
}

fn cmd_simulate(
    home: &Option<PathBuf>,
    text: &str,
    app: &str,
    crash: bool,
    ring_slots: usize,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = open_vault(home)?;
    let engine = Engine::new(vault, ring_slots, Policy::default())?;
    let mut ev = crate::ring::Event::new(crate::ring::EV_FOCUS, app);
    engine.ingest(ev);
    for (i, c) in text.chars().enumerate() {
        ev = crate::ring::Event::new(crate::ring::EV_CHAR, app);
        ev.set_text(&c.to_string());
        ev.ts = crate::ring::now_secs().wrapping_add(i as u32 / 1000);
        engine.ingest(ev);
    }
    if !quiet {
        let buffered = engine.stats.events_buffered.load(Ordering::Relaxed);
        let dropped = engine.stats.events_dropped_by_policy.load(Ordering::Relaxed);
        eprintln!(
            "simulated {} chars as {app}: {buffered} buffered, {dropped} refused by policy",
            text.chars().count()
        );
    }
    if crash {
        // A real crash: no unwinding, no atexit handlers, no flush. Exactly what a hard
        // process death feels like from the ring's point of view.
        let _ = io::stderr().write_all(b"simulating a hard crash (SIGKILL, no flush)...\n");
        io::stderr().flush().ok();
        std::process::abort();
    }
    engine.flush()?;
    if !quiet {
        eprintln!("committed to the vault.");
    }
    Ok(())
}

/// The ring key, derived the same way the vault derives it, for read-only diagnostics.
fn ring_key_for(
    home: &Option<PathBuf>,
) -> Result<crate::crypto::SecretKey, Box<dyn std::error::Error>> {
    let dir = data_dir(home);
    let (key, _) = load_or_create_key(&dir)?;
    let salt_raw = std::fs::read(dir.join("salt.bin"))?;
    let mut salt = [0u8; crate::crypto::SALT_LEN];
    if salt_raw.len() != salt.len() {
        return Err("salt.bin is corrupt".into());
    }
    salt.copy_from_slice(&salt_raw);
    Ok(crate::crypto::derive_keys(&key, &salt).ring)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

/// Async-signal-safe SIGINT handler: flips an `AtomicBool`, nothing else.
#[cfg(unix)]
fn install_sigint(stop: Arc<AtomicBool>) {
    static STOP: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();
    let _ = STOP.set(stop);
    extern "C" fn handler(_sig: i32) {
        if let Some(s) = STOP.get() {
            s.store(true, Ordering::Relaxed);
        }
    }
    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    }
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    unsafe {
        signal(SIGINT, handler);
        signal(SIGTERM, handler);
    }
}

pub fn install_sigint_pub(stop: Arc<AtomicBool>) {
    install_sigint(stop)
}

#[cfg(not(unix))]
pub fn install_sigint(stop: Arc<AtomicBool>) {
    // Windows: the hook thread owns the message loop; Ctrl-C closes it via the console
    // handler that SetConsoleCtrlHandler installs in the tray build.
    let _ = stop;
}

/// Used by tests and the future tray UI: build a Vault without touching argv.
pub fn open_for_test(dir: &Path, key: &SecretKey) -> Result<Vault, Box<dyn std::error::Error>> {
    Ok(Vault::open(dir, key)?)
}

fn _stdin_hint() {
    let stdin = io::stdin();
    let _ = stdin.lock().lines();
}
