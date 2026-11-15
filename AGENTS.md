# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Stack
Rust 1.75+, single-crate binary + lib, bundled SQLite (no system deps), no async runtime (zero tokio/async-std). Release binary target: ≤2.75 MiB.

## Commands
```bash
cargo test                              # all tests
cargo test --test crash_recovery        # single integration test file
cargo test stitch                       # single test by name (runs all matching)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo deny check licenses               # supply-chain gate (requires cargo-deny)
node --test gui/clip-player-math.test.js   # JS math tests (node 22+)
node --test gui/avatar-math.test.js
node --test gui/pages.test.js
```
CI also runs `RUSTFLAGS=-D warnings` — clippy warnings are build failures.

## Architecture
Pipeline (in order): `Policy.verdict` → `Policy.sanitize` → `CrashRing.push` (µs, no fsync) → periodic `Engine.flush` → `Vault` (AES-256-GCM, WAL SQLite).  
`engine.rs` is the **only** module that knows this ordering. All others are deep modules with small interfaces.

Data directory: `~/.local/share/tether/` (Linux), `%LOCALAPPDATA%\dev.tether.tether` (Windows), `~/Library/Application Support/tether` (macOS). Set `TETHER_HOME` env var to override in tests.

## Critical invariants — breaking these is a release blocker
1. **`Policy::sanitize` must run before any byte reaches `CrashRing` or `Vault`.** Redaction runs twice on purpose: once per-event (catches pasted secrets) and once on the assembled session at commit (catches secrets typed one char at a time — per-event alone misses `s`+`k`+`-`+`p`+`r`+`o`+`j`).
2. **Never add a network-capable dependency.** `deny.toml` bans reqwest/hyper/ureq/curl/openssl/native-tls. `tests/privacy.rs::dependencies_are_offline_only` and `shipped_source_contains_no_network_calls` are static checks that will catch it.
3. **Vault fails closed on wrong key** (`VaultError::WrongKey`) — never silently reinitialise, that is data loss.
4. **`delete` must VACUUM** — SQLite leaves deleted bytes in free pages; for a privacy tool that is a real leak.

## Error handling pattern
Errors are domain enums implementing `From<rusqlite::Error>`, `From<std::io::Error>`, etc., returned as `Result<_, VaultError>`. Never swallow errors from the commit loop — log once per distinct message so the user knows they are unprotected.

## Code style (from `rustfmt.toml` + project conventions)
- `max_width = 100`, `use_small_heuristics = "Max"`, edition 2021.
- Comments explain **why** (the design reason), not what. If removing the comment would make the code puzzling to a reviewer, it stays.
- Prefer deep modules: small public interface, all complexity inside. Avoid cross-module coupling beyond the documented dependency direction.
- No new runtime dependencies without a direct need and a PR justification.
- Public items get doc comments; private ones get them when behavior would otherwise puzzle a reviewer.
- `unwrap_or_else(|e| e.into_inner())` is the standard pattern for `Mutex::lock` (poison recovery).

## Testing conventions
- Integration tests live in `tests/` and use `tempfile::tempdir()` for isolation — never a fixed path.
- Unit tests live in `#[cfg(test)] mod tests` at the bottom of each source file.
- **A crash test must actually crash** (drop without flush, or `--crash` flag) — never test recovery via graceful shutdown only.
- New behaviour must have a test that **fails without the change**.
- `tests/privacy.rs` asserts bytes-on-disk properties — always pass it after any storage change.
- `tests/design.rs` enforces the visual identity; prohibited: `linear-gradient(180deg/135deg)`, `radial-gradient`, `backdrop-filter`, violet hues (`#a78bfa`, `#8b5cf6`, `#7c3aed`), border-radius ≥ 12px, any external URL (`https://`, `//fonts.`, `cdn.`).

## GUI / frontend
Shipped pages: `gui/clip-studio.html`, `gui/clip-preview.html`, `assets/pet-studio.html`. All must load `gui/theme.css` (tokens: `--paper`, `--signal`, `--mono`, `--radius`). Paper tone `#f2efe6` is the identity colour. No network references in any shipped HTML/CSS.

## ADRs
Non-obvious decisions are in `docs/adr/`. Read the relevant ADR before changing storage (`0001`), crash safety (`0002`), encryption (`0003`), or draft reconstruction (`0004`).

## Desktop app — non-obvious rules (`desktop/`)
- **New Tauri `#[tauri::command]` functions must be registered in THREE places**: `commands.rs` (`#[tauri::command]` annotation), `lib.rs` (`invoke_handler!` macro), **and `build.rs`** (`AppManifest::commands(&[…])`). The `build.rs` list is what generates `allow-*` permissions for capabilities — omitting it causes a build-time panic with "Permission allow-X not found".
- **Window list in `tauri.conf.json` is authoritative**. Do NOT also create windows in `.setup()` via `WebviewWindowBuilder` — Tauri creates them from config automatically. Duplicate creation causes the duplicate window to be destroyed, reducing the window count and triggering `ExitRequested`.
- **All Tauri IPC must go through `desktop/src/api.js`**. Never use `window.__TAURI__` directly in any other JS file — the architecture test (`desktop/test/architecture.test.js`) scans for `__TAURI__` regex across all `.js` sources, **including comments**.
- **`desktop/src-tauri/capabilities/main.json`** must have `windows: ["main"]` only and no `core:*` permissions — the shell-config test enforces this. Put companion-overlay permissions in a separate `companion-overlay.json` capability.
- **The desktop app IS the capture engine** — it starts `WH_KEYBOARD_LL` in-process via `.setup()` in `lib.rs`. `AppState` (held as Tauri managed state) owns the `Arc<Engine>`, the `stop` atomic, and the `capturing` flag. `commands.rs` opens the vault independently (read-only, per-call) rather than holding the Engine lock.
- **The CLI binary (`tether run`) and the desktop app write to the same vault** (`%LOCALAPPDATA%\dev.tether.tether\`). Do not run both simultaneously or they will race on the ring file.
- Run `cd desktop && npm test` for JS tests; `cargo test --manifest-path desktop/src-tauri/Cargo.toml` for Rust tests.

## Before shipping
Read `CAPABILITY-MAP.md`, `CONSTRAINTS.md`, and any active `SPEC-*.md` before changing the desktop app. Do not weaken quality gates to make a check pass.
