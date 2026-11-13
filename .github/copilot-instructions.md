# Project: Tether

## Tech stack
- Rust 1.75+
- Cargo workspace at `savepoint/`
- SQLite via `rusqlite` with bundled libsqlite3
- CLI-first app with platform capture backends for Windows, macOS, and passive clipboard on Linux

## Commands
- Build: `cargo build --release --quiet`
- Test: `cargo test --quiet`
- Preferred focused check: `cargo test --quiet` before shipping changes

## Conventions
- Keep platform-specific logic behind `Source` and the capture module boundaries.
- Use `Arc` for thread-shared callback/sink ownership; closures moved across threads must be owned and thread-safe.
- Preserve the product contract: no silent data loss, block secret fields before they reach disk, and keep crash recovery deterministic.
- Prefer small, focused changes; the project already has end-to-end recovery tests.

## Boundaries
- Do not remove the active project files under `savepoint/` unless they are clearly stale artifacts.
- Keep `README.md`, release assets under `shots/`, and the `tests/` suite aligned when changing behavior.
- Avoid network requests in offline tooling or authoring flows.
- Do not add runtime dependencies without a direct need.
- Read `CAPABILITY-MAP.md`, `CONSTRAINTS.md`, and the active `SPEC-*.md` before changing the desktop app; do not weaken quality gates to make a check pass.
