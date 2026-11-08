# ADR 0001 — SQLite (WAL) for the vault

**Status:** accepted · **Date:** 2026-11-13

## Context

We need durable, queryable storage for encrypted drafts with revisions, retention, deletion
and search, on three platforms, in a binary small enough to forget about.

## Decision

Bundle SQLite (`rusqlite`, `bundled`) in WAL mode with `synchronous=NORMAL`.

## Why

- **Crash semantics are already solved.** WAL + atomic commit is a decade of hardening we
  would otherwise reimplement badly. A JSON-lines or custom-log format needs its own
  torn-write recovery, which is exactly the failure mode this product is about.
- **One file, no daemon.** TypeScript/Python/Electron options bring a runtime (50–150 MB);
  we ship 2.0 MB with SQLite compiled in. Nothing to install on the user's machine.
- **`VACUUM` gives real deletion.** For a privacy tool, "delete" must mean the bytes are gone
  from free pages, not just unlinked from a B-tree.
- **Migrations are cheap.** `PRAGMA user_version`.

### Rejected alternatives

| Option | Why not |
|---|---|
| Encrypted JSONL append log | Needs hand-rolled torn-write recovery and compaction; O(n) reads |
| sled / redb (pure-Rust KV) | Faster startup, but no SQL for retention/search and less battle-tested crash behaviour |
| SQLCipher | Would move encryption into the engine, but adds a native build dep and we already need per-record AAD + a blind index; AES-GCM per row is more flexible here |
| Whole-file encrypted blob | Any crash mid-write risks the entire history, which is unacceptable for a crash-safety tool |

## Consequences

- `synchronous=NORMAL` can lose the last commit on power loss (not on process crash). The
  crash ring covers that window, which is precisely why the ring exists.
- SQLite keeps some metadata in plaintext (page count, table sizes). Accepted; documented in
  `docs/limitations.md`.
