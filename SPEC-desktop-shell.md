# Spec: Desktop Shell

Capability ID: `desktop-shell`  
Status: approved by the user  
Capability map: [`CAPABILITY-MAP.md`](CAPABILITY-MAP.md)

## Objective

Add a small, installable desktop host for Tether that opens its own app window and loads bundled local UI. The shell supplies the shared foundation for the pet, recovery, and capture-lifecycle capabilities. It must not open an external browser or expose the vault key or arbitrary local files to the UI.

The existing Rust library and CLI remain supported. The desktop app calls the same Rust core locally and keeps the existing encrypted data directory; it does not create a second vault or send data over the network.

## Tech Stack

- Tauri 2 for the native desktop window and Rust/JavaScript bridge.
- Plain HTML, CSS, and JavaScript ES modules for the frontend; avoid a frontend runtime framework and reuse existing GUI assets where appropriate.
- A separate desktop Rust crate (under `desktop/src-tauri/`) with the existing root crate as a path dependency. Keep the core crate's Rust 1.75 MSRV; the desktop wrapper uses Rust 1.90, which is required by the current Tauri source and its autostart plugin.
- Use the operating system webview rather than bundling a browser engine. This keeps the application small, but Linux packages require a compatible WebKitGTK runtime.
- The existing `src/setup.rs::install_autostart` writes entries that launch the CLI's `run` command. It must not be used unchanged for the desktop executable. Login launch belongs to `capture-lifecycle`, behind explicit user opt-in.

The Tauri documentation describes the framework as using each platform's native webview and supporting Rust commands from the frontend. The shell will use this bridge only for explicit app operations, with per-window capabilities and no remote origins.

## Commands

After the desktop package exists, its scripts must support:

```powershell
cargo test --quiet
node --test gui/avatar-math.test.js gui/clip-player-math.test.js gui/pages.test.js
npm --prefix desktop test
npm --prefix desktop run tauri:dev
npm --prefix desktop run tauri:build
```

The desktop package scripts must map to the Tauri CLI's `dev`, `build`, and frontend test/build commands. Native package builds must run on their target OS. The current environment does not have Cargo or rustc on `PATH`, so a desktop build cannot be verified here until the Rust toolchain and platform prerequisites are available.

## Project Structure

```text
src/                         Existing Rust core and CLI; remains usable independently
gui/                         Existing pet/clip renderers, authoring pages, and Node tests
desktop/
  package.json               Desktop frontend and Tauri CLI scripts
  src/
    index.html               Bundled app entry point
    main.js                  App UI
    api.js                   Typed-command adapter
    styles.css               Local app styling
  src-tauri/
    Cargo.toml               Tauri wrapper and path dependency on the core crate
    src/                     App state and narrowly scoped Rust commands
    capabilities/            Least-privilege permissions per window
    tauri.conf.json          Local asset and window configuration
```

The shell owns app startup, the primary app window, local asset loading, and the command bridge. `frontendDist` points directly to `desktop/src/`, keeping frontend assets separate from package files, dependencies, and Rust sources. `pet-experience` owns the pet overlay window and its rendering; this shell spec does not implement pet behavior or recovery screens.

## Code Style and Interface

- Rust uses the existing `rustfmt.toml` and snake_case; serialized UI fields use camelCase.
- Frontend code follows the existing ES module style and routes every Rust call through one app API adapter.
- Commands accept typed request structs and return typed response structs. Errors reject the frontend promise with a consistent serializable error; do not return empty success-shaped fallbacks.
- Async commands are used for work that could stall the UI.

Example contract shape:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    vault_ready: bool,
    capture_active: bool,
}
```

The UI may receive draft text only in response to an explicit view/copy action owned by `recovery-ui`. The master key, raw vault object, and unrestricted filesystem access never cross the bridge.

## Testing Strategy

- Preserve and run the existing Rust and Node test suites.
- Unit-test shell command adapters and error serialization without requiring a window.
- Test that bundled UI has no external network references and that Tauri capabilities grant only the required commands to the intended windows.
- Enforce the offline boundary in the webview policy and verify the shell starts and its local command bridge works with network access disabled; a static scan alone is not sufficient.
- Keep the shell app-command allowlist explicit (`get_app_info` only for this capability) using Tauri's build-time app command manifest; test it separately from Tauri plugin permissions. The shell registers no plugins and grants no plugin permissions.
- The desktop shell owns startup/bridge logging. Log only event names and error categories, never command payloads or results; test that diagnostic output excludes draft/key sentinel values.
- Verify the core crate independently with Rust 1.75 and the Tauri wrapper with Rust 1.90 or newer.
- Smoke-test the app window and local asset loading with `tauri:dev` on each supported OS. A successful Linux build must also document the WebKitGTK runtime requirement.
- Before release, run the blocking checks in `CONSTRAINTS.md`, record package size/startup/memory baselines per OS, and verify those checks fail on an injected violation and pass on a clean build.

## Boundaries

- **Always:** Keep UI assets local; keep raw keys and drafts out of logs; expose only explicit typed commands; use existing validation and encrypted storage; make failures visible.
- **Ask first:** Change the core data directory or vault format, raise the core crate's MSRV, add runtime dependencies beyond the Tauri host, enable remote origins or broad filesystem/shell permissions, or change the agreed capture opt-in behavior.
- **Never:** Open an external browser for app features; expose the master key to JavaScript; grant arbitrary filesystem or process execution to the UI; send drafts or pet files over the network; start capture without the user's login opt-in.

## Success Criteria

1. The desktop host opens as its own application window on Windows, macOS, and Linux; it does not launch a browser.
2. The frontend is bundled locally and has no network dependency.
3. Rust/UI communication uses typed, narrowly scoped commands; the UI cannot read arbitrary files or invoke arbitrary processes.
4. The desktop wrapper uses the existing core and data directory without changing the existing CLI contract or core crate's Rust 1.75 MSRV.
5. The app does not bundle a browser engine; actual package size and startup measurements are recorded per OS before setting numeric budgets.
6. Shell tests and security/architecture checks pass before this capability is considered complete.

## Open Questions

- Minimum OS versions and supported Linux package formats belong to the `installers` spec.
- Package-size and startup limits will be set from the first measured per-OS baseline, not guessed in this spec.

## Sources

- Tauri's official [overview](https://v2.tauri.app/start/) describes its small-binary approach and use of system webviews.
- Official [webview version documentation](https://v2.tauri.app/reference/webview-versions/) identifies WebView2 on Windows and WebKit on macOS/Linux.
- Official [Rust/Frontend command documentation](https://v2.tauri.app/develop/calling-rust/) defines typed command arguments, return values, errors, and async execution.
- Official [capabilities documentation](https://v2.tauri.app/security/capabilities/) describes per-window permissions and the security boundary for frontend access.
- The same [capabilities documentation](https://v2.tauri.app/security/capabilities/) explains that registered commands are available to all windows by default and documents `AppManifest::commands` for restricting the command set.
- Official [prerequisites](https://v2.tauri.app/start/prerequisites/) document the platform build dependencies.
- The [Tauri repository manifest](https://raw.githubusercontent.com/tauri-apps/tauri/dev/Cargo.toml) currently lists Tauri 2.12.5 and Rust 1.90; the version must be locked in the desktop manifest when implementation begins.
