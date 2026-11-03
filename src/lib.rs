//! Savepoint — crash-proof, offline, encrypted rescues for everything you type.
//!
//! Library so the CLI, the tests and any future tray UI share one implementation.
//! Module map (deep modules, small interfaces — see docs/architecture.md):
//!
//! - [`crypto`]  — AES-256-GCM envelope + HKDF key separation. Hides everything.
//! - [`policy`]  — what we refuse to store. Runs *before* anything hits disk.
//! - [`ring`]    — the bounded append-only crash buffer. Survives `kill -9`.
//! - [`recover`] — turns an event stream back into text.
//! - [`vault`]   — encrypted SQLite storage, revisions, search, retention.
//! - [`engine`]  — orders the above; the only module that knows the pipeline.
//! - [`capture`] — OS backends behind one trait.
//! - [`config`]  — user settings, clamped and never fatal.
//! - [`remind`]  — break/water schedule. Behaviour-blind by construction: it can only ever
//!   be told *whether* input happened, never what it was.
//! - [`companion`] — `savepoint ask`: scaffold, install, simulate, and a terminal pet.
//! - [`avatar`]  — 3D humans/animals: untrusted-file audit + procedural rigged bodies.
//! - [`pet`]     — desktop companion manifests: validation (untrusted content) + procedural art.
//! - [`cli`]     — argument parsing and human-facing output.

pub mod crypto;

/// The one-line answer to "where is my stuff and is it safe?".
pub const TAGLINE: &str = "Savepoint — nothing you type is ever lost again.";

pub fn default_data_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("dev", "savepoint", "savepoint")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".savepoint"))
}
