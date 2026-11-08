# ADR 0002 — A bounded, encrypted, append-only ring in front of the vault

**Status:** accepted · **Date:** 2026-11-13

## Context

An fsync per keystroke is 1–10 ms and would make this tool unusable. Committing once a second
leaves a one-second window where a crash loses the last sentence — which is the exact bug
we're fixing.

## Decision

Every keystroke is written to a pre-allocated, fixed-slot, AES-256-GCM-encrypted ring file
with a single `write(2)` and **no fsync**. A background thread drains the ring into the vault
every ~1s, then clears it and re-seeds it with the last 56 characters of committed text.

## Details that matter

- **Pre-allocated size.** The file is sized at open, so writes never extend it. Extension can
  silently produce a short/sparse tail on crash.
- **Cursor in a header, not timestamps.** Ordering is `cursor mod slots`. Two keystrokes can
  share a millisecond and wall clocks step backwards; sorting by time reordered recovered
  sentences into gibberish (regression test: `ring::tests::same_millisecond_events_keep_their_order`).
- **Payload before cursor.** A crash between the two loses the newest event (which never fully
  reached the user's screen) rather than replaying stale bytes from the previous lap.
- **Encrypted slots.** A plaintext ring made "completely secure" false: a disk image would have
  exposed the last few hundred keystrokes. One GCM seal is ~1 µs. GCM tags also give free
  torn-write detection — a slot that fails to open is skipped, never replayed as garbage.
- **Bounded.** 4096 slots ≈ 64 KB default. A hostile or noisy input source cannot grow it, and
  this runs inside an OS hook where unbounded growth is unacceptable.
- **Tail re-seeding.** After a flush the ring holds `FOCUS` + `PASTE(tail)`. Without this, a
  backspace after a commit has no base string to delete from and the next commit *appends*
  instead of correcting (`engine::stitch` plus its seam tests exist for this).

## Consequences

- Power loss (not process death) can lose up to one flush window; documented.
- Wrapped rings set a `truncated` flag that the UI must show. Silent shortening is forbidden.
