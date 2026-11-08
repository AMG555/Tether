# Architecture

## Shape

```
┌───────────────────────────────────────────────────────────────────────────┐
│ capture/            Source trait — one per platform                       │
│   windows.rs        WH_KEYBOARD_LL + ToUnicodeEx + UIA probe              │
│   macos.rs          CGEventTap + AXUIElement probe                        │
│   passive.rs        clipboard watch (Wayland/X11, and the fallback)       │
└───────────────────────────┬───────────────────────────────────────────────┘
                            │ Event { kind, ts, app, text }
                            ▼
┌───────────────────────────────────────────────────────────────────────────┐
│ engine.rs           THE ORDERING. The only module that knows the pipeline. │
│                                                                           │
│   ingest(ev):   policy.verdict → Drop?  stop, store nothing               │
│                 policy.sanitize → masked text                             │
│                 session.apply(masked)      ← live text, RAM only          │
│                 ring.push(sealed event)    ← crash net,            ~4 µs  │
│                                                                           │
│   flush():      session → vault.save_draft  (encrypted, WAL)              │
│                 ring.clear() + re-seed tail                               │
│                                                                           │
│   recover():    ring.drain → reconstruct → stitch(prior, replayed) → vault │
└──────────┬──────────────────────┬───────────────────────┬─────────────────┘
           │                      │                       │
           ▼                      ▼                       ▼
      ┌─────────┐          ┌─────────────┐         ┌──────────────┐
      │ policy  │          │    ring     │         │    vault     │
      │  what   │          │  encrypted  │         │  encrypted   │
      │ we keep │          │  512B slots │         │ SQLite WAL   │
      └────┬────┘          └──────┬──────┘         └──────┬───────┘
           └───────────┬──────────┴───────────────────────┘
                       ▼
                  ┌─────────┐
                  │ crypto  │  AES-256-GCM + HKDF (file/index/ring keys)
                  └─────────┘

      recover.rs — event stream → text (grapheme-aware, session boundaries)
```

## Why two storage layers

Every "don't lose my typing" tool has to answer one question: how often do you pay for durability?

- **Per keystroke, with fsync** — correct, and unusable. An fsync is 1–10 ms; at 8 keys/second you'd spend 8% of a core and hammer the disk for text the user hasn't finished writing.
- **Per keystroke, without fsync** — what Savepoint does in the ring. One `write(2)` into a page-cache-backed, pre-allocated file: ~4 µs. Survives process death (the kernel still has the page), loses only to power loss within the flush window.
- **Per second, with WAL commit** — what the vault does. ~1.3 ms, off the input thread entirely.

Neither layer alone is sufficient, and that's the point:

- Ring only → cannot represent "delete the last character of text I already committed". After a flush clears it, a backspace has no base string.
- Vault only → the 1-second window is exactly where a crash costs you the last sentence, which is the whole product.

So the ring is the crash net for the last ~100 keystrokes, the session (RAM) is the authority on what the text currently *is*, and on flush the ring is re-seeded with the last 56 characters of committed text. A crash then replays "the tail I last saw, plus what came after", and `engine::stitch` splices them. Both halves are covered by tests.

## Storage layout

```
~/.local/share/savepoint/                                %LOCALAPPDATA%\dev.savepoint.savepoint\       ~/Library/Application Support/dev.savepoint.savepoint/
├── vault.db     SQLite, WAL mode, mode 0600
├── ring.bin     64-byte header + N×512-byte encrypted slots, mode 0600
├── salt.bin     16 random bytes (HKDF salt), mode 0600
└── key.txt      64 hex chars = the master key, mode 0600  ← back this up
```

Directory itself is mode 0700: the file listing alone would otherwise reveal that a vault exists.

### vault.db

```sql
meta(key TEXT, value BLOB)                 -- schema version, key verifier

drafts(
  id         INTEGER PRIMARY KEY,
  field_id   TEXT,      -- HMAC-SHA256(index_key, app|start_ts)  ← blind index
  created_at INTEGER,   -- unix seconds
  updated_at INTEGER,
  chars      INTEGER,
  redactions INTEGER,
  truncated  INTEGER,   -- 1 = ring wrapped, text is the newest tail
  revisions  INTEGER,
  sealed     BLOB       -- "SPV1" || nonce(12) || AES-256-GCM(json{app,text})
)

revisions(id, draft_id, updated_at, sealed)  -- prior versions, same sealing
```

What's in plaintext: row count, timestamps, character counts, a 32-byte HMAC. What isn't: the text, the app name, the window title. `field_id` is a blind index so the tool can ask "does this field already have a rescue?" without storing the field's identity — the same trick password-breach services use to search without learning your password.

### ring.bin

```
offset 0     header:  magic(1) version(1) cursor(u64) padding
offset 64    slot 0:  magic(1) len(u16) AES-256-GCM(body) padding
offset 576   slot 1:  ...
```

`cursor` is a monotonic count of events ever pushed; the live window is `cursor-N .. cursor`, mapped modulo N. Ordering therefore comes from the cursor, **not** from timestamps — two keystrokes can share a millisecond and a clock can step backwards, and sorting by time reordered a recovered sentence into gibberish (there's a regression test).

Write order per keystroke is payload-then-cursor, so a crash in between loses the newest event rather than resurrecting stale bytes from the previous lap as if they were new.

## Threat model

Trust boundaries, and what crosses them:

| Boundary | Attacker capability | Mitigation |
|---|---|---|
| OS → capture | A malicious app can name itself "1Password" to blind us, or forge window titles | Policy matches normalized process names case-insensitively; an unresolvable app identity is treated as unsafe (`Drop`) |
| Capture → disk | Full disk read (stolen laptop, careless backup) | AES-256-GCM on both ring and vault; app names and titles encrypted too; 0600/0700 |
| Disk → process | Disk-level tampering with ciphertext | GCM authentication: tampered rows fail closed, never decrypt to garbage |
| Process → user | Someone else at an unlocked machine | Out of scope by design (see limitations) |
| Anything → network | Exfiltration | No network code exists; a test fails the build if a network-capable dependency appears |

STRIDE pass, condensed: **S**poofing — app identity is distrusted, not trusted; **T**ampering — GCM tags everywhere; **R**epudiation — plaintext export refuses without `--i-understand`; **I**nformation disclosure — secrets never stored, text encrypted, metadata bounded; **D**oS — bounded ring, capped session (256 KB), `busy_timeout`, no unbounded growth from a hostile input source; **E**levation — never asks for root, no privileged helper, no setuid.

## Module interfaces

Deep modules: plenty of behaviour behind a small surface.

| Module | Interface | Implementation it hides |
|---|---|---|
| `crypto` | `derive_keys`, `seal`, `open`, `blind_index`, `SecretKey` | Envelope framing, nonce generation, HKDF purposes, HMAC-SHA256, zeroization |
| `policy` | `verdict`, `sanitize`, `is_blocked_*` | 25 app patterns, 16 title patterns, Luhn, 8 token patterns, allocation-free matching |
| `ring` | `open`, `push`, `drain`, `clear`, `pending`, `capacity` | Fixed-slot framing, cursor header, encryption, torn-write skipping |
| `recover` | `reconstruct`, `Builder`, `pop_one`, `Draft` | Grapheme-aware backspace, session boundaries, app switching, truncation flags |
| `vault` | `save_draft`, `list`, `get`, `search`, `revisions`, `prune`, `delete`, `purge_all` | WAL, blind index, dedupe-on-plaintext, vacuum |
| `engine` | `ingest`, `flush`, `recover`, `save_snapshot`, `live_text` | **Ordering**, session/ring/vault reconciliation, tail seeding, stitching |

The `Source` trait is the platform seam — two methods (`name`, `run`) and one optional (`probe`) — so a new backend is a small adapter, not a fork.

## Failure modes, and what each one costs

| Failure | Data lost | Why |
|---|---|---|
| App crash (`SIGSEGV`, OOM) | nothing | ring survives, `recover` replays |
| Force quit / `SIGKILL` | nothing | ring survives |
| Kernel panic / power loss | ≤ 1 flush window (~1s) unless OS flushed the page | ring is page-cache buffered, no fsync per key |
| Ring wraps (>4096 keys before a flush) | text older than the ring | draft is flagged `truncated`, shown as `[partial]` — never silently short |
| Vault corrupted | everything after last backup | `doctor` runs `PRAGMA quick_check`; `backup` is one command |
| Key file lost | everything | stated at `init`; the key is printed once and stored 0600 |

The governing rule: **degrade visibly, never silently.** A truncated draft says `[partial]`. A redacted secret says "3 secrets hidden". A missing permission says which System Settings pane to open. A tool that quietly returns 80% of your sentence is worse than one that tells you it lost 20%.
