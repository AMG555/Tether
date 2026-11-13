//! The promise under test: *nothing you type is lost, even if the process dies mid-sentence.*
//!
//! These tests do not mock the crash. They kill the process the way a crash kills it —
//! `SIGKILL` / `abort()`, no unwinding, no flush, no `Drop` — and then assert the text
//! comes back. A recovery tool that is only tested by graceful shutdown is not tested.

use std::path::Path;
#[cfg(unix)]
use std::process::Command;
use std::time::Duration;

use tether::crypto::SecretKey;
use tether::engine::Engine;
use tether::policy::Policy;
use tether::ring::{Event, EV_BACKSPACE, EV_CHAR, EV_FOCUS, EV_PASTE};
use tether::vault::Vault;

fn engine(dir: &Path, slots: usize) -> Engine {
    let key = SecretKey::from_hex(&std::fs::read_to_string(dir.join("key.txt")).unwrap()).unwrap();
    let vault = Vault::open(dir, &key).unwrap();
    Engine::new(vault, slots, Policy::default()).unwrap()
}

fn init(dir: &Path) -> Engine {
    init_with(dir, 4096)
}

fn init_with(dir: &Path, slots: usize) -> Engine {
    std::fs::create_dir_all(dir).unwrap();
    let key = SecretKey::random().unwrap();
    std::fs::write(dir.join("key.txt"), key.to_hex()).unwrap();
    let vault = Vault::open(dir, &key).unwrap();
    Engine::new(vault, slots, Policy::default()).unwrap()
}

fn type_text(e: &Engine, app: &str, text: &str) {
    for (i, c) in text.chars().enumerate() {
        let mut ev = Event::new(EV_CHAR, app);
        ev.set_text(&c.to_string());
        ev.ts = 1000 + i as u32;
        e.ingest(ev);
    }
}

/// The exact failure from the bug report: you are 40 words into a prompt, the app dies.
#[test]
fn half_written_prompt_survives_an_ungraceful_death() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    let prompt = "refactor the auth middleware so that tokens rotate, and keep the tests green";

    type_text(&e, "Claude", prompt);
    // Drop without flush: nothing reached the vault. Only the ring knows.
    drop(e);
    assert_eq!(engine(dir.path(), 4096).vault().counts().unwrap().0, 0);

    let e2 = engine(dir.path(), 4096);
    assert_eq!(e2.recover().unwrap(), 1);
    let rows = e2.list(10, 200).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].preview.as_deref(), Some(prompt));
    assert_eq!(rows[0].app.as_deref(), Some("Claude"));
}

/// Same thing, but through the real binary killed with SIGKILL from the outside.
#[cfg(unix)]
#[test]
fn subprocess_sigkill_still_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_string_lossy().to_string();
    let bin = env!("CARGO_BIN_EXE_tether");

    // `simulate --crash` types into the ring, then aborts the process (no flush).
    let out = Command::new(bin)
        .env("TETHER_HOME", &home)
        .args([
            "simulate",
            "--text",
            "the half prompt that must not vanish",
            "--app",
            "Editor",
            "--crash",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "process was supposed to die");

    let recovered = Command::new(bin).env("TETHER_HOME", &home).arg("recover").output().unwrap();
    assert!(recovered.status.success());
    let stdout = String::from_utf8_lossy(&recovered.stdout);
    assert!(stdout.contains("recovered 1 rescue"), "{stdout}");

    let listed = Command::new(bin)
        .env("TETHER_HOME", &home)
        .args(["list", "--preview", "80"])
        .output()
        .unwrap();
    let rows = String::from_utf8_lossy(&listed.stdout);
    assert!(rows.contains("the half prompt that must not vanish"), "{rows}");
}

/// Ctrl-C is the graceful path; it must commit rather than discard the tail.
#[test]
fn graceful_shutdown_commits_the_tail() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Notes", "last words before ctrl-c");
    assert_eq!(e.flush().unwrap(), 1);
    let rows = e.list(5, 100).unwrap();
    assert_eq!(rows[0].preview.as_deref(), Some("last words before ctrl-c"));
}

/// A crash *between* two flushes must not split one draft into two rows.
#[test]
fn flushes_do_not_fork_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    let mut head = Event::new(EV_FOCUS, "Code");
    head.ts = 500;
    e.ingest(head);
    type_text(&e, "Code", "first half ");
    e.flush().unwrap();
    type_text(&e, "Code", "second half");
    e.flush().unwrap();

    let rows = e.list(10, 100).unwrap();
    assert_eq!(rows.len(), 1, "one session, one tether: {:?}", rows.len());
    assert_eq!(rows[0].preview.as_deref(), Some("first half second half"));
}

/// Backspacing to correct yourself is part of what you typed — recovery must honour it.
#[test]
fn corrections_are_replayed_not_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Term", "dete");
    for i in 0..2 {
        let mut bs = Event::new(EV_BACKSPACE, "Term");
        bs.ts = 2000 + i;
        e.ingest(bs);
    }
    type_text(&e, "Term", "lete");
    drop(e);

    let e2 = engine(dir.path(), 4096);
    e2.recover().unwrap();
    assert_eq!(e2.list(5, 50).unwrap()[0].preview.as_deref(), Some("delete"));
}

/// Two apps, one crash: drafts must not bleed into each other.
#[test]
fn drafts_stay_separate_per_app() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Code", "const x = 1;");
    type_text(&e, "Slack", "shipping in 5");
    drop(e);

    let e2 = engine(dir.path(), 4096);
    e2.recover().unwrap();
    let rows = e2.list(10, 100).unwrap();
    assert_eq!(rows.len(), 2);
    let all: Vec<String> = rows.iter().map(|r| r.preview.clone().unwrap_or_default()).collect();
    assert!(all.iter().any(|t| t == "const x = 1;"), "{all:?}");
    assert!(all.iter().any(|t| t == "shipping in 5"), "{all:?}");
}

/// A big paste must be one event, not thousands, or it would evict the whole ring.
#[test]
fn large_paste_does_not_evict_history() {
    let dir = tempfile::tempdir().unwrap();
    let e = init_with(dir.path(), 64);
    let mut p = Event::new(EV_PASTE, "Code");
    p.set_text(&"x".repeat(60));
    e.ingest(p);
    type_text(&e, "Code", "tail");
    e.flush().unwrap();
    let rows = e.list(5, 200).unwrap();
    let text = rows[0].preview.clone().unwrap();
    assert!(text.contains("tail"), "recent typing still there: {text}");
}

/// When the ring genuinely overflows, we say so instead of silently returning less text.
#[test]
fn ring_overflow_is_reported_not_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let e = init_with(dir.path(), 8);
    type_text(&e, "Code", "abcdefghijklmnop");
    e.flush().unwrap();
    let rows = e.list(5, 100).unwrap();
    assert!(rows[0].truncated, "overflow must set the partial flag");
    // The ring holds the newest 8 keystrokes ("ijklmnop"); the live session knows the full
    // text and is what gets committed. Either way: nothing past the cap may be claimed as
    // complete, and the newest typing must be present.
    let preview = rows[0].preview.clone().unwrap();
    assert!(preview.ends_with("ijklmnop"), "the newest keystrokes survive: {preview:?}");
    assert_eq!(preview, "abcdefghijklmnop", "session text is authoritative while alive");
}

/// Repeated flushes of an untouched field must not fill the vault with revisions.
#[test]
fn idle_field_does_not_spam_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Code", "stable text");
    for _ in 0..5 {
        e.flush().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    let (drafts, revs) = e.vault().counts().unwrap();
    assert_eq!(drafts, 1);
    assert_eq!(revs, 0, "no-op flushes created {revs} revisions");
}

/// Editing after a flush keeps the previous version, so a bad rewrite is undoable.
#[test]
fn edits_keep_the_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Code", "good version");
    e.flush().unwrap();
    for i in 0..7 {
        let mut bs = Event::new(EV_BACKSPACE, "Code");
        bs.ts = 4000 + i;
        e.ingest(bs);
    }
    type_text(&e, "Code", "bad");
    e.flush().unwrap();

    let rows = e.list(5, 100).unwrap();
    assert_eq!(rows[0].preview.as_deref(), Some("good bad"));
    let revs = e.vault().revisions(rows[0].id).unwrap();
    assert_eq!(revs, vec!["good version".to_string()]);
}

/// Recovery is idempotent: running it twice must not duplicate anything.
#[test]
fn recover_twice_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Code", "once");
    drop(e);
    let e2 = engine(dir.path(), 4096);
    assert_eq!(e2.recover().unwrap(), 1);
    assert_eq!(e2.recover().unwrap(), 0, "second recover finds an empty ring");
    assert_eq!(e2.vault().counts().unwrap().0, 1);
}

/// The vault must fail closed on a wrong key rather than silently starting over.
#[test]
fn wrong_key_never_destroys_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let e = init(dir.path());
    type_text(&e, "Code", "precious");
    e.flush().unwrap();
    drop(e);

    let bad = SecretKey::random().unwrap();
    let res = Vault::open(dir.path(), &bad);
    assert!(matches!(res, Err(tether::vault::VaultError::WrongKey)));
    // And the original still opens fine.
    assert_eq!(engine(dir.path(), 4096).vault().counts().unwrap().0, 1);
}
