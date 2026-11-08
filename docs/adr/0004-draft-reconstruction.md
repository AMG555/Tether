# ADR 0004 — Reconstructing text from an event stream

**Status:** accepted · **Date:** 2026-11-13

## Context

We store `Char` / `Backspace` / `Paste` / `Focus` / `Blur` events, not snapshots. Turning a
partial event stream back into "what the user was looking at" is where a recovery tool either
wins or corrupts silently.

## Decision

`recover::reconstruct` replays events with these rules:

1. `Char` appends. `Backspace` deletes the last **user-perceived character** (trailing
   combining marks first, then the base character — popping the base first leaves a floating
   accent and corrupts the next keystroke).
2. `Paste` inserts the whole string as **one** event. A 5,000-character paste must not consume
   5,000 ring slots and evict history.
3. `Focus`, `Blur`, and an app change are **boundaries**: a new field means a new draft.
4. The caret is assumed to be at the end. Documented and degrade-safe: a mid-string edit
   yields a plausible draft with every character present, possibly rearranged. Probe mode
   (accessibility) removes this assumption entirely where the OS allows it.
5. Any payload that may have been cut at the slot boundary sets `truncated`. The UI must show
   `[partial]`. We never present a known-short draft as complete.

## Session state vs. ring

The live session (in RAM) is the authority on current text; the ring is the crash net. On
flush the ring is cleared and re-seeded with the committed tail, and `engine::stitch` splices a
replayed tail onto the committed prefix:

```
prior    = "AAAA…nnnnnn"              (last 56 chars were seeded)
replayed = "nnnnnnXYZ"                (seed + new keystrokes)
stitched = prior[..len-56] + replayed = "AAAA…nnnnnnXYZ"
```

If the overlap doesn't match, both halves are kept. Duplicating a few words is recoverable;
dropping the user's sentence is not.

## Consequences

- Whitespace-only drafts are not saved (no value, and they'd pollute the list).
- Recovery is idempotent: a second `recover` finds an empty ring.
- Draft identity is `(app, session_start)`, hashed to a blind index, so repeated flushes refine
  one row and keep revisions instead of spawning duplicates.
