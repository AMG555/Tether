# Implementation Plan: Tether Desktop Shell

Status: approved by the user; implementation in progress  
Scope: `desktop-shell` from [`../SPEC-desktop-shell.md`](../SPEC-desktop-shell.md). This plan does not implement the pet, recovery UI, capture lifecycle, or installers.

## Overview

Create the first native desktop shell for Tether using Tauri 2, a bundled local HTML/CSS/JavaScript UI, and the existing Rust core as a path dependency. The shell must remain usable without network access, expose only a single typed app-information command, and preserve the existing CLI, vault, and core Rust 1.75 MSRV.

## Architecture decisions

- Keep the desktop wrapper in `desktop/src-tauri/`, separate from the existing core crate. Build/test the core on Rust 1.75 and the wrapper on Rust 1.90 or newer.
- Bundle frontend assets and use the platform webview. Enforce local-only connections in the webview policy; no remote origins or external browser features.
- The only shell app command is `get_app_info`, returning non-sensitive app metadata. Register no Tauri plugins and grant no plugin permissions. Keep frontend invocation behind one typed adapter.
- The desktop shell owns startup and bridge diagnostics. Log event names/error categories only; never log command arguments, command results, drafts, or keys. Add a test that feeds secret-like sentinel values through the diagnostic/error path and asserts they are absent from captured output.
- Do not add a generic filesystem, shell, HTTP, opener, or autostart bridge. Later capabilities own their commands and permissions.
- Existing GUI and core tests remain part of verification. Do not claim a gate passes when its checker is missing or has not been run.

## Task list

### Phase 1: Baseline and foundation

#### Task 1: Establish test baselines and desktop toolchain boundaries

**Description:** Record the pre-change core and GUI test results, repair any pre-existing lockfile incompatibility that prevents the declared Rust 1.75 core MSRV from building, confirm the required toolchain split, and create the desktop package/crate scripts without changing the existing CLI.

**Acceptance criteria:**
- [ ] Existing Rust core tests are measured with Rust 1.75; any dependency/lockfile incompatibility blocking that declared MSRV is corrected and the tests pass. Existing GUI Node tests are measured before shell code is added.
- [ ] Desktop wrapper manifest requires Rust 1.90 or newer and depends on the root crate by path; root crate MSRV remains 1.75.
- [ ] `desktop` scripts expose `test`, `tauri:dev`, and `tauri:build`; the documented commands match the scripts.

**Verification:** `cargo +1.75 test --quiet`; `node --test gui/avatar-math.test.js gui/clip-player-math.test.js gui/pages.test.js`; inspect `rust-version` and run the wrapper checks with Rust 1.90+. If a local toolchain is unavailable, report it as blocked and obtain the result from a suitable CI/host before claiming completion.

**Dependencies:** None  
**Files likely touched:** root `Cargo.toml`/`Cargo.lock` for the verified MSRV repair; `desktop/package.json`, `desktop/.npmrc`, `desktop/src-tauri/Cargo.toml`, `.gitignore`, generated lock/config files  
**Estimated scope:** Medium

#### Task 2: Configure a local-only, least-privilege app window

**Description:** Configure the Tauri main window to load bundled assets, deny remote connections, and carry no plugin permissions. Restrict its app-command manifest to `get_app_info`. Add automated checks for the local asset policy, command allowlist, and capability file.

**Acceptance criteria:**
- [ ] The main window loads only packaged local assets and has an explicit content security policy that permits only required local asset/IPC schemes.
- [ ] Tauri's build-time app command manifest allows only `get_app_info`; the capability declares no plugin permissions, and no plugins are registered.
- [ ] Tests fail if an external origin, plugin permission, or additional command is introduced.

**Verification:** Focused desktop tests; review generated Tauri configuration and capability manifest. Confirm that forbidden filesystem, shell, HTTP, opener, and autostart permissions are absent.

**Dependencies:** Task 1  
**Files likely touched:** `desktop/src-tauri/build.rs`, `desktop/src-tauri/tauri.conf.json`, `desktop/src-tauri/capabilities/main.json`, desktop configuration tests, package scripts  
**Estimated scope:** Medium

### Checkpoint: Foundation

- [ ] Baselines are recorded without weakening or skipping existing tests.
- [ ] The core and desktop wrapper toolchain requirements are checked independently.
- [ ] Offline policy and the IPC/permission allowlist are explicit and tested.

### Phase 2: Native bridge and app window

#### Task 3: Add the typed app-information bridge and safe diagnostics

**Description:** Implement the sole shell command and its typed response, register only that command, and establish the shell's payload-free logging/error policy.

**Acceptance criteria:**
- [ ] `get_app_info` returns only non-sensitive app metadata using the project's serialization conventions.
- [ ] Frontend command failures reject with the documented serializable error shape; no empty success fallback is introduced.
- [ ] Shell startup/bridge diagnostics contain only event names and error categories; tests verify draft/key sentinel values never appear in captured diagnostics.

**Verification:** Rust unit tests for response serialization, command behavior, and diagnostic redaction; wrapper test/build with Rust 1.90+.

**Dependencies:** Task 2  
**Files likely touched:** `desktop/src-tauri/src/main.rs`, `desktop/src-tauri/src/commands.rs`, `desktop/src-tauri/src/diagnostics.rs` (or equivalent focused modules)  
**Estimated scope:** Medium

#### Task 4: Build the local frontend shell and typed adapter

**Description:** Add the first app screen and a single JavaScript API adapter that invokes `get_app_info`; keep all UI resources local and accessible.

**Acceptance criteria:**
- [ ] The bundled app opens its own native window and displays app metadata through the typed adapter.
- [ ] No frontend code invokes Rust commands outside the adapter or references external network resources.
- [ ] The screen is keyboard-operable and has no critical or serious accessibility findings under the agreed checker.

**Verification:** Node tests for the adapter/UI contract; `npm --prefix desktop test`; `npm --prefix desktop run check:a11y`; `npm --prefix desktop run tauri:dev` on Windows.

**Dependencies:** Task 3  
**Files likely touched:** `desktop/src/index.html`, `desktop/src/main.js`, `desktop/src/api.js`, `desktop/src/styles.css`, focused frontend tests  
**Estimated scope:** Medium

### Checkpoint: Shell integration

- [ ] The app starts without network access, renders its local UI, and completes `get_app_info`.
- [ ] No remote request is observed during the network-disabled smoke test.
- [ ] Rust, Node, and accessibility checks pass; failures are reported rather than suppressed.

### Phase 3: Cross-platform verification

#### Task 5: Verify supported native hosts and document runtime prerequisites

**Description:** Run target-native builds and smoke tests for Windows, macOS, and Linux; document commands and the Linux WebKitGTK runtime requirement. Do not bundle a browser engine.

**Acceptance criteria:**
- [ ] Native build and local-window smoke evidence exists for each supported OS; no platform is marked verified based only on another OS's build.
- [ ] Linux WebKitGTK runtime prerequisites and per-platform build prerequisites are documented.
- [ ] The app works with network access disabled and no external browser is launched.

**Verification:** On each OS, run `npm --prefix desktop run tauri:build` and `npm --prefix desktop run tauri:dev`; disable network access for the smoke test and verify the local UI and `get_app_info` still work. Run core tests with Rust 1.75 and desktop checks with Rust 1.90+.

**Dependencies:** Task 4  
**Files likely touched:** `desktop/README.md` or the project README  
**Estimated scope:** Small

### Checkpoint: Desktop shell complete

- [ ] All task acceptance criteria pass on the target operating systems.
- [ ] The existing CLI contract, core MSRV, encrypted data directory, and offline behavior are preserved.
- [ ] No shell feature exposes keys, drafts, arbitrary filesystem access, or process execution.
- [ ] Missing release-wide gates (coverage, performance baselines, and any remaining quality checkers) remain explicit blockers for release; they are not reported as passing.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Tauri's current MSRV exceeds the core's declared MSRV | Existing CLI/core users could be forced to upgrade | Keep the Tauri wrapper in a separate crate and test each crate with its own toolchain |
| A local bundle still attempts network access | App may fail offline or leak activity | Restrict webview connections, test configuration for remote origins, and run a network-disabled native smoke test |
| Capability configuration may accidentally widen frontend access | Sensitive local data or processes could be exposed | Register one shell command, no plugins/permissions, and test the exact allowlists |
| Native webviews differ by OS | A Windows-only success could hide platform failures | Require native build and smoke evidence on Windows, macOS, and Linux |
| Logging can expose future sensitive command payloads | Drafts or recovery keys could enter logs | Assign shared shell diagnostics ownership and test redaction using sentinels |

## Open questions

- Installer formats and minimum supported OS versions are deferred to the `installers` capability.
- Package size, startup, and memory thresholds are measured per OS before numeric budgets are set.

