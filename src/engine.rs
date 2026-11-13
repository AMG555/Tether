//! The capture engine: the seam between "a key was pressed" and "a draft is safe".
//!
//! Everything platform-specific lives behind `Source`; everything security-relevant lives
//! behind `Vault` and `Policy`. This module is the only place that knows the ordering, and
//! the ordering *is* the product:
//!
//! ```text
//!  OS hook ──► Policy.verdict ──► Policy.sanitize ──┬──► Session (in RAM, live text)
//!                                                   └──► CrashRing.push   (µs, no fsync)
//!                                                              │
//!                                          every flush_ms ─────┤
//!                                                              ▼
//!                                        Session ──► Vault (encrypted, WAL) ──► ring cleared
//!                                                                              + tail re-seeded
//! ```
//!
//! Why both a session *and* a ring, when either alone looks sufficient:
//!
//! - The ring alone cannot represent "delete the last character of text I already saved".
//!   Once a flush clears it, a backspace has no base string to delete from, and the next
//!   commit appends instead of correcting. That bug is invisible until a user backspaces
//!   right after a save — which is exactly when they are fixing a mistake.
//! - The session alone loses everything when the process dies, which is the whole problem
//!   this tool exists to solve.
//!
//! So: the session is the source of truth for *what the text is*, the ring is the crash net
//! for the last few hundred milliseconds, and on flush the ring is re-seeded with the tail of
//! the committed text. A crash then replays "the tail I last saw, plus whatever came after",
//! and `stitch()` puts the two halves back together. Both paths are covered by tests.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::crypto::SecretKey;
use crate::policy::{Policy, Verdict};
use crate::recover::{reconstruct, Draft};
use crate::ring::{CrashRing, Event, EV_BACKSPACE, EV_CHAR, EV_FOCUS, EV_PASTE};
use crate::vault::{DraftRow, Vault, VaultError};

/// How many characters of the committed tail are re-seeded into the ring after a flush.
/// Bounded by the ring slot size, and only needs to cover backspacing.
pub const TAIL_SEED_CHARS: usize = 56;

/// Hard cap on the live session text. Beyond this we keep the newest text and flag the
/// draft as partial rather than grow memory without bound on a pathological input source.
pub const MAX_SESSION_CHARS: usize = 256 * 1024;

/// A snapshot of one focused text field, read through the OS accessibility layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSnapshot {
    pub app: String,
    pub window_title: String,
    /// Accessibility role, e.g. `AXTextField`, `Edit`, `AXWebArea`.
    pub role: String,
    /// Full current contents of the field.
    pub value: String,
    /// The OS says this is a password/secure field.
    pub is_password: bool,
}

/// Platform capture backend. Implementations must be cheap, non-blocking, and must never
/// panic on malformed OS data.
pub trait Source: Send {
    /// Human-readable backend name for `tether doctor`.
    fn name(&self) -> &'static str;
    /// Polls the focused text field. `None` when the platform cannot tell us, when no
    /// text field is focused, or when accessibility permission is missing.
    fn probe(&mut self) -> Option<FieldSnapshot> {
        None
    }
    /// Runs the capture loop until `stop` is set, delivering events to `sink`.
    fn run(&mut self, sink: Arc<dyn Fn(Event) + Send + Sync>, stop: &AtomicBool);
}

/// The text being typed right now. Lives in RAM only; never written in plaintext.
#[derive(Debug, Clone)]
pub struct Session {
    pub app: String,
    pub started_ts: u32,
    pub text: String,
    pub events: u32,
    pub redactions: u32,
    pub truncated: bool,
}

impl Session {
    fn new(app: String, ts: u32) -> Self {
        Self {
            app,
            started_ts: ts,
            text: String::new(),
            events: 0,
            redactions: 0,
            truncated: false,
        }
    }

    fn apply(&mut self, ev: &Event) {
        self.events = self.events.saturating_add(1);
        match ev.kind {
            EV_CHAR | EV_PASTE => {
                let t = ev.text_str();
                if t.contains(crate::policy::MASK_CHAR) {
                    self.redactions = self.redactions.saturating_add(1);
                }
                // An event whose payload reached the slot capacity may have been cut at the
                // storage layer. Say so instead of pretending the draft is complete.
                if ev.kind != EV_CHAR && t.len() >= 60 || t.len() >= 64 {
                    self.truncated = true;
                }
                self.text.push_str(t);
                if self.text.chars().count() > MAX_SESSION_CHARS {
                    let keep: String = self
                        .text
                        .chars()
                        .rev()
                        .take(MAX_SESSION_CHARS)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    self.text = keep;
                    self.truncated = true;
                }
            }
            EV_BACKSPACE => crate::recover::pop_one(&mut self.text),
            _ => {}
        }
    }

    fn tail(&self) -> String {
        let n = self.text.chars().count();
        if n <= TAIL_SEED_CHARS {
            return self.text.clone();
        }
        self.text.chars().skip(n - TAIL_SEED_CHARS).collect()
    }

    fn to_draft(&self) -> Draft {
        Draft {
            app: self.app.clone(),
            text: self.text.clone(),
            started_ts: self.started_ts,
            updated_ts: crate::ring::now_secs(),
            events: self.events,
            redactions: self.redactions,
            truncated: self.truncated,
        }
    }
}

/// Merges a replayed crash tail onto text that was already committed before the crash.
///
/// The replayed text begins with `TAIL_SEED_CHARS` (or fewer) characters that were already
/// saved, so we drop that overlap and splice. If the overlap does not match — the vault was
/// edited out from under us, or the ring wrapped — we keep both halves rather than guess:
/// duplicating a few words is recoverable, silently deleting the user's sentence is not.
pub fn stitch(prior: &str, replayed: &str) -> String {
    if prior.is_empty() {
        return replayed.to_string();
    }
    let n = prior.chars().count();
    let seed_len = n.min(TAIL_SEED_CHARS);
    let seed: String = prior.chars().skip(n - seed_len).collect();
    let replayed_head: String = replayed.chars().take(seed_len).collect();
    if replayed_head == seed {
        // `replayed` *starts with* the seeded tail, so the reconstruction is:
        //     prior[..n-seed_len]  +  replayed
        // not `prior[..] + replayed[seed..]` — the latter silently deletes the seeded
        // characters, which the seam test caught destroying a whole short draft.
        let head: String = prior.chars().take(n - seed_len).collect();
        format!("{head}{replayed}")
    } else {
        // No trustworthy overlap. Concatenating duplicates `seed_len` characters at worst;
        // that is a visible, fixable annoyance. Dropping text is not.
        format!("{prior}{replayed}")
    }
}

pub struct Stats {
    pub events_seen: AtomicU64,
    pub events_dropped_by_policy: AtomicU64,
    pub events_buffered: AtomicU64,
    pub chars_buffered: AtomicU64,
    pub redactions: AtomicU64,
    pub commits: AtomicU64,
    pub recovered_on_start: AtomicU64,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            events_seen: AtomicU64::new(0),
            events_dropped_by_policy: AtomicU64::new(0),
            events_buffered: AtomicU64::new(0),
            chars_buffered: AtomicU64::new(0),
            redactions: AtomicU64::new(0),
            commits: AtomicU64::new(0),
            recovered_on_start: AtomicU64::new(0),
        }
    }
}

pub struct Engine {
    ring: Mutex<CrashRing>,
    vault: Mutex<Vault>,
    policy: Mutex<Policy>,
    session: Mutex<Option<Session>>,
    pub stats: Stats,
    /// Set when the ring wrote past its capacity: the next draft is flagged truncated.
    wrapped: AtomicBool,
    last_flush: Mutex<Instant>,
}

impl Engine {
    pub fn new(vault: Vault, ring_slots: usize, policy: Policy) -> std::io::Result<Self> {
        // Clone the ring key so the ring can encrypt without holding the vault lock.
        let ring_key = SecretKey::from_bytes(*vault.ring_key().as_bytes());
        let ring = CrashRing::open(vault.ring_path(), ring_slots, ring_key)?;
        Ok(Self {
            ring: Mutex::new(ring),
            vault: Mutex::new(vault),
            policy: Mutex::new(policy),
            session: Mutex::new(None),
            stats: Stats::default(),
            wrapped: AtomicBool::new(false),
            last_flush: Mutex::new(Instant::now()),
        })
    }

    pub fn vault(&self) -> std::sync::MutexGuard<'_, Vault> {
        self.vault.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The text currently being typed, for a live "show me my buffer" view. Sanitized: the
    /// in-RAM session is raw for reconstruction purposes, and nothing that leaves this type
    /// should be able to print a secret to a terminal or a log file.
    pub fn live_text(&self) -> Option<(String, String)> {
        let raw = self
            .session
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|x| (x.app.clone(), x.text.clone())))
            .filter(|(_, t)| !t.is_empty())?;
        let pol = self.policy.lock().unwrap_or_else(|e| e.into_inner());
        let (clean, _) = pol.sanitize(&raw.1);
        Some((raw.0, clean))
    }

    /// Hot path: one policy check, one ring write, one String push. Called on the input
    /// thread, so it must stay allocation-light and must never take the vault lock (which
    /// the flusher holds while committing).
    pub fn ingest(&self, ev: Event) {
        self.stats.events_seen.fetch_add(1, Ordering::Relaxed);
        let app = ev.app_str().to_string();

        let (safe, redactions) = {
            let pol = self.policy.lock().unwrap_or_else(|e| e.into_inner());
            // Tier 1: refuse the whole surface. Nothing below runs, so nothing is stored.
            if pol.verdict(&app, "", false) == Verdict::Drop {
                self.stats.events_dropped_by_policy.fetch_add(1, Ordering::Relaxed);
                return;
            }
            // Tier 2: mask secrets *before* they can reach the ring file.
            let (s, n) = pol.sanitize(ev.text_str());
            (s, n)
        };
        if redactions > 0 {
            self.stats.redactions.fetch_add(redactions as u64, Ordering::Relaxed);
        }
        let mut safe_ev = ev;
        if safe != ev.text_str() {
            safe_ev.set_text(&safe);
        }

        // Session bookkeeping. A focus change (or a keystroke from a different app) ends the
        // previous session and commits it, so drafts never bleed across apps.
        let boundary = {
            let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let switch = match guard.as_ref() {
                None => true,
                Some(s) => ev.kind == EV_FOCUS || (s.app != app && !app.is_empty()),
            };
            if switch && guard.is_some() {
                // Commit outside this lock; hand the old session back.
                let old = guard.take();
                drop(guard);
                if let Some(old) = old {
                    let _ = self.commit_session(&old);
                }
                let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some(Session::new(app.clone(), safe_ev.ts));
                false
            } else {
                if guard.is_none() {
                    *guard = Some(Session::new(app.clone(), safe_ev.ts));
                }
                true
            }
        };
        let _ = boundary;

        if let Ok(mut guard) = self.session.lock() {
            if let Some(s) = guard.as_mut() {
                s.apply(&safe_ev);
            }
        }

        // Crash net: mirror the event into the ring.
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        if ring.written >= ring.capacity() as u64 {
            self.wrapped.store(true, Ordering::Relaxed);
        }
        if ring.push(&safe_ev).is_ok() {
            self.stats.events_buffered.fetch_add(1, Ordering::Relaxed);
            self.stats
                .chars_buffered
                .fetch_add(safe_ev.text_str().chars().count() as u64, Ordering::Relaxed);
        }
    }

    /// Writes one session to the vault. Returns the number of drafts committed (0 or 1).
    ///
    /// Redaction runs **twice** on purpose, and the second pass is the one that matters for
    /// the durable store: once per event in `ingest` (catches a pasted secret in one shot,
    /// cheaply) and once here on the assembled session (catches a secret *typed* one character
    /// at a time, which no per-event pass can see because it only ever looks at one char).
    ///
    /// An adversarial review caught exactly that gap: `sk-proj-…` typed into a terminal was
    /// stored unmasked, because each individual keystroke was innocent.
    fn commit_session(&self, session: &Session) -> Result<usize, VaultError> {
        if session.text.trim().is_empty() {
            return Ok(0);
        }
        let mut draft = session.to_draft();
        let (sanitized, masked) = {
            let pol = self.policy.lock().unwrap_or_else(|e| e.into_inner());
            pol.sanitize(&draft.text)
        };
        if masked > 0 {
            draft.text = sanitized;
            draft.redactions = draft.redactions.saturating_add(masked as u32);
            self.stats.redactions.fetch_add(masked as u64, Ordering::Relaxed);
        }
        draft.truncated |= self.wrapped.swap(false, Ordering::Relaxed);
        let vault = self.vault.lock().unwrap_or_else(|e| e.into_inner());
        vault.save_draft(&draft)?;
        self.stats.commits.fetch_add(1, Ordering::Relaxed);
        Ok(1)
    }

    /// Commits the live session, then re-seeds the ring with the tail of what was committed
    /// so a crash immediately after this point still replays corrections correctly.
    pub fn flush(&self) -> Result<usize, VaultError> {
        let session = self.session.lock().map(|g| g.clone()).ok().flatten();

        let n = match session.as_ref() {
            Some(s) => self.commit_session(s)?,
            None => 0,
        };

        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let _ = ring.sync();
        ring.clear()?;
        if let Some(s) = session.as_ref() {
            self.seed_ring(&mut ring, s);
        }
        *self.last_flush.lock().unwrap() = Instant::now();
        Ok(n)
    }

    /// Puts the committed tail back into the ring: FOCUS, then one PASTE holding the last
    /// `TAIL_SEED_CHARS` characters. On a crash, replay starts from real text instead of
    /// from nothing, so "type, crash, backspace, crash" still loses nothing.
    fn seed_ring(&self, ring: &mut CrashRing, s: &Session) {
        let mut head = Event::new(EV_FOCUS, &s.app);
        head.ts = s.started_ts;
        let _ = ring.push(&head);
        let tail = s.tail();
        if !tail.is_empty() {
            let mut ev = Event::new(EV_PASTE, &s.app);
            ev.set_text(&tail);
            let _ = ring.push(&ev);
        }
    }

    /// Replays whatever survived in the ring after a crash and stitches it onto the last
    /// committed version. Runs once at startup, before new input is accepted, so recovered
    /// text can never interleave with fresh keystrokes.
    pub fn recover(&self) -> Result<usize, VaultError> {
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let events = ring.drain()?;
        if events.is_empty() {
            return Ok(0);
        }
        drop(ring);

        let mut drafts = reconstruct(&events);
        // Recovered text gets the same authoritative redaction pass as live text: a secret
        // typed across a crash must not slip into the vault because it arrived as events.
        {
            let pol = self.policy.lock().unwrap_or_else(|e| e.into_inner());
            for d in drafts.iter_mut() {
                let (clean, masked) = pol.sanitize(&d.text);
                if masked > 0 {
                    d.text = clean;
                    d.redactions = d.redactions.saturating_add(masked as u32);
                }
            }
        }
        let vault = self.vault.lock().unwrap_or_else(|e| e.into_inner());
        let mut committed = 0;
        for mut d in drafts {
            let field_id = vault.field_id(&format!("{}|{}", d.app, d.started_ts));
            if let Some((prior, _)) = vault.find_text(&field_id, d.started_ts as i64)? {
                if prior != d.text {
                    d.text = stitch(&prior, &d.text);
                }
            }
            if d.text.trim().is_empty() {
                continue;
            }
            vault.save_draft(&d)?;
            committed += 1;
        }
        drop(vault);

        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.clear()?;
        self.stats.recovered_on_start.fetch_add(committed as u64, Ordering::Relaxed);
        Ok(committed)
    }

    /// True when the last recovery had to rebuild the ring from damaged metadata.
    pub fn ring_was_salvaged(&self) -> bool {
        self.ring.lock().map(|r| r.salvaged()).unwrap_or(false)
    }

    pub fn should_flush(&self, interval: Duration) -> bool {
        self.last_flush.lock().map(|t| t.elapsed() >= interval).unwrap_or(false)
    }

    pub fn list(&self, limit: usize, preview: usize) -> Result<Vec<DraftRow>, VaultError> {
        self.vault().list(limit, preview)
    }

    /// Probe mode: read the focused field's real value and store it as a draft. This is the
    /// high-fidelity path used on macOS/Windows where the accessibility API exposes the full
    /// string, so mid-string edits are captured exactly rather than replayed.
    pub fn save_snapshot(&self, snap: FieldSnapshot) -> Result<Option<i64>, VaultError> {
        let (safe, n) = {
            let pol = self.policy.lock().unwrap_or_else(|e| e.into_inner());
            if pol.verdict(&snap.app, &snap.window_title, snap.is_password) == Verdict::Drop {
                self.stats.events_dropped_by_policy.fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            pol.sanitize(&snap.value)
        };
        if n > 0 {
            self.stats.redactions.fetch_add(n as u64, Ordering::Relaxed);
        }
        if safe.trim().is_empty() {
            return Ok(None);
        }
        let now = crate::ring::now_secs();
        let field_id = self.vault().field_id(&format!("{}|{}", snap.app, snap.window_title));
        let created = self.vault().find_created_at(&field_id).unwrap_or(now as i64);
        let draft = Draft {
            app: snap.app,
            text: safe,
            started_ts: created as u32,
            updated_ts: now,
            events: 1,
            redactions: n as u32,
            truncated: false,
        };
        let id = self.vault().save_draft(&draft)?;
        if id > 0 {
            self.stats.commits.fetch_add(1, Ordering::Relaxed);
        }
        Ok(if id > 0 { Some(id) } else { None })
    }

    /// Runs the periodic commit loop on the calling thread until `stop` is set.
    /// Errors are reported once per distinct failure, not swallowed. A commit loop that fails
    /// silently is worse than one that crashes: the user believes they are protected while
    /// their rescues are not reaching the vault.
    pub fn run_commit_loop(&self, stop: &AtomicBool, interval: Duration) {
        let mut last_err: Option<String> = None;
        let mut last_prune = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            if self.should_flush(interval) {
                match self.flush() {
                    Ok(_) => last_err = None,
                    Err(e) => {
                        let msg = e.to_string();
                        if last_err.as_deref() != Some(msg.as_str()) {
                            eprintln!(
                                "tether: commit failed, text is still in the crash buffer: {msg}"
                            );
                            last_err = Some(msg);
                        }
                    }
                }
            }
            if last_prune.elapsed() >= Duration::from_secs(3600) {
                let _ = self.vault().prune_older_than(14);
                last_prune = Instant::now();
            }
        }
        if let Err(e) = self.flush() {
            eprintln!("tether: final commit failed: {e}");
        }
    }
}

/// Owns the background threads so `main` stays a thin shell.
pub struct Runtime {
    pub engine: Arc<Engine>,
    pub stop: Arc<AtomicBool>,
    /// Handles are kept so a future GUI can join cleanly on shutdown instead of leaking
    /// threads; the CLI relies on the OS reaping them at exit.
    pub threads: Vec<std::thread::JoinHandle<()>>,
}

impl Runtime {
    pub fn start(engine: Engine, flush_interval: Duration) -> Runtime {
        let engine = Arc::new(engine);
        let stop = Arc::new(AtomicBool::new(false));
        let e = Arc::clone(&engine);
        let s = Arc::clone(&stop);
        let handle = std::thread::spawn(move || e.run_commit_loop(&s, flush_interval));
        Runtime { engine, stop, threads: vec![handle] }
    }

    /// Signals the flusher and waits for a final commit — the graceful path.
    pub fn shutdown(self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stitch_does_not_duplicate_when_replay_starts_with_the_seed() {
        // Short draft: the whole thing was seeded, so an honest replay begins with it.
        let prior = "hello brave new world";
        let replayed = "hello brave new world and more";
        let out = stitch(prior, replayed);
        assert_eq!(out, replayed);
        assert_eq!(out.matches("new world").count(), 1, "no duplication: {out}");
    }

    #[test]
    fn stitch_splices_a_long_prior_at_the_seed_boundary() {
        // Long draft: only the last TAIL_SEED_CHARS were seeded into the ring.
        let prior: String = "ABCDEFGHIJKLMN".chars().chain("n".repeat(56).chars()).collect();
        let seed: String = prior.chars().skip(14).collect();
        assert_eq!(seed.chars().count(), TAIL_SEED_CHARS);
        let replayed = format!("{seed}XYZ");
        let out = stitch(&prior, &replayed);
        // Exactly the original text plus the new suffix: no lost prefix, no doubled seam.
        assert_eq!(out, format!("{prior}XYZ"));
    }

    #[test]
    fn stitch_keeps_both_halves_when_overlap_is_untrustworthy() {
        let out = stitch("what I actually wrote", "unrelated garbage");
        assert!(out.contains("what I actually wrote") && out.contains("unrelated garbage"));
    }

    #[test]
    fn stitch_empty_prior_is_pass_through() {
        assert_eq!(stitch("", "abc"), "abc");
    }

    #[test]
    fn stitch_without_overlap_keeps_everything() {
        let out = stitch("alpha", "omega");
        assert!(out.contains("alpha") && out.contains("omega"), "{out}");
    }

    #[test]
    fn tail_is_bounded() {
        let mut s = Session::new("app".into(), 0);
        s.text = "x".repeat(500);
        assert_eq!(s.tail().chars().count(), TAIL_SEED_CHARS);
    }
}
