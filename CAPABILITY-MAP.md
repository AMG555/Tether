# Capability Map: Tether Desktop App

Status: approved by the user on 2026-11-13.

The desktop app combines independently testable capabilities. Stable module IDs and dependencies:

| Module ID | Responsibility | Depends on |
|---|---|---|
| `desktop-shell` | Cross-platform app window, local UI loading, lifecycle, and a narrow Rust/UI bridge; no external browser | - |
| `pet-experience` | Visible 2D pet runtime and in-app procedural sprite/WebP clip customization, reusing existing pet and clip validation | `desktop-shell` |
| `recovery-ui` | Saved-draft list, search, view, copy, and recovery backed by the existing encrypted vault | `desktop-shell` |
| `capture-lifecycle` | Capture start/stop/status and user-opted-in login launch, preserving privacy and a clear capture indicator | `desktop-shell` |
| `installers` | Per-OS build/install packages and install/uninstall behavior; verify the integrated app | `desktop-shell`, `pet-experience`, `recovery-ui`, `capture-lifecycle` |

Build order: `desktop-shell` -> `pet-experience`, `recovery-ui`, and `capture-lifecycle` (in parallel) -> `installers`.

## Boundaries

- Keep the existing Rust library and encrypted vault as the local source of truth; retain the CLI.
- Keep the app offline and local-only. Do not upload drafts or pet files.
- The UI uses its own app window, not an external browser.
- Version 1 includes procedural sprites and animated WebP clip packs; 3D avatars and cloud sync are out of scope.
- Capture may start at login only after the user opts in; the UI must clearly show when capture is active.

The repository currently has reusable GUI prototypes and Rust pet, clip, capture, and vault modules, but no native desktop shell or desktop installer.
