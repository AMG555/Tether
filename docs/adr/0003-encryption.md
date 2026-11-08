# ADR 0003 — AES-256-GCM with HKDF-separated keys per purpose

**Status:** accepted · **Date:** 2026-11-13

## Context

The user's requirement was "completely secure" plus "fully offline". Both the vault and the
crash ring need confidentiality at rest, tamper detection, and no key-management UI.

## Decision

AES-256-GCM. One 256-bit master key, three purpose-derived keys via HKDF-SHA256
(`rescue/v1/file`, `/index`, `/ring`). Random 96-bit nonce per record, stored in the
envelope. Associated data binds each record to its own metadata.

```
file_key   ── AES-GCM ── draft.app + draft.text
index_key  ── HMAC-SHA256 ── field identity → blind index (searchable without plaintext)
ring_key   ── AES-GCM ── hot-path keystroke slots (separate key: hot path is more exposed)
```

## Why GCM

- **Authenticated.** Confidentiality without integrity is worse than none for a crash-recovery
  tool: a tampered row must fail closed, never decrypt to plausible garbage. `open()` returns
  an error, the UI shows `<undecryptable>`, nothing is overwritten.
- **Hardware accelerated.** AES-NI/ARMv8 makes it ~0.4–1.2 µs for our payload sizes.
- **AAD binds metadata.** An attacker with write access to the DB cannot move a ciphertext
  under a different field or timestamp label and have it decrypt.

### Rejected alternatives

| Option | Why not |
|---|---|
| ChaCha20-Poly1305 | Equally fine; AES-GCM chosen for hardware acceleration on the platforms that matter for a background daemon |
| XChaCha20 (24-byte nonce) | Longer nonce safety margin, but 12-byte random nonces are safe to 2^32 records and we rotate per record; not worth losing AES-NI |
| Age / sealed-box style | Adds a file-per-record model we don't need |
| Password-based (Argon2) only | Adds a prompt that breaks unattended capture; can be layered later (roadmap) |

## Nonce discipline

96-bit random per record from the OS CSPRNG. Birthday-bound risk reaches 2^-32 at ~2^32
records (≈4 billion rescues); at one commit/second that is 136 years. A counter-based nonce
was rejected because it needs durable state, and durable state is what we're trying to avoid on
the hot path.

## Deliberate non-goals

- **No padding.** Record sizes leak approximate length. Fixed-rate padding would multiply
  storage for a metadata-only gain; noted in `docs/limitations.md`.
- **No metadata encryption.** Row counts and timestamps are plaintext — 64-bit counters can't
  be usefully hidden without traffic shaping.
- **Key in a file, not a keychain (v1).** Mode 0600 `key.txt`. OS keychain integration is the
  top roadmap item; it changes no file format bytes.
