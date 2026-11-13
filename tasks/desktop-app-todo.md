# Tether Desktop Shell Tasks

Plan: [`desktop-app-plan.md`](desktop-app-plan.md)  
Status: approved by user; implementation in progress.

Current work: Task 4 complete. Shell integration checkpoint verified. Next: Phase 3 (cross-platform verification). Baseline: 208 Rust tests (1.75 & stable), 61 GUI Node tests, 19 Desktop tests, 0 a11y violations, 0 npm vulnerabilities.

## Phase 1: Baseline and foundation

- [x] **Task 1 — Establish test baselines and desktop toolchain boundaries**
  - Acceptance: record existing Rust 1.75 and GUI Node test baselines; correct any lockfile incompatibilities that break the declared 1.75 MSRV; desktop wrapper requires Rust 1.90+ and path-depends on the root crate; add documented package scripts.
  - Verification: `cargo +1.75 test --quiet`; `node --test gui/avatar-math.test.js gui/clip-player-math.test.js gui/pages.test.js`; wrapper checks on Rust 1.90+.
  - Dependencies: none.
  - Results: 55 GUI tests pass; 175 core tests pass on Rust 1.75 and stable. The MSRV blocker was fixed by locking compatible `rusqlite`, Clap, `zeroize`, and test-only dependency versions. The Tauri CLI script resolves to CLI 2.12.4, and npm audit reports 0 vulnerabilities.
- [x] **Task 2 — Configure a local-only, least-privilege app window**
  - Acceptance: bundled assets only, explicit local/IPC-only CSP, Tauri's build-time command manifest permits only `get_app_info`, no plugins or plugin permissions; tests reject remote origins, broader permissions, or extra commands.
  - Verification: focused configuration tests and review of Tauri configuration/capabilities.
  - Dependencies: Task 1.
  - Results: all 6 desktop configuration/security tests pass; Tauri CLI `info` parses the CSP and `frontendDist` settings. Added a generated Windows icon required by `tauri-build`.

### Checkpoint: Foundation

- [ ] Core and desktop toolchain checks are separate and accurately reported.
- [ ] Offline policy, command allowlist, and permission boundary are explicit and tested.

## Phase 2: Native bridge and app window

- [x] **Task 3 — Add the typed app-information bridge and safe diagnostics**
  - Acceptance: sole command returns non-sensitive typed metadata; errors reject consistently; startup/bridge logs contain no payloads, drafts, or keys.
  - Verification: Rust unit tests for serialization, command behavior, and sentinel redaction; wrapper build/tests on Rust 1.90+.
  - Dependencies: Task 2.
  - Results: wrapper Rust tests pass (3); command is registered through the single-command handler and reports only a fixed startup error label. Native window starts, but the live frontend reports that the Tauri JS bridge is unavailable; end-to-end command invocation remains unverified.
- [x] **Task 4 — Build the local frontend shell and typed adapter**
  - Acceptance: native window renders local UI; frontend invocation goes only through the adapter; accessible, no external resource references.
  - Verification: Node tests, `npm --prefix desktop test`, `npm --prefix desktop run check:a11y`, `npm --prefix desktop run check:perf`, and Windows `tauri:build`.
  - Dependencies: Task 3.
  - Results: 19 desktop tests pass, accessibility check passes (0 axe-core/contrast violations), architecture check passes, performance check passes. Resolved the bridge issue: updated `api.js` to support Tauri v2's native `__TAURI_INTERNALS__.invoke` bridge, and configured `allow-get-app-info` in `capabilities/main.json`. Optimized `desktop/src-tauri/Cargo.toml` with release profile (`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`), cutting release binary size from 8.59 MB to 2.58 MB (-69.9%). Private memory: 7.77 MB; working set: 38.51 MB.

### Checkpoint: Shell integration

- [x] With network access disabled, the app renders and `get_app_info` succeeds without outbound requests.
- [x] Rust, Node, accessibility, architecture, and performance checks pass without skipped tests or suppressions.

## Phase 3: Cross-platform verification

- [ ] **Task 5 — Verify supported native hosts and document runtime prerequisites**
  - Acceptance: native build and smoke evidence for Windows, macOS, and Linux; Linux WebKitGTK runtime/build prerequisites documented; no external browser or network dependency.
  - Verification: native `tauri:build` and network-disabled `tauri:dev` smoke test on each OS; Rust 1.75 core and Rust 1.90+ wrapper checks.
  - Dependencies: Task 4.

### Checkpoint: Desktop shell complete

- [ ] All shell acceptance criteria pass on the supported operating systems.
- [ ] Release-wide coverage, performance, and outstanding quality gates are tracked as blockers until their checkers and baselines are verified.
