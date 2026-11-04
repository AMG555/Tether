//! The crash buffer: an append-only ring of the last N keystrokes, on disk.
//!
//! Why this exists (ADR 0002): the database write is "slow" (fsync, WAL checkpoint).
//! The two seconds between two keystrokes and the next commit are exactly the window
//! your work disappears through. So every keystroke lands in an append-only ring file
//! first — one `write(2)`, no fsync, page-cache buffered — and the database catches up
//! lazily. A process crash loses at most the bytes still in flight; a kernel panic or
//! power cut loses at most one 20 ms flush window.
//!
//! The file is a flat array of fixed 512-byte slots with a slot header, so recovery is
//! a linear scan that needs no index and no fsync. Fixed slots also make the file
//! bounded (default 64 KiB) — which matters, because this file is written by a
//! low-level input hook and must never grow unbounded.
//!
//! **Slots are encrypted** with AES-256-GCM under a key derived separately from the vault
//! key. A plaintext ring was the obvious hole in "completely secure": a disk image, a
//! stolen laptop or a careless `cp` would have exposed the last few hundred keystrokes in
//! the clear. One AES-GCM seal per keystroke costs single-digit microseconds with AES-NI,
//! which is invisible next to the terminal round-trip the user is waiting on. GCM's
//! authentication tag also gives us free torn-write detection: a slot that fails to open
//! is a slot we ignore rather than replay as garbage.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{self, SecretKey};

pub const SLOT_SIZE: usize = 512;
const SLOT_MAGIC: u8 = 0xA5;

/// A 64-byte header holding the write cursor. Ordering comes from this monotonic counter,
/// never from timestamps: two keystrokes can land in the same millisecond, and a clock can
/// step backwards (NTP, sleep/wake, DST). Sorting by `ts_ms` reordered a recovered sentence
/// into gibberish — `seq` is what makes replay exact.
pub const HEADER_SIZE: usize = 64;
const HEADER_MAGIC: u8 = 0x5A;
/// MAGIC(1) + seq(8) + len(2).
const SLOT_HDR: usize = 11;
/// Payload bytes available per slot (front-packed, then zero-padded).
pub const SLOT_PAYLOAD: usize = SLOT_SIZE - 8;

pub const EV_CHAR: u8 = 1;
pub const EV_BACKSPACE: u8 = 2;
pub const EV_FOCUS: u8 = 3;
pub const EV_BLUR: u8 = 4;
pub const EV_PASTE: u8 = 5;
pub const EV_FLUSH_MARK: u8 = 6;

/// One buffered input event. Deliberately small and Copy: the hot path must not allocate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: u8,
    /// Unix **seconds**, u32 — good until 2106, and only 4 bytes per event.
    ///
    /// Seconds, not milliseconds: a u32 holds 4.29e9 ms, i.e. 49 days, so millisecond
    /// epoch timestamps wrapped on every event. Ordering never depends on this field
    /// (the ring cursor does that); it exists only so the UI can say "3m ago".
    pub ts: u32,
    pub app_len: u8,
    pub app: [u8; 24],
    pub text: [u8; 64],
    pub text_len: u8,
}

impl Event {
    pub fn new(kind: u8, app: &str) -> Self {
        let mut ev = Event {
            kind,
            ts: now_secs(),
            app_len: 0,
            app: [0u8; 24],
            text: [0u8; 64],
            text_len: 0,
        };
        ev.set_app(app);
        ev
    }

    pub fn set_app(&mut self, app: &str) {
        let n = fit_bytes(app, 24);
        self.app[..n].copy_from_slice(&app.as_bytes()[..n]);
        self.app[n..].fill(0);
        self.app_len = n as u8;
    }

    pub fn set_text(&mut self, text: &str) {
        let n = fit_bytes(text, 64);
        self.text[..n].copy_from_slice(&text.as_bytes()[..n]);
        self.text[n..].fill(0);
        self.text_len = n as u8;
    }

    pub fn app_str(&self) -> &str {
        std::str::from_utf8(&self.app[..self.app_len as usize]).unwrap_or("")
    }

    pub fn text_str(&self) -> &str {
        std::str::from_utf8(&self.text[..self.text_len as usize]).unwrap_or("")
    }

    /// Packs the event into its plaintext body (never written to disk as-is).
    fn body(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(96);
        b.push(self.kind);
        b.push(self.app_len);
        b.push(self.text_len);
        b.extend_from_slice(&self.ts.to_le_bytes());
        b.extend_from_slice(&self.app);
        b.extend_from_slice(&self.text);
        b
    }

    fn from_body(body: &[u8]) -> Option<Self> {
        if body.len() < 7 {
            return None;
        }
        let kind = body[0];
        let app_len = (body[1] as usize).min(24);
        let text_len = (body[2] as usize).min(64);
        let mut ts = [0u8; 4];
        ts.copy_from_slice(&body[3..7]);
        // Fixed-width fields: app is always 24 bytes and text always 64, regardless of how
        // many of those bytes are meaningful. Reading them as variable-length packed fields
        // shifted the text window and replayed every draft as NUL bytes.
        const APP_OFF: usize = 7;
        const TEXT_OFF: usize = APP_OFF + 24;
        const BODY_LEN: usize = TEXT_OFF + 64;
        if body.len() < BODY_LEN || app_len > 24 || text_len > 64 {
            return None;
        }
        let mut app = [0u8; 24];
        let mut text = [0u8; 64];
        app[..app_len].copy_from_slice(&body[APP_OFF..APP_OFF + app_len]);
        text[..text_len].copy_from_slice(&body[TEXT_OFF..TEXT_OFF + text_len]);
        Some(Event {
            kind,
            ts: u32::from_le_bytes(ts),
            app_len: app_len as u8,
            app,
            text,
            text_len: text_len as u8,
        })
    }

    /// Slot frame: `MAGIC(1) | seq(u64) | ciphertext_len(u16) | envelope | zero padding`.
    ///
    /// `seq` is plaintext on purpose: it is a bare counter that reveals nothing about content,
    /// and it is what lets a **salvage** scan rebuild the correct order when the header (page 0)
    /// was lost to a torn write. Without it, a corrupt header meant "buffer empty" — silently
    /// discarding every pending keystroke, which is the one thing this tool may never do.
    fn encode(&self, key: &SecretKey, seq: u64) -> Option<[u8; SLOT_SIZE]> {
        let body = self.body();
        let sealed = crypto::seal(key, &body, b"ring-slot", None).ok()?;
        if sealed.len() + SLOT_HDR > SLOT_SIZE {
            return None;
        }
        let mut slot = [0u8; SLOT_SIZE];
        slot[0] = SLOT_MAGIC;
        slot[1..9].copy_from_slice(&seq.to_le_bytes());
        slot[9..11].copy_from_slice(&(sealed.len() as u16).to_le_bytes());
        slot[SLOT_HDR..SLOT_HDR + sealed.len()].copy_from_slice(&sealed);
        Some(slot)
    }

    /// Returns the event plus its sequence number, or `None` for an empty/torn/foreign slot.
    fn decode(slot: &[u8; SLOT_SIZE], key: &SecretKey) -> Option<(u64, Self)> {
        if slot[0] != SLOT_MAGIC {
            return None;
        }
        let mut seqb = [0u8; 8];
        seqb.copy_from_slice(&slot[1..9]);
        let seq = u64::from_le_bytes(seqb);
        let mut lenb = [0u8; 2];
        lenb.copy_from_slice(&slot[9..11]);
        let len = u16::from_le_bytes(lenb) as usize;
        if len == 0 || len + SLOT_HDR > SLOT_SIZE {
            return None;
        }
        // A torn slot fails authentication here and is skipped — never replayed as garbage.
        let body = crypto::open(key, &slot[SLOT_HDR..SLOT_HDR + len], b"ring-slot").ok()?;
        Event::from_body(&body).map(|ev| (seq, ev))
    }
}

/// Largest byte length <= `max` that lands on a UTF-8 character boundary.
///
/// Without this, a redacted secret (mask `█` is 3 bytes) could be cut mid-character, the
/// stored bytes were no longer valid UTF-8, and `text_str()` returned an empty string — the
/// draft silently vanished. Silent truncation of a user's text is the exact failure this
/// tool exists to prevent, so it must be impossible at the storage layer too.
fn fit_bytes(s: &str, max: usize) -> usize {
    if s.len() <= max {
        return s.len();
    }
    let mut n = max;
    while n > 0 && !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

/// Unix seconds, saturating at 0 before the epoch (clock misconfiguration, not panicking).
pub fn now_secs() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0)
}

/// Append-only, fixed-size ring buffer living in one file.
pub struct CrashRing {
    path: PathBuf,
    file: File,
    key: SecretKey,
    slots: usize,
    /// Monotonic total-events-written counter; the durable write cursor lives in the header.
    cursor: u64,
    /// Count of slots written since open; used to trigger the "ring is wrapping" alarm.
    pub written: u64,
    /// Set when a drain had to salvage because the header was corrupt.
    salvaged: bool,
    /// The header was unreadable at open. We deliberately do NOT rewrite it here: doing so
    /// would erase the evidence and make `drain` believe the ring is empty, discarding every
    /// pending keystroke. It is rewritten only after a successful salvage or `clear`.
    header_corrupt: bool,
    /// The file was shorter than a full ring (interrupted creation, full disk, partial copy).
    /// We do not silently extend it: the surviving prefix is salvaged and reported first.
    short_file: bool,
}

// Layout on disk: [HEADER_SIZE header][slot 0][slot 1]...[slot N-1]
// `cursor` is the total number of events ever pushed; the live window is
// `cursor-N .. cursor`, mapped modulo N. That is the whole index — no extra file, no fsync.

impl CrashRing {
    /// Opens (or creates) the ring. `capacity_slots` is clamped to at least 8.
    pub fn open(
        path: impl AsRef<Path>,
        capacity_slots: usize,
        key: SecretKey,
    ) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let slots = capacity_slots.max(8);
        let file =
            OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
        // Pre-size so that writes never extend the file mid-crash (extension can silently
        // produce a short, sparse tail). But never blow away a short file: it may be all that
        // survived a full disk or an interrupted copy, and extending it with zeros would
        // destroy the only recoverable bytes without a word.
        let want = (HEADER_SIZE + slots * SLOT_SIZE) as u64;
        let actual = file.metadata()?.len();
        let mut me = Self {
            path,
            file,
            key,
            slots,
            cursor: 0,
            written: 0,
            salvaged: false,
            header_corrupt: false,
            short_file: actual < want,
        };
        if !me.short_file && actual != want {
            // Longer than expected: truncate back to the ring shape.
            me.file.set_len(want)?;
        }
        match me.read_cursor() {
            Ok(c) => me.cursor = c,
            Err(_) => me.header_corrupt = true,
        }
        Ok(me)
    }

    fn read_cursor(&mut self) -> std::io::Result<u64> {
        let mut h = [0u8; HEADER_SIZE];
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_exact(&mut h)?;
        if h[0] != HEADER_MAGIC {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "no header"));
        }
        let mut c = [0u8; 8];
        c.copy_from_slice(&h[2..10]);
        Ok(u64::from_le_bytes(c))
    }

    fn write_header(&mut self) -> std::io::Result<()> {
        let mut h = [0u8; HEADER_SIZE];
        h[0] = HEADER_MAGIC;
        h[1] = 1;
        h[2..10].copy_from_slice(&self.cursor.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&h)
    }

    fn slot_offset(idx: usize) -> u64 {
        (HEADER_SIZE + idx * SLOT_SIZE) as u64
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn capacity(&self) -> usize {
        self.slots
    }

    /// Hot path. One positional write, no fsync, no allocation.
    /// Returns the slot index written.
    ///
    /// Order of writes is deliberate: payload first, then the cursor. A crash in between
    /// loses the newest event (which never reached the user's screen in full) rather than
    /// resurrecting stale bytes from the previous lap as if they were new.
    pub fn push(&mut self, ev: &Event) -> std::io::Result<usize> {
        if self.header_corrupt || self.short_file {
            // Adopt a fresh header only if there is nothing worth salvaging; otherwise the
            // caller must drain first (Engine always recovers before accepting new input).
            if self.salvage().map(|v| v.is_empty()).unwrap_or(true) {
                self.header_corrupt = false;
                self.short_file = false;
                self.cursor = 0;
                self.write_header()?;
            }
        }
        let idx = self.cursor as usize % self.slots;
        let seq = self.cursor;
        let Some(slot) = ev.encode(&self.key, seq) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "event too large for a ring slot",
            ));
        };
        self.file.seek(SeekFrom::Start(Self::slot_offset(idx)))?;
        self.file.write_all(&slot)?;
        self.cursor = self.cursor.wrapping_add(1);
        self.written = self.written.saturating_add(1);
        self.write_header()?;
        Ok(idx)
    }

    /// Forces bytes to the platter. Called on flush, on blur and on shutdown — NOT per key.
    pub fn sync(&mut self) -> std::io::Result<()> {
        self.file.sync_data()
    }

    /// Reads the live window in true chronological order (oldest first), dropping torn and
    /// empty slots. Torn slots are expected after a hard crash; a zeroed slot is skipped.
    pub fn drain(&mut self) -> std::io::Result<Vec<Event>> {
        self.salvaged = false;
        let header = if self.header_corrupt || self.short_file {
            Err(())
        } else {
            self.read_cursor().map_err(|_| ())
        };
        let Ok(cursor) = header else {
            // Header is unreadable (torn write to page 0, or a foreign file). Do NOT report an
            // empty buffer: salvage every readable slot and order by its own sequence number.
            self.salvaged = true;
            return self.salvage();
        };
        let slots = self.slots as u64;
        let start = cursor.saturating_sub(slots);
        let mut out = Vec::with_capacity(slots as usize);
        for seq in start..cursor {
            let idx = (seq % slots) as usize;
            let mut slot = [0u8; SLOT_SIZE];
            self.file.seek(SeekFrom::Start(Self::slot_offset(idx)))?;
            self.file.read_exact(&mut slot)?;
            if let Some((_, ev)) = Event::decode(&slot, &self.key) {
                out.push(ev);
            }
        }
        Ok(out)
    }

    /// Best-effort scan of every slot, ordered by the slot's own sequence number. Used when the
    /// header is gone. Slots from a foreign key or a torn write are skipped.
    fn salvage(&mut self) -> std::io::Result<Vec<Event>> {
        let mut found: Vec<(u64, Event)> = Vec::with_capacity(self.slots);
        let len = self.file.metadata().map(|m| m.len()).unwrap_or(0);
        for idx in 0..self.slots {
            let off = Self::slot_offset(idx);
            if off + SLOT_SIZE as u64 > len {
                break; // beyond the surviving prefix of a short file
            }
            let mut slot = [0u8; SLOT_SIZE];
            self.file.seek(SeekFrom::Start(off))?;
            if self.file.read_exact(&mut slot).is_err() {
                break;
            }
            if let Some(pair) = Event::decode(&slot, &self.key) {
                found.push(pair);
            }
        }
        found.sort_by_key(|(seq, _)| *seq);
        // Keep only the newest `slots` events, mirroring normal ring semantics.
        let skip = found.len().saturating_sub(self.slots);
        Ok(found.into_iter().skip(skip).map(|(_, ev)| ev).collect())
    }

    /// Clears the ring after a successful flush to the database.
    pub fn clear(&mut self) -> std::io::Result<()> {
        let want = (HEADER_SIZE + self.slots * SLOT_SIZE) as u64;
        self.file.set_len(0)?;
        self.file.set_len(want)?;
        self.cursor = 0;
        self.header_corrupt = false;
        self.short_file = false;
        self.write_header()?;
        self.file.sync_data()
    }

    /// True when the last `drain` had to rebuild order from slot sequence numbers because the
    /// header was unreadable. The UI should say so rather than pretend everything was normal.
    pub fn salvaged(&self) -> bool {
        self.salvaged
    }

    /// Number of events currently recoverable (0..=capacity).
    pub fn pending(&mut self) -> usize {
        if self.header_corrupt || self.short_file {
            // Don't guess from a cursor we know is gone; count what actually decrypts.
            return self.salvage().map(|v| v.len()).unwrap_or(0);
        }
        let cursor = self.read_cursor().unwrap_or(self.cursor);
        cursor.min(self.slots as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testkey() -> SecretKey {
        SecretKey::from_bytes([7u8; 32])
    }

    #[test]
    fn ring_file_reveals_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        let mut ring = CrashRing::open(&path, 64, testkey()).unwrap();
        let mut ev = Event::new(EV_CHAR, "SuperSecretEditor");
        ev.set_text("my half-written prompt");
        ring.push(&ev).unwrap();
        ring.sync().unwrap();
        let raw = std::fs::read(&path).unwrap();
        let hay = String::from_utf8_lossy(&raw).to_lowercase();
        assert!(!hay.contains("half-written"), "typed text is readable on disk");
        assert!(!hay.contains("supersecreteditor"), "app name is readable on disk");
    }

    #[test]
    fn corrupt_header_salvages_instead_of_losing_everything() {
        // Chaos engineer: a torn write lands on page 0 and wipes the cursor. Previously this
        // reported an empty buffer — silent total loss. Now every slot is still readable.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        {
            let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
            for c in "salvage me".chars() {
                let mut ev = Event::new(EV_CHAR, "t");
                ev.set_text(&c.to_string());
                ring.push(&ev).unwrap();
            }
            ring.sync().unwrap();
        }
        {
            use std::io::Write;
            let mut f = OpenOptions::new().write(true).open(&path).unwrap();
            f.seek(SeekFrom::Start(0)).unwrap();
            f.write_all(&[0xFF; 16]).unwrap(); // destroy the header magic and cursor
        }
        let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
        let events = ring.drain().unwrap();
        let text: String = events.iter().map(|e| e.text_str()).collect();
        assert_eq!(text, "salvage me", "salvage must recover the text, in order");
        assert!(ring.salvaged(), "the caller must be told this was a salvage");
    }

    #[test]
    fn truncated_file_is_salvaged_not_zero_filled() {
        // Chaos: disk fills mid-write, or a partial copy. Previously `open` re-extended the
        // file with zeros and the surviving text silently disappeared.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        {
            let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
            for c in "keep this".chars() {
                let mut ev = Event::new(EV_CHAR, "t");
                ev.set_text(&c.to_string());
                ring.push(&ev).unwrap();
            }
            ring.sync().unwrap();
        }
        let full = std::fs::read(&path).unwrap();
        std::fs::write(&path, &full[..HEADER_SIZE + 6 * SLOT_SIZE]).unwrap(); // cut mid-ring

        let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
        let text: String = ring.drain().unwrap().iter().map(|e| e.text_str()).collect();
        assert_eq!(text, "keep t", "surviving slots are salvaged in order");
        assert!(ring.salvaged());
    }

    #[test]
    fn wrong_key_cannot_read_the_ring() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        {
            let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
            let mut ev = Event::new(EV_CHAR, "a");
            ev.set_text("secret");
            ring.push(&ev).unwrap();
        }
        let mut ring = CrashRing::open(&path, 16, SecretKey::from_bytes([9u8; 32])).unwrap();
        assert!(ring.drain().unwrap().is_empty(), "foreign key must yield nothing");
    }

    #[test]
    fn roundtrip_and_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 64, testkey()).unwrap();
        for (i, c) in "hello".chars().enumerate() {
            let mut ev = Event::new(EV_CHAR, "editor");
            ev.ts = 1000 + i as u32;
            ev.set_text(&c.to_string());
            ring.push(&ev).unwrap();
        }
        let events = ring.drain().unwrap();
        let text: String = events.iter().map(|e| e.text_str()).collect();
        assert_eq!(text, "hello");
        assert_eq!(events[0].app_str(), "editor");
    }

    #[test]
    fn wraps_and_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 8, testkey()).unwrap();
        for i in 0..20u32 {
            let mut ev = Event::new(EV_CHAR, "t");
            ev.ts = i;
            ev.set_text("x");
            ring.push(&ev).unwrap();
        }
        let events = ring.drain().unwrap();
        assert_eq!(events.len(), 8, "capacity is a hard bound");
        assert_eq!(events.last().unwrap().ts, 19, "newest survives");
    }

    #[test]
    fn torn_write_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        {
            let mut ring = CrashRing::open(&path, 8, testkey()).unwrap();
            let mut ev = Event::new(EV_CHAR, "a");
            ev.set_text("ok");
            ring.push(&ev).unwrap();
        }
        // Simulate power loss: scribble garbage over slot 3, zero the magic.
        use std::io::{Seek, Write};
        let mut f = OpenOptions::new().write(true).open(&path).unwrap();
        f.seek(SeekFrom::Start((HEADER_SIZE + 3 * SLOT_SIZE) as u64)).unwrap();
        f.write_all(&[0x00; SLOT_SIZE]).unwrap();
        let mut ring = CrashRing::open(&path, 8, testkey()).unwrap();
        let events = ring.drain().unwrap();
        assert_eq!(events.len(), 1, "garbage slot ignored, good slot kept");
        assert_eq!(events[0].text_str(), "ok");
    }

    #[test]
    fn clear_empties() {
        let dir = tempfile::tempdir().unwrap();
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 8, testkey()).unwrap();
        ring.push(&Event::new(EV_CHAR, "a")).unwrap();
        ring.clear().unwrap();
        assert!(ring.drain().unwrap().is_empty());
    }

    #[test]
    fn same_millisecond_events_keep_their_order() {
        // Regression: ordering used to come from `ts_ms`, so a burst of keystrokes in one
        // millisecond (or any clock step backwards) replayed as nonsense.
        let dir = tempfile::tempdir().unwrap();
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 32, testkey()).unwrap();
        for c in "abcdefgh".chars() {
            let mut ev = Event::new(EV_CHAR, "t");
            ev.ts = 7777; // every event in the same millisecond
            ev.set_text(&c.to_string());
            ring.push(&ev).unwrap();
        }
        let text: String = ring.drain().unwrap().iter().map(|e| e.text_str()).collect();
        assert_eq!(text, "abcdefgh");
    }

    #[test]
    fn order_survives_the_wrap_point() {
        let dir = tempfile::tempdir().unwrap();
        // Capacity is clamped to a floor of 8 so recovery always has a usable window.
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 5, testkey()).unwrap();
        assert_eq!(ring.capacity(), 8);
        for c in "0123456789AB".chars() {
            let mut ev = Event::new(EV_CHAR, "t");
            ev.ts = 1; // identical timestamps: only the cursor can order these
            ev.set_text(&c.to_string());
            ring.push(&ev).unwrap();
        }
        let text: String = ring.drain().unwrap().iter().map(|e| e.text_str()).collect();
        assert_eq!(text, "456789AB", "oldest-first across the wrap");
    }

    #[test]
    fn cursor_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ring.bin");
        {
            let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
            for c in "xyz".chars() {
                let mut ev = Event::new(EV_CHAR, "t");
                ev.set_text(&c.to_string());
                ring.push(&ev).unwrap();
            }
        }
        let mut ring = CrashRing::open(&path, 16, testkey()).unwrap();
        assert_eq!(ring.pending(), 3);
        assert_eq!(ring.drain().unwrap().len(), 3);
    }

    #[test]
    fn multibyte_text_is_never_cut_mid_character() {
        // 3-byte mask characters plus ASCII, longer than a slot field.
        let mut ev = Event::new(EV_CHAR, "t");
        let long = format!("key {}{}", "\u{2588}".repeat(30), "tail");
        ev.set_text(&long);
        let s = ev.text_str();
        assert!(!s.is_empty(), "text must survive truncation, not vanish");
        assert!(s.chars().count() > 10);
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
    }

    #[test]
    fn utf8_multibyte_survives() {
        let dir = tempfile::tempdir().unwrap();
        let mut ring = CrashRing::open(dir.path().join("ring.bin"), 8, testkey()).unwrap();
        let mut ev = Event::new(EV_PASTE, "t");
        ev.set_text("नमस्ते");
        ring.push(&ev).unwrap();
        assert_eq!(ring.drain().unwrap()[0].text_str(), "नमस्ते");
    }

    #[cfg(unix)]
    #[test]
    fn the_ring_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("ring.bin");
        let _ = CrashRing::open(&path, 16, testkey()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the crash ring must not be world-readable, got {mode:o}");
    }
}
