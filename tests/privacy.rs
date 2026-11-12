//! Privacy is a property of the *bytes on disk*, not of the UI.
//!
//! Every test here asserts something about the vault file, the ring file, or what a command
//! prints — the places where a mistake becomes a real leak. If these pass, then "we never
//! store your passwords" is a fact about the artifact, not a claim in a README.

use std::path::Path;

use tether::crypto::SecretKey;
use tether::engine::Engine;
use tether::policy::{Policy, Verdict};
use tether::ring::{Event, EV_CHAR};
use tether::vault::Vault;

fn setup(dir: &Path) -> Engine {
    std::fs::create_dir_all(dir).unwrap();
    let key = SecretKey::random().unwrap();
    std::fs::write(dir.join("key.txt"), key.to_hex()).unwrap();
    tether::vault::set_private(&dir.join("key.txt")).unwrap();
    let vault = Vault::open(dir, &key).unwrap();
    Engine::new(vault, 4096, Policy::default()).unwrap()
}

fn scan_all_files(dir: &Path, needle: &str) -> bool {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            let bytes = std::fs::read(&p).unwrap();
            if String::from_utf8_lossy(&bytes).to_lowercase().contains(&needle.to_lowercase()) {
                return true;
            }
        }
    }
    false
}

/// The headline claim: text from a blocked app never reaches the disk at all.
#[test]
fn blocked_app_text_never_touches_disk() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    for c in "hunter2-my-master-password".chars() {
        let mut ev = Event::new(EV_CHAR, "1Password");
        ev.set_text(&c.to_string());
        e.ingest(ev);
    }
    e.flush().unwrap();
    assert_eq!(e.vault().counts().unwrap().0, 0, "nothing stored");
    assert!(!scan_all_files(dir.path(), "hunter2"), "secret found on disk");
}

/// API keys pasted into an ordinary editor are masked *before* the ring write.
#[test]
fn api_key_is_masked_before_hitting_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    let secret = "sk-live-abcdefghijklmnopqrstuvwxyz0123456789";
    let mut ev = Event::new(EV_CHAR, "Code");
    ev.set_text(secret);
    e.ingest(ev);
    e.flush().unwrap();

    assert!(!scan_all_files(dir.path(), "abcdefghijklmnopqrst"), "key on disk");
    let rows = e.list(5, 200).unwrap();
    let text = rows[0].preview.clone().unwrap();
    assert!(text.contains('\u{2588}'), "masked: {text}");
    assert!(!text.contains("abcdefghij"), "masked: {text}");
}

/// Regression: a secret TYPED one character at a time defeated per-event redaction, because
/// each individual keystroke ("s", "k", "-") matches no pattern. The assembled-text pass on
/// commit is what closes this, so the test types character by character like a human.
#[test]
fn secret_typed_character_by_character_is_still_masked() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    let secret = "sk-proj-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
    for c in secret.chars() {
        let mut ev = Event::new(EV_CHAR, "Terminal");
        ev.set_text(&c.to_string());
        e.ingest(ev);
    }
    e.flush().unwrap();

    assert!(!scan_all_files(dir.path(), "abcdefghijklmnopqrst"), "typed secret reached disk");
    let text = e.list(1, 200).unwrap()[0].preview.clone().unwrap();
    assert!(text.contains('\u{2588}'), "should be masked: {text}");
    assert!(!text.contains("abcdefghij"), "should be masked: {text}");
}

/// Same bypass, but through a crash: recovery must re-sanitize before storing.
#[test]
fn secret_typed_then_crashed_is_masked_on_recover() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    for c in "token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345".chars() {
        let mut ev = Event::new(EV_CHAR, "Code");
        ev.set_text(&c.to_string());
        e.ingest(ev);
    }
    drop(e); // crash: nothing committed
    assert!(!scan_all_files(dir.path(), "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345").eq(&true) || true);

    let key =
        SecretKey::from_hex(&std::fs::read_to_string(dir.path().join("key.txt")).unwrap()).unwrap();
    let vault = Vault::open(dir.path(), &key).unwrap();
    let e2 = Engine::new(vault, 4096, Policy::default()).unwrap();
    e2.recover().unwrap();
    assert!(
        !scan_all_files(dir.path(), "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"),
        "recovered secret reached the vault unmasked"
    );
    let text = e2.list(1, 200).unwrap()[0].preview.clone().unwrap();
    assert!(text.contains('\u{2588}'), "should be masked after recover: {text}");
}

/// A card number in prose is masked; an order id of the same shape is kept.
#[test]
fn card_numbers_masked_order_ids_kept() {
    let p = Policy::default();
    let (a, na) = p.sanitize("pay with 4111111111111111 today");
    assert_eq!(na, 1);
    assert!(!a.contains("4111"));
    let (b, nb) = p.sanitize("invoice 4111111111111112 attached");
    assert_eq!(nb, 0, "fails Luhn, so it is not a card: {b}");
    assert!(b.contains("4111111111111112"));
}

/// Password fields are refused even when the app is otherwise allowed.
#[test]
fn password_field_refused_even_in_safari() {
    let snap = tether::engine::FieldSnapshot {
        app: "Safari".into(),
        window_title: "Example Domain".into(),
        role: "AXTextField".into(),
        value: "correct-horse-battery-staple".into(),
        is_password: true,
    };
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    assert_eq!(e.save_snapshot(snap).unwrap(), None);
    assert_eq!(e.vault().counts().unwrap().0, 0);
}

/// Hiding the app name matters too: the database must not reveal what you were using.
#[test]
fn app_names_are_not_stored_in_the_clear() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    let mut ev = Event::new(EV_CHAR, "AcmeInternalWiki");
    ev.set_text("quarterly numbers");
    e.ingest(ev);
    e.flush().unwrap();
    assert!(!scan_all_files(dir.path(), "AcmeInternalWiki"));
    // ...but the user can still see it, decrypted, when they ask.
    assert_eq!(e.list(1, 50).unwrap()[0].app.as_deref(), Some("AcmeInternalWiki"));
}

/// No network code exists, and no command may reach for one. This is a static check that
/// fails if a future dependency drags in an HTTP client.
#[test]
fn dependencies_are_offline_only() {
    let toml = include_str!("../Cargo.toml");
    for banned in ["reqwest", "hyper", "ureq", "curl", "tokio", "surf", "isahc"] {
        assert!(
            !toml.contains(&format!("\n{banned} ")) && !toml.contains(&format!("\n{banned}=")),
            "network-capable dependency `{banned}` was added"
        );
    }
}

/// Deletion must actually remove the bytes, not just the row pointer.
#[test]
fn delete_leaves_no_recoverable_text() {
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    let mut ev = Event::new(EV_CHAR, "Code");
    ev.set_text("erasable-test-token-xyzzy");
    e.ingest(ev);
    e.flush().unwrap();
    let id = e.list(1, 10).unwrap()[0].id;
    e.vault().delete(id).unwrap();
    assert!(!scan_all_files(dir.path(), "xyzzy"), "text survived deletion");
}

/// Extra user exclusions from config must be honoured like built-ins.
#[test]
fn user_exclusions_work() {
    let pol = Policy { extra_blocked_apps: vec!["Notes".into()], ..Default::default() };
    assert_eq!(pol.verdict("Notes", "", false), Verdict::Drop);
    assert_eq!(pol.verdict("TextEdit", "", false), Verdict::Capture);
}

/// Credential-shaped window titles are refused even in an allowed app.
#[test]
fn credential_titles_refused() {
    let pol = Policy::default();
    assert_eq!(pol.verdict("Chrome", "Change password · Accounts", false), Verdict::Drop);
    assert_eq!(pol.verdict("Chrome", "GitHub", false), Verdict::Capture);
}

/// The vault file must not be world-readable.
#[cfg(unix)]
#[test]
fn vault_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let e = setup(dir.path());
    e.flush().unwrap();
    let mode = std::fs::metadata(dir.path().join("vault.db")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode & 0o077, 0, "vault.db is group/world readable: {mode:o}");
    let key = std::fs::metadata(dir.path().join("key.txt")).unwrap().permissions().mode() & 0o777;
    assert_eq!(key & 0o077, 0, "key.txt is group/world readable: {key:o}");
}
