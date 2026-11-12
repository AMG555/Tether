//! Performance is a feature here, not a vanity metric.
//!
//! The capture path runs inside a low-level OS input hook. On Windows, a hook callback that
//! takes longer than ~300 ms is silently uninstalled by the OS; on macOS a stalled event tap
//! is disabled. We are nowhere near that, but the budget philosophy matters: the hot path
//! must stay in microseconds so it stays invisible, and the flush runs on its own thread so a
//! slow disk never delays a keystroke.
//!
//! These are deliberately loose ceilings (10x headroom over current hardware) — they exist to
//! catch an accidental `fsync`, a per-key `String` allocation storm, or a synchronous DB call
//! sneaking into `ingest`, not to benchmark the machine.

use std::time::Instant;

use tether::{
    crypto::SecretKey,
    engine::Engine,
    policy::Policy,
    ring::{Event, EV_CHAR},
    vault::Vault,
};

fn engine(dir: &std::path::Path) -> Engine {
    std::fs::create_dir_all(dir).unwrap();
    let key = SecretKey::random().unwrap();
    std::fs::write(dir.join("key.txt"), key.to_hex()).unwrap();
    let vault = Vault::open(dir, &key).unwrap();
    Engine::new(vault, 8192, Policy::default()).unwrap()
}

#[test]
fn keystroke_ingest_stays_in_microseconds() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(dir.path());
    for _ in 0..500 {
        let mut ev = Event::new(EV_CHAR, "Code");
        ev.set_text("a");
        e.ingest(ev);
    }
    let n = 20_000;
    let t = Instant::now();
    for _ in 0..n {
        let mut ev = Event::new(EV_CHAR, "Code");
        ev.set_text("a");
        e.ingest(ev);
    }
    let per_key = t.elapsed().as_secs_f64() / n as f64;
    // Typical build: ~11 µs. Ceiling of 200 µs still leaves a 1500x margin under the OS limit.
    assert!(
        per_key < 200e-6,
        "ingest took {:.1} µs/keystroke — something slow was added to the hot path",
        per_key * 1e6
    );
}

#[test]
fn ingest_never_touches_the_database() {
    // With no flush, the vault must stay empty: the hot path is ring-only, which is what makes
    // a crash-safe tool cheap enough to run constantly.
    let dir = tempfile::tempdir().unwrap();
    let e = engine(dir.path());
    for c in "a moderately long sentence".chars() {
        let mut ev = Event::new(EV_CHAR, "Code");
        ev.set_text(&c.to_string());
        e.ingest(ev);
    }
    assert_eq!(e.vault().counts().unwrap().0, 0, "ingest wrote to the vault");
}

#[test]
fn flush_is_fast_enough_for_a_one_second_cadence() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(dir.path());
    for c in "a moderately long sentence being typed".chars() {
        let mut ev = Event::new(EV_CHAR, "Code");
        ev.set_text(&c.to_string());
        e.ingest(ev);
    }
    e.flush().unwrap();
    let t = Instant::now();
    for _ in 0..50 {
        e.flush().unwrap();
    }
    let per = t.elapsed().as_secs_f64() / 50.0;
    assert!(per < 0.05, "flush took {:.1} ms — the commit loop would lag", per * 1000.0);
}
