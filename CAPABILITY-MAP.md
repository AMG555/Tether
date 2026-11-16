# Capability Map: Tether Desktop App

Status: approved by the user on 2026-11-13. Last updated: 2026-11-16 (v0.1.20).

The desktop app combines independently testable capabilities. Stable module IDs and dependencies:

| Module ID | Responsibility | Depends on | Status |
|---|---|---|---|
| `desktop-shell` | Cross-platform app window, local UI loading, lifecycle, and a narrow Rust/UI bridge; no external browser | - | ✅ Shipped |
| `pet-experience` | Visible 2D pet runtime and in-app procedural sprite/WebP clip customization, reusing existing pet and clip validation | `desktop-shell` | ✅ Shipped |
| `companion-overlay` | Transparent full-screen overlay window; animated WebP/procedural character; free 2D drag (OS passthrough toggling); live capture-status reactions; 8-state machine (idle/walk/wave/water/stretch/happy/sad/eyes) | `desktop-shell`, `pet-experience` | ✅ Shipped |
| `recovery-ui` | Saved-draft list, search, view, copy, and recovery backed by the existing encrypted vault | `desktop-shell` | ✅ Shipped |
| `capture-lifecycle` | Capture start/stop/status and user-opted-in login launch, preserving privacy and a clear capture indicator | `desktop-shell` | ✅ Shipped |
| `installers` | Per-OS build/install packages and install/uninstall behavior; verify the integrated app | `desktop-shell`, `pet-experience`, `recovery-ui`, `capture-lifecycle` | ✅ Shipped |

Build order: `desktop-shell` → `pet-experience`, `recovery-ui`, `capture-lifecycle`, and `companion-overlay` (in parallel) → `installers`.

## Boundaries

- Keep the existing Rust library and encrypted vault as the local source of truth; retain the CLI.
- Keep the app offline and local-only. Do not upload drafts or pet files.
- The UI uses its own app window, not an external browser.
- Version 1 includes procedural sprites and animated WebP clip packs; 3D avatars and cloud sync are out of scope.
- Capture may start at login only after the user opts in; the UI must clearly show when capture is active.
- The companion overlay window uses OS-level `set_ignore_cursor_events` for click-through; passthrough is temporarily disabled during drag so fast moves outside the hit-zone are not lost.

## Shipped (v0.1.20)

The repository has a fully functional Tauri 2 native desktop shell, 32 bundled companion packs, an encrypted vault UI, capture lifecycle controls, and a floating companion overlay. All five capability modules are live and tested.

v0.1.20 adds a local `SessionBrain` to the companion overlay: keystroke-velocity tracking, context-aware state weights, session-time and idle-time escalation, milestone celebrations (burst typing, 25/60-min sessions, draft saves), daily streak counter, and 25 new unit tests in `desktop/test/companion-brain.test.js`. Zero network calls; zero new dependencies.
